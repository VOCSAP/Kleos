"""
Classify routes by category and target audience.

Inputs:
    docs/dev-notes/kleos-mcp-routes-inventory.csv (from extract-routes-inventory.py)

Outputs:
    docs/dev-notes/kleos-mcp-routes-classified.csv (input + category + target columns)

Rules:
- category: mapping from canonical name prefix to a high-level functional group.
- target:
  - admin scope -> "operator" (override)
  - gate/inbox/approvals prefixes -> "system" (kleos-sh handles)
  - llm-friendly prefixes (memory/context/brain/skill/graph/conversation/...) -> "llm-runtime"
  - everything else -> "operator"

Run from repo root:
    python3 docs/dev-notes/classify-routes.py
"""
import csv
import sys
from pathlib import Path

SRC = Path("docs/dev-notes/kleos-mcp-routes-inventory.csv")
OUT = Path("docs/dev-notes/kleos-mcp-routes-classified.csv")


PREFIX_CATEGORY = {
    # Core memory and search
    "memory": "memory",
    "search": "memory",
    "batch": "memory",
    "pack": "memory",
    "context": "context",
    # Skills cloud
    "skill": "skill",
    "skills": "skill",
    # Brain / intelligence / growth
    "brain": "brain",
    "intelligence": "brain",
    "growth": "brain",
    "fsrs": "brain",
    # Graph + structural
    "graph": "graph",
    "structural": "graph",
    # Conversations / sessions / episodes / scratchpad
    "conversations": "conversation",
    "episodes": "conversation",
    "sessions": "conversation",
    "handoffs": "conversation",
    "scratchpad": "conversation",
    # Identity / auth / users / agents / onboard
    "identity": "identity",
    "identities": "identity",
    "identity_keys": "identity",
    "auth_keys": "identity",
    "users": "identity",
    "agents": "identity",
    "onboard": "identity",
    # Approval workflow (kleos-sh)
    "gate": "approval",
    "inbox": "approval",
    "approvals": "approval",
    # Syntheos services (broca/axon/soma/thymus/loom/chiasm/personality/dispatch)
    "activity": "activity",
    "broca": "activity",
    "axon": "activity",
    "soma": "activity",
    "thymus": "activity",
    "loom": "activity",
    "chiasm": "activity",
    "personality": "activity",
    "dispatch": "activity",
    # Projects / multi-tenant
    "projects": "projects",
    "platform": "projects",
    "tasks": "projects",
    # Ingestion / import / fetch
    "ingestion": "ingestion",
    # Portability / backup
    "portability": "portability",
    # Errors / supervisor / observability
    "errors": "errors",
    "supervisor": "errors",
    # Security / policy / commerce / quotas
    "security": "security",
    "policy": "security",
    "commerce": "security",
    "audit": "audit",
    # Webhooks
    "webhooks": "webhooks",
    # Admin / schema / jobs / well-known / docs / mcp_schema / gui / prompts / grounding / artifacts / health
    "admin": "admin",
    "schema": "admin",
    "jobs": "admin",
    "well_known": "admin",
    "docs": "admin",
    "mcp_schema": "admin",
    "gui": "admin",
    "prompts": "admin",
    "grounding": "admin",
    "artifacts": "admin",
    "health": "admin",
}


LLM_RUNTIME_CATEGORIES = {
    "memory", "context", "skill", "brain", "graph", "conversation",
    "activity", "projects", "ingestion",
}

SYSTEM_CATEGORIES = {"approval"}


def classify(name: str, scope: str) -> tuple[str, str]:
    prefix = name.split(".", 1)[0]
    category = PREFIX_CATEGORY.get(prefix, "other")

    # Override: Admin scope is always operator-only, regardless of category.
    if scope == "Admin":
        return category, "operator"

    if category in SYSTEM_CATEGORIES:
        return category, "system"
    if category in LLM_RUNTIME_CATEGORIES:
        return category, "llm-runtime"
    return category, "operator"


def main() -> int:
    rows = []
    with SRC.open(encoding="utf-8", newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            category, target = classify(row["name"], row["scope"])
            row["category"] = category
            row["target"] = target
            rows.append(row)

    if not rows:
        print("ERROR: no rows read", file=sys.stderr)
        return 1

    fieldnames = ["name", "aliases", "method", "path", "scope", "category", "target", "description"]
    with OUT.open("w", encoding="utf-8", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames, quoting=csv.QUOTE_MINIMAL)
        writer.writeheader()
        writer.writerows(rows)

    # Summary
    from collections import Counter
    cat_count = Counter(r["category"] for r in rows)
    target_count = Counter(r["target"] for r in rows)
    cat_target = Counter((r["category"], r["target"]) for r in rows)

    print(f"Wrote {len(rows)} routes to {OUT}")
    print("\nBy category:")
    for cat, n in cat_count.most_common():
        print(f"  {cat:15s} {n:4d}")
    print("\nBy target:")
    for t, n in target_count.most_common():
        print(f"  {t:15s} {n:4d}")
    print("\nBy (category, target):")
    for (cat, t), n in sorted(cat_target.items()):
        print(f"  {cat:15s} {t:15s} {n:4d}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
