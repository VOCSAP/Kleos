#!/usr/bin/env bash
set -euo pipefail

# Encrypt existing plaintext tenant databases with SQLCipher.
# Run ONCE on the production host BEFORE deploying the tenant-encryption code.
#
# Usage:
#   ENGRAM_DB_KEY=<64-hex-chars> ./encrypt-tenant-dbs.sh /path/to/data
#
# Prerequisites:
#   - sqlcipher in PATH
#   - sqlite3 in PATH
#   - Server STOPPED (no open handles on tenant dbs)

DATA_DIR="${1:?Usage: $0 <data-dir>}"
TENANTS_DIR="$DATA_DIR/tenants"

if [ ! -d "$TENANTS_DIR" ]; then
    echo "ERROR: tenants directory not found: $TENANTS_DIR"
    exit 1
fi

if [ -z "${ENGRAM_DB_KEY:-}" ]; then
    echo "ERROR: ENGRAM_DB_KEY not set. Export the 64-char hex key first."
    exit 1
fi

if [ ${#ENGRAM_DB_KEY} -ne 64 ]; then
    echo "ERROR: ENGRAM_DB_KEY must be exactly 64 hex characters (32 bytes). Got ${#ENGRAM_DB_KEY}."
    exit 1
fi

PRAGMA_KEY="\"x'${ENGRAM_DB_KEY}'\""

command -v sqlcipher >/dev/null 2>&1 || { echo "ERROR: sqlcipher not in PATH"; exit 1; }
command -v sqlite3 >/dev/null 2>&1 || { echo "ERROR: sqlite3 not in PATH"; exit 1; }

MIGRATED=0
SKIPPED=0
FAILED=0

for db_file in "$TENANTS_DIR"/*/kleos.db; do
    [ -f "$db_file" ] || continue

    tenant_dir="$(dirname "$db_file")"
    tenant_id="$(basename "$tenant_dir")"

    # Check if already encrypted by trying to read with plain sqlite3
    if ! sqlite3 "$db_file" "SELECT count(*) FROM sqlite_master;" >/dev/null 2>&1; then
        echo "SKIP $tenant_id -- already encrypted (or corrupt)"
        SKIPPED=$((SKIPPED + 1))
        continue
    fi

    echo "MIGRATING $tenant_id ..."

    backup="$db_file.plaintext.bak"
    encrypted="$db_file.encrypted"

    # 1. Backup the original
    cp "$db_file" "$backup"

    # 2. Open the plaintext db with sqlcipher (no key), attach an encrypted
    #    clone, and use sqlcipher_export() to copy everything including
    #    virtual tables and sequences.
    rm -f "$encrypted"
    export_out=$(sqlcipher "$db_file" <<ENDSQL 2>&1
ATTACH DATABASE '$encrypted' AS encrypted KEY $PRAGMA_KEY;
SELECT sqlcipher_export('encrypted');
DETACH DATABASE encrypted;
ENDSQL
    )
    export_rc=$?
    if [ $export_rc -ne 0 ]; then
        echo "  FAILED: sqlcipher_export failed for $tenant_id"
        echo "  $export_out"
        rm -f "$encrypted"
        FAILED=$((FAILED + 1))
        continue
    fi

    # 4. Verify the encrypted db can be opened with the key
    row_count=$(sqlcipher "$encrypted" <<ENDSQL 2>&1
PRAGMA key = $PRAGMA_KEY;
SELECT count(*) FROM sqlite_master;
ENDSQL
    )
    if [ $? -ne 0 ] || [ -z "$row_count" ]; then
        echo "  FAILED: encrypted db verification failed for $tenant_id"
        rm -f "$encrypted"
        FAILED=$((FAILED + 1))
        continue
    fi

    # 5. Verify plain sqlite3 CANNOT read the encrypted db
    if sqlite3 "$encrypted" "SELECT count(*) FROM sqlite_master;" >/dev/null 2>&1; then
        echo "  FAILED: encrypted db is still readable without key for $tenant_id"
        rm -f "$encrypted"
        FAILED=$((FAILED + 1))
        continue
    fi

    # 6. Atomic swap
    mv "$encrypted" "$db_file"

    # Also handle WAL/SHM files from the old plaintext db
    rm -f "$db_file-wal" "$db_file-shm"

    echo "  OK -- backup at $backup"
    MIGRATED=$((MIGRATED + 1))
done

echo ""
echo "Done: $MIGRATED migrated, $SKIPPED skipped, $FAILED failed"

if [ $FAILED -gt 0 ]; then
    echo "WARNING: $FAILED databases failed migration. Check output above."
    exit 1
fi
