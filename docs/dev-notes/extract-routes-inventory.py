"""
Extract kleos-client/src/routes.rs ROUTES into a CSV inventory.

Output columns: name, aliases, method, path, scope, description
Aliases are joined with '|' (no commas to keep CSV clean).
description is escaped (double quotes doubled).

Run from repo root:
    python3 docs/dev-notes/extract-routes-inventory.py
"""
import re
import csv
import sys
from pathlib import Path

SRC = Path("kleos-client/src/routes.rs")
OUT = Path("docs/dev-notes/kleos-mcp-routes-inventory.csv")

ROUTE_BLOCK = re.compile(
    r'Route\s*\{\s*'
    r'name:\s*"([^"]+)",\s*'
    r'aliases:\s*&\[([^\]]*)\],\s*'
    r'method:\s*Method::(\w+),\s*'
    r'path:\s*"([^"]+)",\s*'
    r'scope:\s*Scope::(\w+),\s*'
    r'description:\s*"((?:[^"\\]|\\.)*)",\s*'
    r'input_schema:',
    re.MULTILINE,
)

ALIAS_RE = re.compile(r'"([^"]+)"')


def main() -> int:
    text = SRC.read_text(encoding="utf-8")
    matches = list(ROUTE_BLOCK.finditer(text))
    if not matches:
        print("ERROR: no Route block matched", file=sys.stderr)
        return 1

    rows = []
    for m in matches:
        name, aliases_raw, method, path, scope, description = m.groups()
        aliases = ALIAS_RE.findall(aliases_raw)
        rows.append({
            "name": name,
            "aliases": "|".join(aliases),
            "method": method.upper(),
            "path": path,
            "scope": scope,
            "description": description,
        })

    OUT.parent.mkdir(parents=True, exist_ok=True)
    with OUT.open("w", encoding="utf-8", newline="") as f:
        writer = csv.DictWriter(
            f,
            fieldnames=["name", "aliases", "method", "path", "scope", "description"],
            quoting=csv.QUOTE_MINIMAL,
        )
        writer.writeheader()
        writer.writerows(rows)

    print(f"Wrote {len(rows)} routes to {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
