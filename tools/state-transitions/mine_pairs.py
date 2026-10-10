#!/usr/bin/env python3
"""Step 0 of the state-transition plan: mine candidate memory pairs to label.

Read-only. Opens a Kleos SQLite database, finds pairs of memories that are
semantically close (bge-m3 cosine), written at different times, in the same
user + space, and exports a stratified sample for hand labelling. A second
subcommand reports the label distribution once the CSV is filled in.

Usage:
  python -I mine_pairs.py mine   --db kleos.db [--user-id 1] [--out-dir DIR]
  python -I mine_pairs.py report --labels DIR/to_label.csv

Encrypted (SQLCipher) databases: either pass --key-hex (needs the `sqlcipher3`
Python module) or export a plaintext copy first, see tools/state-transitions/PLAN.md.

Output files contain memory content. The default output directory sits under
docs/dev-notes/, which is gitignored: keep it there, never commit the data.

Requires numpy (pip install numpy). Python >= 3.9.
"""

from __future__ import annotations

import argparse
import csv
import json
import math
import random
import re
import sqlite3
import sys
import unicodedata
from collections import Counter, defaultdict
from datetime import datetime, timezone
from pathlib import Path

LABELS = (
    "resout",        # B ends a problem / state described in A (A was true, is now over)
    "remplace",      # B gives a new value / state of the same subject (version, config, decision)
    "contredit",     # B asserts the opposite of A with no temporal transition: one is wrong
    "complete",      # B adds information; A stays true and current
    "doublon",       # same information
    "sans_rapport",  # different subjects despite the similarity
    "incertain",     # cannot decide from the two texts
)
TRANSITION_LABELS = ("resout", "remplace", "contredit")

DEFAULT_EXCLUDED_CATEGORIES = ("activity", "growth")
EMBED_DIM = 1024
CONTENT_CAP = 1500

# Transition markers. Used ONLY to stratify the sample (so rare positives are
# not drowned by random pairs) and to measure, after labelling, how much a
# pure keyword rule would catch. They are not a decision rule.
MARKERS = {
    "fr": [
        "resolu", "resolue", "corrige", "corrigee", "repare", "reparee", "fixe",
        "regle", "reglee", "debloque", "contourne", "n'est plus", "ne fonctionne plus",
        "desormais", "dorenavant", "maintenant", "remplace", "remplacee", "migre",
        "migree", "abandonne", "abandonnee", "supprime", "retire", "deprecie",
        "obsolete", "fonctionne", "refonctionne", "a ete mis a jour", "passe a",
        "passe en", "au lieu de", "plus d'actualite", "ferme", "cloture", "termine",
    ],
    "en": [
        "resolved", "fixed", "repaired", "solved", "unblocked", "worked around",
        "no longer", "not anymore", "now", "from now on", "replaced", "migrated",
        "switched", "deprecated", "removed", "obsolete", "works again", "working",
        "updated to", "upgraded", "downgraded", "instead of", "closed", "done",
        "superseded", "reverted", "rolled back",
    ],
}

STOPWORDS = {
    "fr": {"le", "la", "les", "des", "est", "une", "un", "pour", "dans", "pas",
           "que", "qui", "sur", "avec", "et", "du", "au", "ce", "sont", "par"},
    "en": {"the", "is", "and", "of", "to", "in", "for", "with", "not", "that",
           "on", "was", "are", "be", "this", "it", "by", "from", "as", "at"},
}

MONTHS = (
    r"janvier|fevrier|mars|avril|mai|juin|juillet|aout|septembre|octobre|novembre|decembre"
    r"|janv|fevr|avr|juil|sept|oct|nov|dec"
    r"|january|february|march|april|may|june|july|august|september|october|november|december"
    r"|jan|feb|mar|apr|jun|jul|aug|sep"
)
DATE_PATTERNS = [
    re.compile(r"\b\d{4}-\d{2}-\d{2}\b"),
    re.compile(r"\b\d{1,2}[/.]\d{1,2}[/.]\d{2,5}\b"),
    re.compile(rf"\b\d{{1,2}}(?:er)?\s+(?:{MONTHS})\.?\s+\d{{4}}\b"),
    re.compile(rf"\b(?:{MONTHS})\.?\s+\d{{1,2}}(?:st|nd|rd|th)?,?\s+\d{{4}}\b"),
]


def fold(text: str) -> str:
    """Lowercase and strip accents so markers match both 'résolu' and 'resolu'."""
    decomposed = unicodedata.normalize("NFKD", text.lower())
    return "".join(c for c in decomposed if not unicodedata.combining(c)).replace("’", "'")


MARKER_RES = {
    lang: re.compile(r"(?<![\w])(?:" + "|".join(re.escape(m) for m in words) + r")(?![\w])")
    for lang, words in MARKERS.items()
}


def find_markers(folded: str) -> list[str]:
    hits: list[str] = []
    for regex in MARKER_RES.values():
        hits.extend(m.group(0) for m in regex.finditer(folded))
    return sorted(set(hits))


def find_dates(folded: str) -> list[str]:
    out: list[str] = []
    for regex in DATE_PATTERNS:
        out.extend(m.group(0) for m in regex.finditer(folded))
    return sorted(set(out))


def guess_lang(folded: str, stored: str | None) -> str:
    if stored:
        return stored.lower()[:2]
    words = re.findall(r"[a-z']+", folded)
    if not words:
        return "und"
    fr = sum(w in STOPWORDS["fr"] for w in words)
    en = sum(w in STOPWORDS["en"] for w in words)
    if fr == en:
        return "und"
    return "fr" if fr > en else "en"


def parse_ts(value: str | None) -> float | None:
    if not value:
        return None
    text = value.strip().replace("Z", "+00:00")
    for candidate in (text, text.replace(" ", "T", 1)):
        try:
            dt = datetime.fromisoformat(candidate)
        except ValueError:
            continue
        if dt.tzinfo is None:
            dt = dt.replace(tzinfo=timezone.utc)
        return dt.timestamp()
    return None


def connect(db_path: str, key_hex: str | None):
    if key_hex:
        try:
            import sqlcipher3  # type: ignore
        except ImportError:
            sys.exit("--key-hex needs the sqlcipher3 module; or export a plaintext copy (see PLAN.md)")
        conn = sqlcipher3.connect(db_path)
        conn.execute(f"PRAGMA key = \"x'{key_hex}'\";")
        conn.execute("PRAGMA query_only = ON;")
        return conn
    uri = Path(db_path).resolve().as_uri() + "?mode=ro"
    conn = sqlite3.connect(uri, uri=True)
    try:
        conn.execute("SELECT count(*) FROM sqlite_master").fetchone()
    except sqlite3.DatabaseError:
        sys.exit(f"{db_path}: not a readable SQLite file (SQLCipher-encrypted? see PLAN.md)")
    return conn


def table_columns(conn, table: str) -> set[str]:
    return {row[1] for row in conn.execute(f"PRAGMA table_info({table})")}


def load_memories(conn, args) -> list[dict]:
    cols = table_columns(conn, "memories")
    if not cols:
        sys.exit("no `memories` table in this database")
    emb_col = "embedding_vec_1024" if "embedding_vec_1024" in cols else "embedding"
    wanted = ["id", "content", "created_at", emb_col]
    optional = ["user_id", "space_id", "category", "lang", "root_memory_id", "source"]
    select = wanted + [c for c in optional if c in cols]

    where = [f"{emb_col} IS NOT NULL"]
    if "is_forgotten" in cols:
        where.append("is_forgotten = 0")
    if "status" in cols:
        where.append("status != 'pending'")
    if "is_latest" in cols and not args.include_non_latest:
        where.append("is_latest = 1")
    if "is_archived" in cols and not args.include_archived:
        where.append("is_archived = 0")
    params: list = []
    if args.user_id is not None and "user_id" in cols:
        where.append("user_id = ?")
        params.append(args.user_id)
    if args.space_id is not None and "space_id" in cols:
        where.append("space_id = ?")
        params.append(args.space_id)
    excluded = [c for c in args.exclude_categories.split(",") if c]
    if excluded and "category" in cols:
        where.append(f"category NOT IN ({','.join('?' * len(excluded))})")
        params.extend(excluded)

    sql = f"SELECT {', '.join(select)} FROM memories WHERE {' AND '.join(where)} ORDER BY id"
    if args.max_memories:
        sql += f" LIMIT {int(args.max_memories)}"

    rows = []
    skipped = Counter()
    for raw in conn.execute(sql, params):
        rec = dict(zip(select, raw))
        blob = rec.pop(emb_col)
        if not isinstance(blob, (bytes, bytearray)) or len(blob) != EMBED_DIM * 4:
            skipped["bad_embedding"] += 1
            continue
        ts = parse_ts(rec.get("created_at"))
        if ts is None:
            skipped["bad_created_at"] += 1
            continue
        rec["_blob"] = bytes(blob)
        rec["_ts"] = ts
        rec["content"] = rec.get("content") or ""
        rows.append(rec)
    return rows, skipped


def load_links(conn) -> dict[tuple[int, int], list[str]]:
    if not table_columns(conn, "memory_links"):
        return {}
    links: dict[tuple[int, int], list[str]] = defaultdict(list)
    for src, dst, typ in conn.execute("SELECT source_id, target_id, type FROM memory_links"):
        links[(min(src, dst), max(src, dst))].append(str(typ))
    return links


def sim_band(sim: float) -> str:
    if sim < 0.80:
        return "a_<0.80"
    if sim < 0.90:
        return "b_0.80-0.90"
    return "c_>=0.90"


def mine(args) -> None:
    try:
        import numpy as np
    except ImportError:
        sys.exit("numpy is required: pip install numpy")

    conn = connect(args.db, args.key_hex)
    memories, skipped = load_memories(conn, args)
    links = load_links(conn)
    conn.close()
    if not memories:
        sys.exit("no memory matched the filters")

    for m in memories:
        folded = fold(m["content"])
        m["_lang"] = guess_lang(folded, m.get("lang"))
        m["_markers"] = find_markers(folded)
        m["_dates"] = find_dates(folded)

    groups: dict[tuple, list[int]] = defaultdict(list)
    for idx, m in enumerate(memories):
        groups[(m.get("user_id"), m.get("space_id"))].append(idx)

    gap = args.min_gap_hours * 3600.0
    pairs = []
    for key, idxs in groups.items():
        if len(idxs) < 2:
            continue
        mat = np.frombuffer(b"".join(memories[i]["_blob"] for i in idxs), dtype="<f4")
        mat = mat.reshape(len(idxs), EMBED_DIM).astype(np.float32)
        norms = np.linalg.norm(mat, axis=1, keepdims=True)
        norms[norms == 0] = 1.0
        mat /= norms
        ts = np.array([memories[i]["_ts"] for i in idxs])
        for start in range(0, len(idxs), args.block):
            sims = mat[start:start + args.block] @ mat.T
            for row, newer_local in enumerate(range(start, min(start + args.block, len(idxs)))):
                s = sims[row]
                older_mask = ts < ts[newer_local] - gap
                band_mask = (s >= args.min_sim) & (s <= args.max_sim)
                cand = np.nonzero(older_mask & band_mask)[0]
                if cand.size == 0:
                    continue
                top = cand[np.argsort(-s[cand])[: args.k]]
                newer = memories[idxs[newer_local]]
                for older_local in top:
                    older = memories[idxs[int(older_local)]]
                    root_o = older.get("root_memory_id") or older["id"]
                    root_n = newer.get("root_memory_id") or newer["id"]
                    if root_o == root_n:
                        continue  # same version chain: already linked by design
                    pairs.append((older, newer, float(s[older_local])))

    stats = {
        "memories_loaded": len(memories),
        "memories_skipped": dict(skipped),
        "groups": len(groups),
        "candidate_pairs": len(pairs),
        "params": {k: v for k, v in vars(args).items() if k not in ("func", "key_hex")},
        "lang_memories": dict(Counter(m["_lang"] for m in memories)),
        "memories_with_dates_in_text": sum(1 for m in memories if m["_dates"]),
        "memories_with_marker": sum(1 for m in memories if m["_markers"]),
    }

    records = []
    for n, (older, newer, sim) in enumerate(pairs):
        key = (min(older["id"], newer["id"]), max(older["id"], newer["id"]))
        records.append({
            "pair_id": f"p{n:06d}",
            "cosine": round(sim, 4),
            "sim_band": sim_band(sim),
            "marker_newer": "yes" if newer["_markers"] else "no",
            "markers_newer": " | ".join(newer["_markers"]),
            "lang_pair": f"{older['_lang']}-{newer['_lang']}",
            "gap_days": round((newer["_ts"] - older["_ts"]) / 86400.0, 2),
            "existing_links": " | ".join(sorted(links.get(key, []))),
            "older_id": older["id"],
            "older_created_at": older.get("created_at"),
            "older_category": older.get("category"),
            "older_dates_in_text": " | ".join(older["_dates"]),
            "older_content": older["content"][:CONTENT_CAP],
            "newer_id": newer["id"],
            "newer_created_at": newer.get("created_at"),
            "newer_category": newer.get("category"),
            "newer_dates_in_text": " | ".join(newer["_dates"]),
            "newer_content": newer["content"][:CONTENT_CAP],
        })

    stats["pairs_by_band"] = dict(Counter(r["sim_band"] for r in records))
    stats["pairs_by_marker"] = dict(Counter(r["marker_newer"] for r in records))
    stats["pairs_by_lang_pair"] = dict(Counter(r["lang_pair"] for r in records))
    stats["pairs_with_existing_link"] = sum(1 for r in records if r["existing_links"])

    sample = stratified_sample(records, args.sample, args.seed)
    stats["sampled"] = len(sample)
    stats["sampled_by_stratum"] = dict(Counter(f"{r['sim_band']}/{r['marker_newer']}" for r in sample))

    out = Path(args.out_dir)
    out.mkdir(parents=True, exist_ok=True)
    with open(out / "pairs_all.jsonl", "w", encoding="utf-8") as fh:
        for r in records:
            fh.write(json.dumps(r, ensure_ascii=False) + "\n")
    fields = ["pair_id", "label", "notes"] + [k for k in records[0] if k != "pair_id"] if records else []
    with open(out / "to_label.csv", "w", encoding="utf-8-sig", newline="") as fh:
        writer = csv.DictWriter(fh, fieldnames=fields, delimiter=";")
        writer.writeheader()
        for r in sample:
            writer.writerow({"label": "", "notes": "", **r})
    with open(out / "stats.json", "w", encoding="utf-8") as fh:
        json.dump(stats, fh, ensure_ascii=False, indent=2)

    print(json.dumps(stats, ensure_ascii=False, indent=2))
    print(f"\nwrote {out / 'to_label.csv'} ({len(sample)} pairs to label)")
    print("labels: " + ", ".join(LABELS))


def stratified_sample(records: list[dict], size: int, seed: int) -> list[dict]:
    """Even allocation across (similarity band x marker) strata, so the rare
    marked/high-similarity pairs are not drowned by the common ones."""
    rng = random.Random(seed)
    strata: dict[str, list[dict]] = defaultdict(list)
    for r in records:
        strata[f"{r['sim_band']}/{r['marker_newer']}"].append(r)
    for bucket in strata.values():
        rng.shuffle(bucket)
    picked: list[dict] = []
    keys = sorted(strata)
    while len(picked) < size and any(strata[k] for k in keys):
        for k in keys:
            if strata[k] and len(picked) < size:
                picked.append(strata[k].pop())
    rng.shuffle(picked)
    return picked


def report(args) -> None:
    with open(args.labels, encoding="utf-8-sig", newline="") as fh:
        rows = list(csv.DictReader(fh, delimiter=";"))
    labelled = [r for r in rows if (r.get("label") or "").strip()]
    bad = [r["pair_id"] for r in labelled if r["label"].strip() not in LABELS]
    if bad:
        sys.exit(f"unknown labels on {bad[:10]} (allowed: {', '.join(LABELS)})")
    for r in labelled:
        r["label"] = r["label"].strip()
        r["_pos"] = r["label"] in TRANSITION_LABELS

    def dist(key: str) -> dict:
        table: dict[str, Counter] = defaultdict(Counter)
        for r in labelled:
            table[r[key]][r["label"]] += 1
        return {k: {"n": sum(c.values()), "transition_rate": round(
            sum(c[l] for l in TRANSITION_LABELS) / max(1, sum(c.values())), 3), **dict(c)}
            for k, c in sorted(table.items())}

    pos = [r for r in labelled if r["_pos"]]
    marked = [r for r in labelled if r["marker_newer"] == "yes"]
    marker_tp = sum(1 for r in marked if r["_pos"])
    cos_pos = sorted(float(r["cosine"]) for r in pos)

    result = {
        "rows": len(rows),
        "labelled": len(labelled),
        "labels": dict(Counter(r["label"] for r in labelled)),
        "by_sim_band": dist("sim_band"),
        "by_marker": dist("marker_newer"),
        "by_lang_pair": dist("lang_pair"),
        "marker_rule": {
            "precision": round(marker_tp / max(1, len(marked)), 3),
            "recall": round(marker_tp / max(1, len(pos)), 3),
            "note": "how a pure keyword rule would do on this sample (stratified, not the population)",
        },
        "transition_cosine_quantiles": {
            q: cos_pos[min(len(cos_pos) - 1, math.floor(q * len(cos_pos)))] if cos_pos else None
            for q in (0.05, 0.10, 0.25, 0.50)
        },
        "event_dates_in_positive_pairs": sum(
            1 for r in pos if r["older_dates_in_text"] or r["newer_dates_in_text"]),
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))
    if args.out:
        Path(args.out).write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")


def main() -> None:
    default_out = Path(__file__).resolve().parents[2] / "docs" / "dev-notes" / "state-transitions" / "data"
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(required=True)

    p = sub.add_parser("mine", help="mine candidate pairs and export a sample to label")
    p.add_argument("--db", required=True, help="path to the Kleos SQLite file (plaintext, or --key-hex)")
    p.add_argument("--key-hex", help="SQLCipher raw key (64 hex chars); needs the sqlcipher3 module")
    p.add_argument("--user-id", type=int)
    p.add_argument("--space-id", type=int)
    p.add_argument("--min-sim", type=float, default=0.70)
    p.add_argument("--max-sim", type=float, default=0.985, help="above this, treat as duplicate and skip")
    p.add_argument("--k", type=int, default=5, help="older neighbours kept per memory")
    p.add_argument("--min-gap-hours", type=float, default=1.0)
    p.add_argument("--sample", type=int, default=150)
    p.add_argument("--seed", type=int, default=42)
    p.add_argument("--block", type=int, default=512)
    p.add_argument("--max-memories", type=int, default=0)
    p.add_argument("--exclude-categories", default=",".join(DEFAULT_EXCLUDED_CATEGORIES))
    p.add_argument("--include-non-latest", action="store_true")
    p.add_argument("--include-archived", action="store_true")
    p.add_argument("--out-dir", default=str(default_out))
    p.set_defaults(func=mine)

    r = sub.add_parser("report", help="summarise a labelled to_label.csv")
    r.add_argument("--labels", required=True)
    r.add_argument("--out", help="also write the JSON summary here")
    r.set_defaults(func=report)

    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
