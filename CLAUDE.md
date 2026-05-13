# CLAUDE.md -- Kleos VOCSAP Fork

## Bugs connus et corrections pendantes

**`docs/dev-notes/credd-todo.md`** -- TODOs actifs sur kleos-credd :
- TODO-1 : timeout 408 sur `POST /secret` quand pas de YubiKey (cause : `yubikey::YubiKey::open()` bloque ~30s sur LXC sans PCSC). Fix : ajouter `KLEOS_IDENTITY_KEY` dans credd.env.
- TODO-2 : entree `[CRED:v3]` dans kleos-server a creer manuellement a chaque nouvel agent.
- TODO-3 : `cred` CLI necessite `KLEOS_ENCRYPTION_MODE=env KLEOS_DB_KEY=...` dans l'env pour ouvrir la DB chiffree.

**`docs/dev-notes/kleos-sh-todo.md`** -- TODOs actifs sur kleos-sh (client Windows) :
- TODO-1 : reformuler le message "gate unreachable" trompeur en cas de timeout HTTP (le serveur a juste mis le gate en hold pour approval -- pas un probleme reseau).
- TODO-2 : reduire `max_attempts` a 1 en mode exec pour les tools dans `TOOLS_REQUIRING_APPROVAL` (evite 4 entrees "DENIED/TIMEOUT" cote serveur par appel rate).
- TODO-3 : la GUI Svelte n'expose aucune route pour approuver les gates -- l'approval passe par `engram-approval-tui` (crate `kleos-approval-tui`). Voir wiki/Gate-and-Approvals.md.

---

## Règle absolue avant tout merge upstream

**Lire `docs/dev-notes/local-patches.md` avant chaque `git merge origin/main`.**

Ce repo est un fork local de Ghost-Frame/Kleos avec des patches locaux (Windows port,
embedding backend Ollama). Ces patches ne sont pas dans upstream et seront écrasés
ou perdus à chaque merge si on ne les vérifie pas.

### Checklist merge upstream

```bash
# 1. Vérifier quels patches sont encore absents d'upstream
git show origin/main:agent-forge/Cargo.toml | grep "cfg(windows)"
git show origin/main:kleos-sidecar/Cargo.toml | grep "cfg(windows)"
git show origin/main:kleos-approval-tui/Cargo.toml | grep "cfg(windows)"
git show origin/main:kleos-sh/src/main.rs | grep "cfg(not(unix))"
git show origin/main:kleos-cred/src/bin/derive-db-key.rs | grep "cfg(unix)"
git show origin/main:kleos-server/src/main.rs | grep "EMBEDDING_BACKEND"
git show origin/main:kleos-lib/src/auth.rs | grep "kleos_\|split_once"
git show origin/main:kleos-server/src/routes/gui/mod.rs | grep "starts_with.*spa"

# 2. Stasher les patches locaux avant merge
git stash push -m "local-patches" -- \
  agent-forge/Cargo.toml \
  kleos-sidecar/Cargo.toml \
  kleos-approval-tui/Cargo.toml \
  kleos-sh/src/main.rs \
  kleos-sh/src/gate.rs \
  kleos-sh/src/exec.rs \
  kleos-cred/src/bin/derive-db-key.rs \
  kleos-lib/src/embeddings/openai.rs \
  kleos-lib/src/auth.rs \
  kleos-server/src/main.rs \
  kleos-server/src/routes/gui/mod.rs

# 3. Merger
git merge origin/main --allow-unrelated-histories --no-commit
# Résoudre les conflits add/add avec --theirs pour les fichiers non-patchés
# puis git add + git commit

# 4. Re-appliquer les patches
git checkout stash@{0} -- <fichiers patchés>
git stash drop

# 5. Vérifier
cargo check -p kleos-server -p kleos-sh -p agent-forge -p kleos-cred -p kleos-sidecar
```

---

## Architecture du repo

Fork de Ghost-Frame/Kleos (anciennement Ghost-Frame/Engram). Pas de merge base
commune avec upstream -- le merge initial a été fait avec `--allow-unrelated-histories`.

**Patches locaux :** `docs/dev-notes/local-patches.md`
**Analyse Windows port :** `docs/dev-notes/windows-port-changes.md`
**Analyse kleos-sh :** `docs/dev-notes/kleos-sh-windows-port.md`

---

## Compilation par plateforme

```bash
# Windows (MSVC) -- binaires client
cargo build --release -p kleos-sh -p agent-forge -p kleos-sidecar -p kleos-cred

# Linux musl (WSL) -- binaires serveur
cargo build --release \
  --target x86_64-unknown-linux-musl \
  -p kleos-server -p kleos-cli -p kleos-mcp
```

Les binaires Windows vont dans `target/release/`.
Les binaires Linux vont dans `target/x86_64-unknown-linux-musl/release/`.

---

## Variables d'environnement -- serveur (deploy/kleos.env)

Toutes les vars `KLEOS_*` sont traduites automatiquement en `ENGRAM_*` au démarrage
par `kleos_lib::config::migrate_env_prefix()`. Les deux préfixes sont donc équivalents.

**Vars critiques pour ce déploiement :**

```env
# Embedding via Ollama (patch local -- non upstream)
KLEOS_EMBEDDING_BACKEND=openai
KLEOS_EMBEDDING_OPENAI_BASE_URL=http://192.168.10.16:11434/v1
KLEOS_EMBEDDING_OPENAI_API_KEY=<token>
KLEOS_EMBEDDING_OPENAI_MODEL=bge-m3
KLEOS_EMBEDDING_DIM=1024

# Recommandé : persistance des sessions entre redémarrages
# KLEOS_SESSION_KEY=<hex 32 octets -- générer avec : openssl rand -hex 32>
```

**Vars optionnelles nouvelles en v1.0.0 :**

| Variable | Défaut | Usage |
|---|---|---|
| `KLEOS_SESSION_KEY` | clé éphémère (avertissement log) | Persistance cookies session |
| `KLEOS_PUBLIC_URL` | `https://kleos.example.com` | Endpoint `/.well-known/` |
| `KLEOS_ORG_NAME` | `Kleos` | Nom affiché dans `/.well-known/` |
| `KLEOS_METRICS_TOKEN` | désactivé | Protège `GET /metrics` |

---

## Variables d'environnement -- client Windows

```env
# Utilisées par kleos-sh (gate hooks)
KLEOS_API_KEY=kleos_ca...
KLEOS_URL=http://192.168.10.21:4200

# Utilisées par kleos-sidecar (compression contexte Claude Code)
# compress_enabled=true par défaut -- OLLAMA_URL/MODEL obligatoires si actif
OLLAMA_URL=http://192.168.10.16:11434/v1/chat/completions  # format OpenAI-compatible
OLLAMA_MODEL=ministral-14b-q8-tools
KLEOS_SIDECAR_TOKEN=<token>

# Utilisées par kleos-mcp
KLEOS_MCP_BEARER_TOKEN=kleos_ca...

# Inutilisées par les binaires Windows actuels (server-side uniquement)
# KLEOS_LLM_URL / KLEOS_LLM_MODEL -> ENGRAM_LLM_URL/MODEL -> intelligence layer serveur
```

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

Rules:
- ALWAYS read graphify-out/GRAPH_REPORT.md before reading any source files, running grep/glob searches, or answering codebase questions. The graph is your primary map of the codebase.
- IF graphify-out/wiki/index.md EXISTS, navigate it instead of reading raw files
- For cross-module "how does X relate to Y" questions, prefer `graphify query "<question>"`, `graphify path "<A>" "<B>"`, or `graphify explain "<concept>"` over grep — these traverse the graph's EXTRACTED + INFERRED edges instead of scanning files
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).

## Graphify -- détection automatique

Au démarrage de chaque session dans un repo, vérifier si `graphify-out/graph.json` existe.
Si ce fichier est absent ET que le repo contient des fichiers source (.py, .ts, .rs, .go, .java, .cs, etc.) :
Signaler proactivement : "Ce repo n'a pas encore été analysé par Graphify. Pour améliorer
la navigation du code, lance : graphify extract . --backend ollama --model qwen2.5:14b"
Ne pas signaler si le repo est vide ou ne contient que de la config/docs.