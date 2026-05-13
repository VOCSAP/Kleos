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

**`docs/dev-notes/dreamer-llm-contract.md`** -- contrat Ollama du dreamer / intelligence layer (consulter pour tout switch de modele LLM, ajout de tests, ou migration LiteLLM) :
- Le code appelle `/api/generate` (Ollama native) et lit uniquement le champ JSON `response`. Il ignore `thinking` / `message.content`.
- Modeles **reasoning / thinking** (`qwen3.5:*`, `deepseek-r1:*`, etc.) renvoient `response=""` -> `validate_observation` echoue silencieusement -> phases dream muettes. A eviter.
- Modeles recommandes : `llama3.2:3b` (default code), `qwen2.5:7b-instruct`, `gemma2:2b`, `mistral:7b-instruct`. Non-thinking, 2-4 B params, `num_predict=300` suffit.
- Bug 2026-05-13 : `qwen3.5:4b` configure depuis 2026-05-07 -> aucune `growth_observation` stockee depuis. Fix : switch `KLEOS_LLM_MODEL` + `OLLAMA_MODEL` vers `llama3.2:3b`.
- TODO : ajouter smoke-test E2E (bash deploy ou `cargo test --ignored`) qui appelle Ollama avec la config en cours et valide `response` non-vide + `validate_observation`.

---

## Architecture du repo (depuis rebase v1.1.0 -- 2026-05-13)

Fork de Ghost-Frame/Kleos (anciennement Ghost-Frame/Engram). **Modele branche
topic** : `main` suit upstream/main exact (fast-forward only), `local/patches`
est la branche topic VOCSAP avec ~8 commits semantiques au-dessus de `main`.

Historiquement, le repo avait ete initialise avec `merge --allow-unrelated-histories`
(2026-05-09 -> 2026-05-12) avant d'etre reconstruit en branche topic propre
(2026-05-13). La nouvelle structure permet un `git rebase main` standard plutot
qu'un merge-stash-pop a chaque release upstream.

**Patches locaux :** `docs/dev-notes/local-patches.md` (statut + commits par patch)
**Audit rebase v1.1.0 :** `docs/dev-notes/v1.1.0-rebase-audit.md` (matrice 93 fichiers)
**Analyse Windows port :** `docs/dev-notes/windows-port-changes.md`
**Analyse kleos-sh :** `docs/dev-notes/kleos-sh-windows-port.md`

---

## Regle absolue avant integration d'une release upstream

**Lire `docs/dev-notes/local-patches.md` section "Procedure d'integration des
releases upstream" avant chaque `git rebase main` de la branche `local/patches`.**

### Checklist condensee

```bash
# 1. Sync upstream sur main
git fetch upstream main
git checkout main
git merge --ff-only upstream/main
git push origin main

# 2. Tags de backup
DATE=$(date +%Y-%m-%d)
git tag "backup/local-patches-before-rebase-$DATE" local/patches
git tag "backup/main-before-rebase-$DATE" main
git push origin --tags

# 3. Verifier quels patches sont absorbes upstream (cf. local-patches.md
#    section "Verification rapide" pour les greps detailles)

# 4. Rebase de la topic branch
git checkout local/patches
git rebase main
# Drop les commits absorbes upstream, adapter les commits qui touchent une
# API ayant change upstream (ex: refactor de struct extrait dans une crate).

# 5. Validation + push
cargo check --workspace --all-features
git push --force-with-lease origin local/patches
```

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