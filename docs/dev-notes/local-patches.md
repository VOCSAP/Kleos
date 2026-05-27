# Local Patches -- Kleos VOCSAP Fork

**Date de création :** 2026-05-11
**Dernière mise à jour :** 2026-05-18 (rebase v1.1.5)
**Contexte :** Ce fichier répertorie tous les changements locaux (non upstream) appliqués
sur la branche `local/patches` VOCSAP. À consulter impérativement avant tout merge ou
rebase depuis Ghost-Frame/Kleos pour identifier les conflits prévisibles et les
re-appliquer si perdus.

---

## Statut après rebase v1.1.5 (2026-05-18)

`local/patches` HEAD = `473f214`, 17 commits semantiques au-dessus de main = `80982dc`
(upstream/main qui inclut tags v1.1.3 / v1.1.4 / v1.1.5 sans bump Cargo.toml version,
qui reste 1.1.2).

| Patch | Statut v1.1.5 | Commit |
|---|---|---|
| 1 -- agent-forge/Cargo.toml cfg(windows) bundled rusqlite | ABSORBE depuis v1.1.2 (26e930b) | drop |
| 2, 10 -- Cargo.toml cfg(windows) sqlcipher (sidecar, approval-tui) | RE-APPLIQUE | `fe53eef` |
| 4 -- kleos-cred derive-db-key cfg(unix) | **ABSORBE upstream v1.1.5** (commit `133e783 fix(ci): cfg-gate Unix-only APIs for Windows cross-compilation`) | drop |
| 7 -- kleos-lib auth normalize_key kleos_ prefix | RE-APPLIQUE | `b4fef72` |
| 5/8B/8C -- embedding Ollama base_url + SPA prefix match | RE-APPLIQUE | `53d1d72` |
| 3, 9 -- kleos-sh Windows port + gate curl subprocess | RE-APPLIQUE (conflit textuel resolu : retrait du `#[cfg(unix)]` que upstream a ajoute sur `resolve_key_via_credd` -- notre wrapper dispatch deja entre `_socket` et `_tcp`) | `2cc1977` -> `d3c4716` apres rebase |
| 11 -- kleos-sidecar namespace OLLAMA env vars | RE-APPLIQUE (conflit textuel resolu, upstream a adopte la convention `KLEOS_SIDECAR_*` pour son nouveau `GATE_MODEL`) | `fe023d4` -> `4c5d377` |
| 12 -- hooks bundle preservation | RE-APPLIQUE, preserve byte-a-byte | `e86bb03` -> `10c829e` |
| **13 -- fix(broca,activity,growth) post-rebrand engram->kleos** (NOUVEAU 2026-05-18) | 2 commits separes pour faciliter PR upstream eventuelle | `61b9c3c` + `a72008d` |
| **14 -- LLM thinking-mode toggle (`LLM_THINK` / `KLEOS_SIDECAR_LLM_THINK`)** (NOUVEAU 2026-05-18) | Permet d'utiliser Qwen3 et autres reasoning models sans casser Kleos qui lit le champ `response` Ollama | a commiter |
| **14b -- Mirror `think` -> `reasoning_effort` sur OpenAI-compat** (NOUVEAU 2026-05-19) | Contourne le bug Ollama #14820 : le param `think` est ignore sur `/v1/chat/completions`. Mesure: Broca ask 47s -> 6s sur qwen3:8b-ctx16k. | a commiter |
| **15 -- Dynamic LLM prompt overlay (`KLEOS_LLM_PROMPT_REPOSITORY`)** (NOUVEAU 2026-05-19) | 17 call sites LLM surchargeables a chaud via fichier (`broca`, `chiasm`, `skills`, `extraction`, `memory`, `growth`, `loom`). Mecanisme cascade env + cache mtime TTL 5s. Submodule `prompts-overrides` (VOCSAP/Kleos.prompts) pour les overrides VOCSAP. Catalog dans `docs/dev-notes/llm-prompts-catalog.md`. | a commiter |
| **18 -- kleos-mcp allowlist (`KLEOS_MCP_TOOL_ALLOWLIST`)** (NOUVEAU 2026-05-21) | Filtre additif sur `kleos-mcp/src/tools.rs::registry()` pour restreindre la registry MCP (474 routes -> 15 a 140 selon profil Minimal/Standard/Advanced). Matcher manuel exact + suffix `.*`. Var unset/vide = comportement upstream. Tests: 7 unitaires inline. Cf. section dediee "Patch 18" plus bas. | `5c12c88` |
| **19 -- kleos-mcp stdio newline framing** (NOUVEAU 2026-05-21) | Remplace le framing LSP `Content-Length: NN\r\n\r\n{body}` par newline-delimited JSON per spec MCP stdio. Resout timeout 30s cote tout client MCP conforme (Claude Code, Claude Desktop). Bug upstream Ghost-Frame pur (commits `2dcebf0`/`7dee90d`/`92a94bc`, aucun patch VOCSAP avant). 8 tests unitaires inline (gap upstream). **CANDIDAT PR UPSTREAM**. Cf. section dediee plus bas. | a commiter |
| **Hooks VOCSAP -- fixes + extensions** (cumulatif 2026-05-20/2026-05-21) | Voir section dediee "Hooks VOCSAP" plus bas. Couvre les fixes du 2026-05-20 (alignement bodies `/gate/check`, `/gate/complete-latest`, commenting GROWTH.md) et les ajouts du 2026-05-21 (bloc `ensure_eidolon_running`, hook `eidolon-supervisor-drain-pending.sh`, cascade env vars URL/KEY, fixes flags `kleos-cli list/context`, README `eidolon-supervisor/`). Decalage permanent vs upstream (le bundle `hooks/full` a ete retire upstream, Patch 12 le conserve). | a commiter |
| kleos-mcp refonte standalone | ABANDONNE (decision v1.1.0) | n/a |

**Drops vs v1.1.2** : Patch 4 (`b83f495`) etait re-applique en v1.1.2 mais a ete absorbe
upstream depuis. Lors du rebase v1.1.5 il a ete drop via `git rebase --skip` (le diff
devenait vide vs HEAD post-application upstream).

**Branche topic utilisee** : `local/patches-fixupstream` cree depuis `local/patches`,
rebase sur le nouveau `main` (80982dc), puis `git reset --hard` de `local/patches` vers
`local/patches-fixupstream` apres validation. Push remote via `--force-with-lease`.
Branche topic supprimee post-merge.

**Issues upstream identifiees pendant v1.1.5** :

1. **`kleos-lib/src/services/chiasm/tasks.rs:646-659`** (et logique identique dans
   `services/broca.rs`) : heuristic `is_openai_compat = url.contains("11434")` est
   trop agressif. Si `LLM_URL=http://host:11434/api/generate` (Ollama native), le code
   append `/v1/chat/completions` -> URL bidon `http://host:11434/api/generate/v1/chat/completions`
   -> 404. **Workaround** : configurer `LLM_URL=http://host:11434/v1/chat/completions`
   verbatim (le path `/chat/completions` desactive l'append). A reporter upstream :
   le heuristic devrait verifier le path complet, ou laisser le verbatim si l'URL
   semble explicite (`/v1/chat/completions` OR `/api/generate`).

2. **`kleos-lib/src/activity.rs:261`** : `service: Some("engram".to_string())` hardcode
   dans `fanout_broca`. Residu legacy pre-rebrand. Consequence : tout event poste via
   `POST /activity` (ou `kleos-cli activity`) atterrit dans `broca_actions` avec
   `service="engram"`, mais `kleos-lib/src/services/broca.rs:1209` filtre par
   `known_services = ["kleos","chiasm","axon","loom","soma","thymus","broca"]`.
   `engram` absent -> Broca `ask` retourne toujours vide alors que `feed` montre les
   events. **Workaround** : Patch local potentiel changer la ligne en `"kleos"` ou
   patcher `known_services` pour ajouter `"engram"`. Pas encore appliqué -- candidat
   Patch 13.

**Nouvelles env vars v1.1.5 (TOUTES optionnelles avec defaults)** :

| Var | Defaut | Usage |
|---|---|---|
| `KLEOS_SIDECAR_GATE_MODEL` | = compress_model | Modele LLM pour memory gate (file watcher quality filter, nouveau module sidecar) |
| `KLEOS_SIDECAR_GATE_PACE_MS` | `1500` | Pacing entre gate LLM calls (0 = disable) |
| `KLEOS_SIDECAR_MAX_TURNS_PER_EXTRACT` | `10` | Limite tours par extraction (hardware tuning) |
| `CHIASM_LLM_URL` | -> `LLM_URL` | LLM pour AI plan generation (POST /tasks/{id}/plan) |
| `CHIASM_LLM_API_KEY` | -> `LLM_API_KEY` | API key Chiasm LLM |
| `CHIASM_LLM_MODEL` | -> `LLM_MODEL` | Modele Chiasm |

**Migrations DB ajoutees v1.1.5** : monolith 60 (chiasm_extended_fields), 61 (chiasm_path_claims),
62 (chiasm_agent_keys), 63 (handoff_atoms) + tenant 52/53/54. Auto-applied au boot.

**Deploy v1.1.5 effectue 2026-05-18** :
- LXC 121 : 5 binaires `/usr/local/bin/` (kleos-server, kleos-cli, kleos-credd, kleos-mcp, cred),
  migrations OK, /health 200.
- Windows : 10 binaires `~/.cargo/bin/` (kleos-cli, kr/ke/kw, agent-forge, kleos-sh,
  kleos-sidecar, eidolon-supervisor, engram-approval-tui, cred).
- Backups locaux : `backup/v1.1.2-2026-05-18/{server,windows}/` (508 MB total).
- Tags git backup pousses sur origin : `backup/local-patches-before-rebase-v1.1.3-2026-05-18`,
  `backup/main-before-rebase-v1.1.3-2026-05-18`.

---

## Statut après rebase v1.1.2 (2026-05-15, historique)

`local/patches-1.1.2` a été reconstruite sur la base `upstream/main = v1.1.2 + fix CI (4669ed5)`.
Branche resultante : 14 commits semantiques au-dessus de main (sera renommee en
`local/patches` apres validation + deploy).

| Patch | Statut v1.1.2 | Commit |
|---|---|---|
| 1 -- agent-forge/Cargo.toml cfg(windows) bundled rusqlite | **ABSORBE upstream** (26e930b, syntaxe workspace=true) | drop |
| 2, 10 -- Cargo.toml cfg(windows) sqlcipher (sidecar, approval-tui) | RE-APPLIQUE + bump 1.1.0 -> 1.1.2 | `fe53eef` |
| 4 -- kleos-cred derive-db-key cfg(unix) | RE-APPLIQUE | `b83f495` |
| 7 -- kleos-lib auth normalize_key kleos_ prefix | RE-APPLIQUE | `b4fef72` |
| 5/8B/8C -- embedding Ollama base_url + SPA prefix match | RE-APPLIQUE | `53d1d72` |
| 3, 9 -- kleos-sh Windows port + gate curl subprocess | RE-APPLIQUE | `2cc1977` |
| 11 -- kleos-sidecar namespace OLLAMA env vars | RE-APPLIQUE | `fe023d4` |
| **12 -- hooks bundle preservation** (NOUVEAU v1.1.2) | RE-APPLIQUE | `e86bb03` |
| kleos-mcp refonte standalone | **ABANDONNE** (decision v1.1.0) | n/a |

**ABANDONNE -- kleos-mcp standalone refactor (decision audit 2026-05-13) :** la refonte
locale ~2000 lignes (Database, LocalModelClient, modules auth/tools/transport) etait
jamais testee en prod. Le proxy HTTP upstream simple est conserve. Si un jour
kleos-mcp devient critique, ajouter des patches minimaux cibles, **pas une refonte
parallele**.

**TAKE_UPSTREAM (fixes automatiquement appliques par la base v1.1.0) :**
- `kleos-lib/src/{intelligence/*,episodes,facts,pack,services/broca,graph/*}.rs` :
  fixes SQL `user_id` drop sur tenant-sharded tables (commits upstream a0880ee +
  a798947). **Fixe un bug latent** qui existait en prod LXC 121 v1.0.0.
- `kleos-client/*` : nouvelle crate extraite par upstream (Client + routes registry
  + signed HTTP). Remplace l'inline Client de v1.0.0 dans kleos-cli/main.rs.
- `kleos-server/src/routes/{auth_keys, graph, intelligence, mcp_schema}` : fixes
  upstream divers.
- `kleos-lib/src/db/migrations.rs` : migration 58 `api_key_hash_version_fixup`
  retiree, migration 19 simplifiee (idempotente directement). La prod LXC 121 a
  deja applique migration 58 historique, retirer du source ne casse pas la DB.

Reference complete : `docs/dev-notes/v1.1.0-rebase-audit.md`.

**TAKE_UPSTREAM supplementaire en v1.1.2 :**
- `agent-forge/Cargo.toml` : bloc `[target.'cfg(windows)'.dependencies]` ajoute par
  upstream (26e930b) avec `rusqlite = { workspace = true, features = ["bundled"] }`.
  Notre Patch 1 historique (`version = "0.31"`) est rendu redondant et plus
  divergent que la version upstream (qui s'aligne sur la version workspace). Drop
  pendant le rebase v1.1.2 via `git checkout --ours agent-forge/Cargo.toml`.
- `kleos-cli` : sous-commande `Activity` (PIV) et refacto `Client` extraite en
  crate `kleos-client` deja absorbes en v1.1.0. v1.1.2 ajoute query-string
  forwarding + percent-encoded params + non-JSON response handling cote client,
  rien a porter cote VOCSAP.
- `kleos-lib/src/intelligence/temporal.rs` (+506 lignes) : nouvelle implementation
  temporal pattern detection upstream. Aucun chevauchement avec nos patches.
- `kleos-server/src/routes/broca/*` (+997 lignes), `soma/*` (+134), `thymus/*`
  (+80) : nouvelles routes upstream. Aucun chevauchement avec nos patches.
- Bump cargo workspace version 1.1.0 -> 1.1.2 sur les 17 crates. Nos patches 2
  et 10 (Cargo.toml cfg(windows) sqlcipher) ont du etre mis a jour pour referencer
  `kleos-lib = { ..., version = "1.1.2", ... }` au lieu de "1.1.0".

---

## Patch kleos-cli activity (PIV) -- DEJA AMONT v1.1.0, rien a porter

**Verification 2026-05-14** : le diff `+325/-2` initialement etiquete "patch
local activity" entre `c46fc94` (v1.0.0) et `main` (v1.1.0) ne correspondait
pas a un patch local non porte mais a la **refactorisation upstream** qui a
extrait `struct Client` (anciennement inline dans `kleos-cli/src/main.rs`)
vers la crate `kleos-client`.

- La sous-commande `Commands::Activity` (ajoutee par upstream `725ed3e`,
  10 mai 2026) est presente en v1.1.0 et utilise directement
  `client.post("/activity", body).await` (API publique propre).
- Le bloc `enroll_identity_key` qui appelait jadis `client.http.post(&url)`
  est en v1.1.0 reecrit en `client.post("/identity-keys/enroll", body).await`
  (kleos-cli/src/main.rs:1302).

**Conclusion** : aucun patch a porter cote VOCSAP. Le code v1.1.0 upstream
est deja conforme a l'usage souhaite. L'analyse du diff `c46fc94..main` doit
toujours decouper "diff applique par upstream" vs "patch local divergent",
sinon on poursuit des fantomes.

---

## Patch 1 -- Windows port : SQLite bundled pour agent-forge (ABSORBE en v1.1.2)

**Statut :** ABSORBE upstream a partir de v1.1.2 (commit `26e930b
fix(agent-forge): add bundled rusqlite for Windows`). Le bloc upstream utilise
`rusqlite = { workspace = true, features = ["bundled"] }`, plus aligne sur le
workspace que notre version historique `version = "0.31"`. Conserve ici pour
historique.

**Pour les rebases anterieurs a v1.1.2 uniquement** : ajouter a la fin de
`agent-forge/Cargo.toml` :
```toml
[target.'cfg(windows)'.dependencies]
rusqlite = { version = "0.31", features = ["bundled"] }
```

---

## Patch 2 -- Windows port : sqlcipher feature pour kleos-sidecar

**Fichier :** `kleos-sidecar/Cargo.toml`
**Statut upstream :** Absent. Seul crate kleos-lib-dépendant sans override Windows.
**Symptôme si absent :** `LINK : fatal error LNK1181: cannot open input file 'sqlite3.lib'`

```toml
# Ajouter à la fin du fichier :
[target.'cfg(windows)'.dependencies]
kleos-lib = { path = "../kleos-lib", version = "1.1.0", features = ["sqlcipher"] }
```

**Pourquoi :** kleos-lib nécessite SQLCipher sur Windows. Les autres crates (kleos-cli,
kleos-mcp, kleos-cred) ont déjà cet override -- kleos-sidecar était le seul oublié.

---

## Patch 3 -- Windows port : cfg-gate Unix socket dans kleos-sh

**Fichier :** `kleos-sh/src/main.rs`
**Statut upstream :** Absent.
**Symptôme si absent :** `error[E0433]: could not find 'unix' in 'os'` à la compilation.
**Analyse complète :** `docs/dev-notes/kleos-sh-windows-port.md`

Deux fonctions touchées :

**A) `resolve_key_via_credd()`** -- split en deux implémentations cfg-gated :
- `#[cfg(unix)]`     : `resolve_key_via_credd_socket()` -- UnixStream inchangé
- `#[cfg(not(unix))]`: `resolve_key_via_credd_tcp()`   -- TcpStream via CREDD_BIND

**B) `read_hostname()`** -- ajout fallback COMPUTERNAME :
```rust
#[cfg(not(unix))]
if let Ok(h) = std::env::var("COMPUTERNAME") { ... }
```

**Env var mapping résultant :**

| Plateforme | Transport | Env var |
|---|---|---|
| Linux/macOS | Unix socket | `CREDD_SOCKET` |
| Windows | TCP | `CREDD_BIND` (défaut `127.0.0.1:4400`) |

---

## Patch 4 -- Windows port : OpenOptionsExt gated dans derive-db-key (ABSORBE en v1.1.5)

**Statut :** ABSORBE upstream a partir de v1.1.5 (commit `133e783 fix(ci): cfg-gate
Unix-only APIs for Windows cross-compilation`). Upstream a applique le meme fix avec
une variante de syntaxe : `#[cfg(unix)]` sur l'import au lieu d'un bloc inline.
Semantiquement equivalent. Notre commit `b83f495` a ete drop pendant le rebase v1.1.5
(diff vide vs HEAD post-application upstream). Conserve ici pour historique.

**Fichier :** `kleos-cred/src/bin/derive-db-key.rs`
**Symptôme historique si absent :** `error[E0433]: use of undeclared type 'OpenOptionsExt'`

```rust
// Pour les rebases anterieurs a v1.1.5 uniquement :
let mut opts = std::fs::OpenOptions::new();
opts.write(true).create(true).truncate(true);
#[cfg(unix)]
{
    use std::os::unix::fs::OpenOptionsExt;
    opts.mode(0o600);
}
let mut f = opts.open(path).unwrap_or_else(|e| { ... });
```

**Note sécurité :** Sur Windows, les ACL APPDATA protègent déjà le fichier. `0o600` n'a pas d'équivalent direct mais la protection est équivalente par héritage d'ACL.

---

## Patch 5 -- Embedding backend configurable (KLEOS_EMBEDDING_BACKEND)

**Fichiers :**
- `kleos-lib/src/embeddings/openai.rs`
- `kleos-server/src/main.rs`

**Statut upstream :** Absent. Fonctionnalité locale nécessaire pour Ollama.
**Contexte :** L'upstream hardcode `OnnxProvider` (modèle ONNX local). Le serveur VOCSAP
utilise Ollama (192.168.10.16:11434) via l'API OpenAI-compatible. Sans ce patch, le
serveur essaie de charger un modèle ONNX inexistant et le vector search est désactivé.

**A) `openai.rs` -- `base_url` configurable**

`OpenAiProvider` gagne un champ `base_url` (plus d'URL hardcodée) :

```rust
pub struct OpenAiProvider {
    http: reqwest::Client,
    base_url: String,   // nouveau
    api_key: String,
    model: String,
    dim: usize,
}

pub fn new(
    http: reqwest::Client,
    base_url: Option<String>,  // nouveau -- None = api.openai.com/v1
    api_key: String,
    model: Option<String>,
    dim: usize,
) -> Self { ... }
```

URLs construites dynamiquement : `format!("{}/embeddings", self.base_url)`

**B) `main.rs` -- sélection du backend**

```rust
use kleos_lib::embeddings::openai::OpenAiProvider;  // ajout import

// Dans le spawn de l'embedder :
let embedding_backend = std::env::var("KLEOS_EMBEDDING_BACKEND")
    .unwrap_or_else(|_| std::env::var("ENGRAM_EMBEDDING_BACKEND").unwrap_or_default());

if embedding_backend == "openai" {
    // Lire KLEOS_EMBEDDING_OPENAI_{BASE_URL,API_KEY,MODEL}
    // Instancier OpenAiProvider
} else {
    // Comportement upstream inchangé : OnnxProvider
}
```

**Env vars requises côté serveur :**
```
KLEOS_EMBEDDING_BACKEND=openai
KLEOS_EMBEDDING_OPENAI_BASE_URL=http://192.168.10.16:11434/v1
KLEOS_EMBEDDING_OPENAI_API_KEY=<token>
KLEOS_EMBEDDING_OPENAI_MODEL=bge-m3
KLEOS_EMBEDDING_DIM=1024
```

---

## Patch 6 -- Auth : accepter le préfixe `kleos_` dans normalize_key (REMPLACE PAR PATCH 7)

**Statut :** INCOMPLET. Patch 6 était un premier essai correct dans l'intention mais incorrect
dans l'implémentation. Remplacé intégralement par Patch 7. Conservé ici pour l'historique.

**Erreur de Patch 6 :** `normalize_key` convertissait `kleos_<hex>` en `engram_<hex>` avant
le hash. Or les clés pepper-era (générées depuis que `KLEOS_API_KEY_PEPPER` est défini) ont
leur hash calculé sur la forme `kleos_<hex>`. La conversion produisait un mismatch permanent.

---

## Patch 7 -- Auth : préserver le préfixe original dans normalize_key

**Fichier :** `kleos-lib/src/auth.rs`
**Statut upstream :** Absent.
**Symptôme si absent :** GUI retourne "Invalid API Key" pour toute clé à préfixe `kleos_`.

**Cause racine :** Les clés générées depuis que `KLEOS_API_KEY_PEPPER` est configuré ont
leur hash stocké en DB sur la forme canonique `kleos_<hex>` (c'est le préfixe produit
lors de la génération). `normalize_key` ne doit donc PAS convertir `kleos_` en `engram_`.

**Deux corrections dans `auth.rs` :**

**A) `normalize_key` -- préserver le préfixe**

```rust
// AVANT (Patch 6 -- incorrect)
fn normalize_key(raw_key: &str) -> Option<String> {
    let hex_portion = if let Some(rest) = raw_key.strip_prefix("engram_") {
        rest
    } else if let Some(rest) = raw_key.strip_prefix("kleos_") {
        rest           // extrait le hex mais...
    } else {
        raw_key.strip_prefix("eg_")?
    };
    Some(format!("engram_{}", hex_portion...))  // ...force toujours engram_ -> mismatch
}

// APRES (Patch 7 -- correct)
fn normalize_key(raw_key: &str) -> Option<String> {
    let (canonical_prefix, hex_portion) =
        if let Some(rest) = raw_key.strip_prefix("engram_") {
            ("engram_", rest)   // engram_ reste engram_
        } else if let Some(rest) = raw_key.strip_prefix("kleos_") {
            ("kleos_", rest)    // kleos_ reste kleos_ (pepper-era)
        } else if let Some(rest) = raw_key.strip_prefix("eg_") {
            ("engram_", rest)   // eg_ -> engram_ (alias court legacy)
        } else {
            return None;
        };
    // ...validation hex...
    Some(format!("{}{}", canonical_prefix, hex_portion.to_ascii_lowercase()))
}
```

**B) `validate_key` -- extraction hex robuste**

```rust
// AVANT -- hypothèse engram_ (7 chars), tronque kleos_ (6 chars) silencieusement
let hex_portion = normalized_key[7..].to_string();

// APRES -- split_once indépendant de la longueur du préfixe
let hex_portion = normalized_key
    .split_once('_')
    .map(|(_, hex)| hex.to_string())
    .ok_or_else(|| crate::EngError::Auth("invalid key format".into()))?;
```

**Règle de hashing par génération :**

| Époque | Préfixe clé | Forme canonique hash | Hash version |
|---|---|---|---|
| Avant PEPPER | `engram_<hex>` | `engram_<hex>` | v1 (SHA-256 nu) |
| Depuis PEPPER | `kleos_<hex>` | `kleos_<hex>` | v2 (SHA-256 + pepper) |

**Note :** Le champ `key_prefix` en DB contient toujours les 8 premiers chars du hex (sans
le préfixe textuel). Le lookup par `key_prefix` fonctionne donc pour les deux formes.

---

## Patch 8 -- GUI : activation et fix CSP SvelteKit

**Concerne :** Déploiement sur LXC 121 (kleos-server Linux musl)
**Statut upstream :** N/A -- problème de configuration déploiement + patch de fichier statique généré.

Ce patch couvre deux problèmes liés à l'interface web, indépendants mais toujours
présents ensemble lors d'un redéploiement.

---

### A) 401 sur sous-routes SvelteKit -- SPA_ROUTES match exact

**Symptôme :** La page affiche `401: {"error":"Authentication required..."}` quand
le navigateur est sur une sous-route comme `/gui/memories`, `/search/details`, etc.
(rafraîchissement ou bookmark sur un sous-chemin du SvelteKit).

**Cause :** `gui_spa_middleware` utilisait `SPA_ROUTES.contains(&path)` (match exact).
Les sous-routes comme `/gui/settings` ne sont pas dans la liste, donc le middleware
tombait en `next.run()` → `api_routes` → `auth_middleware` → 401.

**Fichier :** `kleos-server/src/routes/gui/mod.rs` -- `gui_spa_middleware`.

**Fix (appliqué dans le code) :** Remplacer le `contains` par un matching par préfixe :
- `"/"` reste exact (pour éviter de capturer toutes les routes)
- Les autres entrées (`/gui`, `/search`, etc.) matchent aussi leurs sous-chemins via
  `path.starts_with(&format!("{}/", spa))`

**Statut :** Fix commité et déployé. Validé le 2026-05-12.

---

### B) 401 sur GET / -- variables d'environnement GUI manquantes

**Symptôme :** La page web répond `401: {"error":"Authentication required. Provide
X-Kleos-Sig header or Bearer token."}` au lieu d'afficher la page de login.

**Cause :** `gui_spa_middleware` n'intercepte les requêtes HTML que si
`state.config.gui_enabled = true`. Ce flag est positionné uniquement si
`ENGRAM_GUI_PASSWORD` (traduit depuis `KLEOS_GUI_PASSWORD` par `migrate_env_prefix`)
est non-vide. Sans lui, le middleware passe la main à `api_routes`, qui applique
`auth_middleware` → 401.

Fichiers concernés : `kleos-lib/src/config.rs:826` (lit `ENGRAM_GUI_PASSWORD`)
et `kleos-server/src/routes/gui/mod.rs:722` (condition `!state.config.gui_enabled`).

**Fix :** Vérifier que `/etc/kleos/kleos.env` (sur LXC 121) contient :
```env
KLEOS_GUI_PASSWORD=1
KLEOS_GUI_BUILD_DIR=/usr/local/share/kleos/gui-build
```
Ces deux lignes sont déjà dans `deploy/kleos.env`. Si l'env LXC est plus ancien,
les ajouter manuellement puis `systemctl restart kleos`.

---

### C) Page blanche CSP -- éléments inline SvelteKit bloqués

**Symptôme :** Après login réussi, la page GUI est blanche. La console du navigateur
affiche :
```
Refused to apply inline style because it violates CSP directive "style-src 'self'"
Refused to execute inline script because it violates CSP directive "script-src 'self'"
```

**Cause :** SvelteKit (build prod, `adapter-static`) génère dans `index.html` :
1. `<body style="display: contents">` -- style inline sur `<body>`
2. Un bloc `<script>` inline d'initialisation (définit `__sveltekit_*`)

Le serveur impose `style-src 'self'; script-src 'self' 'wasm-unsafe-eval'`
dans son CSP (`kleos-server/src/server.rs`), ce qui bloque les deux.

**Fix : patcher `index.html` sur le LXC et créer deux fichiers compagnons.**

Le build GUI est à `/usr/local/share/kleos/gui-build/`. Ce répertoire est `.gitignored`
(artefact de build), le patch se re-applique à chaque redéploiement de la GUI.

```bash
GUI=/usr/local/share/kleos/gui-build

# 1. Externaliser le style inline : remplacer l'attribut style par une classe
sed -i 's/style="display: contents"/class="sk-root"/' "$GUI/index.html"

# 2. Créer le CSS correspondant
echo '.sk-root { display: contents }' > "$GUI/_app/root.css"

# 3. Injecter le <link> dans index.html (avant </head>)
sed -i 's|</head>|    <link href="/_app/root.css" rel="stylesheet">\n</head>|' "$GUI/index.html"

# 4. Extraire le script inline
#    a. Repérer le bloc <script>...</script> inline dans index.html
grep -n "<script>" "$GUI/index.html"
#    b. Copier le contenu du bloc dans _app/init.js (sans les balises <script>)
#    c. Remplacer le bloc entier par : <script src="/_app/init.js"></script>
```

**Vérification après patch :**
```bash
curl -sI http://localhost:4200/_app/root.css | grep "200"
curl -sI http://localhost:4200/_app/init.js  | grep "200"
```

**Note :** Le bloc `<script>` varie à chaque build SvelteKit -- l'extraction manuelle
est nécessaire. La correction permanente serait de configurer SvelteKit pour externaliser
son init script (non supporté nativement par `adapter-static` à ce jour).

---

## Patch 9 -- Windows : gate.rs subprocess curl + exec.rs Windows shell

**Fichiers :**
- `kleos-sh/src/gate.rs` -- `send_request()` (cfg-gate Windows via curl subprocess)
- `kleos-sh/src/main.rs` -- `build_client()` (connect_timeout configurable)
- `kleos-sh/src/exec.rs` -- `run_command()` (shell Windows)

**Statut upstream :** Absent.
**Date initiale :** 2026-05-12
**Date resolution :** 2026-05-13 (Attempt 7)

### A) gate.rs : gate check Windows -- RESOLU (Attempt 7)

**Fichier :** `kleos-sh/src/gate.rs`

#### Symptome (avant fix)

Sur Windows, `kleos-sh.exe --gate-only -c "echo test"` echouait avec :
- `reqwest async` : "operation timed out" (IOCP ne recoit pas les paquets)
- `reqwest::blocking` : meme echec (utilise tokio IOCP en interne)
- `std::net::TcpStream` brut : connect+write OK, read bloque jusqu'au timeout
- `curl subprocess` (sans contrainte de port) : exit 28, source port=1434 (anormal), 0 octets recus

Pendant ce temps, `curl.exe` lance directement depuis PowerShell sur la meme machine
contre le meme endpoint retournait 201 en <1s.

#### Cause racine identifiee (2026-05-13)

**Deux problemes superposes** :

1. **OPNSense (firewall homelab Proxmox)** droppait les paquets de retour destines
   a un source port "registered" (port 1434, plage 1024-49151). Le client TCP
   ouvrait depuis 1434 parce que la plage dynamique Windows defaut commencait a
   1024 (consequence d'un `netsh set dynamicport start=1024 num=64511` applique
   precedemment pour tenter de contourner le probleme).

2. **curl Windows et `--local-port range`** : curl prend uniquement le PREMIER
   port de la plage et ne scanne PAS les ports suivants. Si ce premier port est
   en TIME_WAIT (cas typique apres un appel precedent), curl echoue avec exit 7
   (EADDRINUSE) au lieu de retenter avec le port suivant. Toute plage
   `--local-port` etait donc inutilisable apres le premier appel.

#### Fix applique (Attempt 7)

1. **OPNSense** nettoye le 2026-05-12 : regle `subnet_admin` (seq=25, quick=1)
   ajoutee pour autoriser les retours quels que soient les ports.
2. **netsh dynamicport** restaure aux defauts Microsoft (start=49152, num=16384)
   pour tcp/udp en ipv4/ipv6.
3. **gate.rs** : retrait integral de `--local-port` du subprocess curl. L'OS
   choisit librement un port ephemere a chaque appel, evitant la collision
   TIME_WAIT.
4. **Nouvelle env var `KLEOS_SH_APPROVAL_TIMEOUT_SECS`** (defaut 12s) qui
   pilote `curl --max-time`. Override jusqu'a 121s pour permettre au mode exec
   d'attendre une approbation humaine via `engram-approval-tui`. Le mode
   `--claude-hook` n'a pas besoin d'override -- il fail-open silencieusement
   sur les tools dans `TOOLS_REQUIRING_APPROVAL` au timeout court.

#### Etat du code (gate.rs)

`send_request` reste split en deux implementations cfg-gatees :
- `#[cfg(unix)]` : reqwest async inchange (Linux/macOS ne souffraient pas du bug).
- `#[cfg(not(unix))]` : `tokio::process::Command` + curl subprocess, sans
  `--local-port`, `--max-time` pilote par `KLEOS_SH_APPROVAL_TIMEOUT_SECS`.

Le commentaire dans gate.rs documente l'historique complet (Attempts 1-7) pour
eviter de re-tenter une approche deja invalidee.

#### Verification de non-regression

```powershell
# 1. Connectivite base (tool Read = pas d'approval)
$env:KLEOS_API_KEY="kleos_..."; $env:KLEOS_URL="http://192.168.10.21:4200"
'{}' | kleos-sh.exe --gate-only --tool-name Read -c "ls"
# Attendu: {"hookSpecificOutput":{"permissionDecision":"allow",...}} en <1s

# 2. Tool require-approval sans TUI active (timeout court attendu)
'{}' | kleos-sh.exe --gate-only --tool-name Bash -c "echo test"
# Attendu: deny "gate timed out..." apres ~12s (fail-closed en mode exec)

# 3. Avec TUI active et approbation manuelle
$env:KLEOS_SH_APPROVAL_TIMEOUT_SECS="121"
# Lancer engram-approval-tui dans un autre terminal
'{}' | kleos-sh.exe --gate-only --tool-name Bash -c "echo test"
# Approuver dans la TUI, le check renvoie allow dans la limite des 121s
```

**main.rs `build_client()` :** connect_timeout reste configurable
(defaut 5s, env var `KLEOS_SH_CONNECT_TIMEOUT_SECS`), conserve pour les
chemins reqwest (Unix uniquement post-Attempt 7, mais le reglage reste valide
si une future implementation Windows revient a reqwest).

**Env vars actives :**
- `KLEOS_SH_APPROVAL_TIMEOUT_SECS` : Windows uniquement -- curl `--max-time`
  pour `/gate/check`. Defaut 12s. Override 121s recommande pour attendre une
  approbation humaine en mode exec.
- `KLEOS_SH_CONNECT_TIMEOUT_SECS` : Unix uniquement -- reqwest connect_timeout.
  Defaut 5s.
- `KLEOS_SH_TIMEOUT_SECS` : Unix uniquement -- reqwest timeout total.

---

### B) Windows shell dans exec.rs

**Symptome si absent :** `exec failed: failed to spawn shell: Le chemin d'acces specifie
est introuvable. (os error 3)` en mode non-hook sur Windows.

**Avant :**
```rust
Command::new("/bin/sh").arg("-c").arg(command)
```

**Apres :**
```rust
#[cfg(unix)]
let (shell, flag): (&str, &str) = ("/bin/sh", "-c");
#[cfg(not(unix))]
let (shell, flag): (&str, &str) = ("cmd", "/C");

Command::new(shell).arg(flag).arg(command)
```

**Note :** Ce bug n'affecte pas le mode `--claude-hook` (qui ne passe pas par exec.rs).
Affecte uniquement le mode exec direct (`kleos-sh.exe -c "cmd"`).

---

## Patch 10 -- Windows : sqlcipher feature pour kleos-approval-tui

**Fichier :** `kleos-approval-tui/Cargo.toml`
**Statut upstream :** Absent.
**Date :** 2026-05-13
**Symptome si absent :** `LINK : fatal error LNK1181: cannot open input file 'sqlite3.lib'`
au linkage final de `engram-approval-tui.exe`.

```toml
# Ajouter a la fin du fichier :
[target.'cfg(windows)'.dependencies]
kleos-lib = { path = "../kleos-lib", version = "1.1.0", features = ["sqlcipher"] }
```

**Pourquoi :** Identique aux Patches 1 et 2. `kleos-approval-tui` depend de
`kleos-lib` (alors meme qu'il pourrait s'en passer en tant que pur client HTTP --
voir TODO de refactoring ci-dessous). `kleos-lib` tire `libsqlite3-sys` sans la
feature `bundled` par defaut, donc le linker MSVC cherche un `sqlite3.lib`
systeme inexistant sur Windows. La feature `sqlcipher` de `kleos-lib` active
SQLCipher embarque qui satisfait le linker.

**TODO de refactoring (non bloquant) :** `kleos-approval-tui` est conceptuellement
un client HTTP pur (parle a `kleos-server` via reqwest pour `/approvals/pending`,
`/approvals/{id}/decide`). Il ne devrait pas tirer toute la couche DB de
`kleos-lib`. Une refactorisation propre consisterait a extraire les types
partages (proto/DTO) dans un sous-crate `kleos-proto` sans deps DB. Charge
estimee : moyenne (extraction de structs serde). Hors scope du patch local
courant.

---

## Patch 11 -- kleos-sidecar : namespacer OLLAMA_URL/MODEL en KLEOS_SIDECAR_OLLAMA_*

**Fichier :** `kleos-sidecar/src/main.rs`
**Statut upstream :** Absent. Specifique au poste Windows VOCSAP.
**Date :** 2026-05-14

### Intention

Permettre au binaire Windows `kleos-sidecar` (compression de contexte Claude
Code via Ollama) de cibler un endpoint et un modele independants des variables
generiques `OLLAMA_URL` / `OLLAMA_MODEL` utilisees par d'autres outils
cote operateur (Claude CLI direct, scripts ponctuels, etc.).

### Pourquoi pas upstream

`OllamaConfig::from_env()` dans `kleos-lib` lit les variables generiques
`OLLAMA_URL` / `OLLAMA_MODEL`. Toucher cette methode partagee impacterait
tous les consommateurs (`kleos-server`, `kleos-ingest`, evolver skills) qui
n'ont pas le meme besoin. Le sidecar Windows est le seul cas ou la collision
de namespace est genante. Patch garde local.

### Comment

Override applique **uniquement cote sidecar**, sans modifier `kleos-lib`. Apres
l'appel `OllamaConfig::from_env()`, on lit les variables namespacees et on
surcharge les champs `url` et `model` si elles sont presentes. `OLLAMA_URL` /
`OLLAMA_MODEL` restent en fallback pour migration douce (aucun breaking change).

### Ce qui a ete fait

`kleos-sidecar/src/main.rs:317-335` (bloc d'initialisation du LLM) :

```rust
// AVANT
let llm_config = OllamaConfig::from_env();

// APRES
let llm_config = {
    let mut cfg = OllamaConfig::from_env();
    if let Ok(v) = std::env::var("KLEOS_SIDECAR_OLLAMA_URL") {
        cfg.url = v;
    }
    if let Ok(v) = std::env::var("KLEOS_SIDECAR_OLLAMA_MODEL") {
        cfg.model = v;
    }
    cfg
};
```

Comportement attendu :

| Env vars positionnees | URL / model effectifs |
|---|---|
| Aucune | defauts de `OllamaConfig::default()` (`127.0.0.1:11434`, `llama3.2:3b`) |
| `OLLAMA_URL`/`OLLAMA_MODEL` seuls | valeurs `OLLAMA_*` (compat legacy) |
| `KLEOS_SIDECAR_OLLAMA_URL`/`MODEL` seuls | valeurs sidecar-scoped |
| Les deux ensembles | `KLEOS_SIDECAR_OLLAMA_*` prime sur `OLLAMA_*` |

Autres parametres `OLLAMA_TIMEOUT_*`, `OLLAMA_CONCURRENCY`, `LLM_API_KEY` :
non namespaces dans ce patch. Si un besoin sidecar-specifique apparait, etendre
le bloc override avec la meme logique.

### Comment reproduire le fix

```bash
# Sur une base fraiche (rebase sans ce patch)
grep -n "OllamaConfig::from_env()" kleos-sidecar/src/main.rs
# Doit retourner une seule ligne dans le bloc compress_enabled.

# Remplacer cette ligne par le bloc override (voir section "Ce qui a ete fait").
# Verifier :
cargo check -p kleos-sidecar
# Doit passer 0 erreur.
```

Variables a positionner cote poste Windows (operateur uniquement) :

```env
KLEOS_SIDECAR_OLLAMA_URL=http://192.168.10.16:11434/v1/chat/completions
KLEOS_SIDECAR_OLLAMA_MODEL=llama3.2:3b
```

Une fois le binaire `kleos-sidecar.exe` recompile et redeploye, les
`OLLAMA_URL` / `OLLAMA_MODEL` generiques peuvent etre repris pour d'autres
outils sans impact sur le sidecar.

### Note design : pourquoi pas une refacto kleos-lib ?

Une refacto propre (ajouter `OllamaConfig::from_env_with_prefix(prefix)` dans
`kleos-lib`) serait portable upstream et benefierait a tous les consommateurs.
Charge : etendre la methode + adapter trois call sites. Si l'operateur juge
le besoin generalisable a `kleos-ingest` ou `kleos-server`, ouvrir une PR
upstream et supprimer ce patch local au merge suivant. En attendant, le patch
chirurgical cote sidecar minimise la divergence.

---

## Patch 12 -- hooks bundle : preserver hooks/full + hooks/simple en attendant le rewrite upstream

**Fichiers :** `hooks/full/*.sh` (8 scripts) et `hooks/simple/*.sh` (4 scripts)
**Statut upstream :** Supprime par `bebb537` (chore: repo hygiene). Le nouveau
`hooks/README.md` upstream documente : "The Kleos hooks bundle is under maintenance
and not currently shipped. [...] A new hooks bundle will be reintroduced once the
surface stabilises."
**Date :** 2026-05-15
**Commit :** `e86bb03 patch(hooks): preserve VOCSAP bundle pending upstream rewrite`

### Intention

Conserver les 12 scripts hook (session-start, user-prompt, mnemonic-observe,
session-end, enforce-agent-forge, enforce-kleos-search, lib-eidolon,
track-agent-forge) tels qu'ils existaient avant `bebb537`, pour pouvoir
continuer a les iterer cote VOCSAP en attendant que la nouvelle version
upstream stabilisee soit publiee.

### Pourquoi pas upstream

Decision upstream explicite (cf README) : "removed pending a rewrite that
decouples them from operator-specific tooling". Le rewrite n'est pas encore
disponible. Plutot que d'attendre sans hooks fonctionnels, on garde la version
v1.1.0 era localement.

### Comment

Le `hooks/README.md` upstream (nouveau message "under maintenance") est
conserve tel quel -- il documente correctement la divergence cote upstream.
Seuls les 12 scripts .sh sont restaures depuis le tag de backup pre-rebase.

### Ce qui a ete fait

Le commit `e86bb03` recree :

```
hooks/full/enforce-agent-forge.sh        (3.8K)
hooks/full/enforce-kleos-search.sh       (3.8K)
hooks/full/lib-eidolon.sh                (1.6K)
hooks/full/mnemonic-observe.sh           (1.8K)
hooks/full/session-end.sh                (7.6K)
hooks/full/session-start-kleos.sh        (9.2K)
hooks/full/track-agent-forge.sh          (4.0K)
hooks/full/user-prompt-lean.sh           (5.4K)
hooks/simple/mnemonic-observe.sh         (1.6K)
hooks/simple/session-end.sh              (697B)
hooks/simple/session-start.sh            (~)
hooks/simple/user-prompt.sh              (~)
```

Les blobs sont identiques byte-a-byte a ceux de v1.1.0 era (verifies via
`git ls-tree` croise avec `backup/local-patches-before-rebase-v1.1.2-2026-05-15`).

### Comment reproduire le fix

Sur une base fraiche apres rebase qui aurait absorbe `bebb537` :

```bash
# Le tag de backup doit exister pour cette commande -- voir procedure
# de rebase ci-dessous (etape 2 cree le tag).
git checkout backup/local-patches-before-rebase-vX.Y.Z-YYYY-MM-DD -- \
  hooks/full hooks/simple
git add hooks/full hooks/simple
git commit -m "patch(hooks): preserve VOCSAP bundle pending upstream rewrite"
```

### Cycle de vie attendu

Ce patch est **temporaire**. Lorsque upstream publiera le nouveau hook bundle
(annonce dans hooks/README.md upstream), il faudra :

1. Comparer la nouvelle API hook upstream avec nos scripts actuels.
2. Migrer nos personnalisations vers le nouveau format.
3. Dropper ce Patch 12 lors du rebase qui integre le nouveau bundle.

D'ici la, ne pas modifier `hooks/README.md` (qui reste la version upstream
"under maintenance") pour eviter un conflit inutile au prochain rebase.

---

## Patch 13 -- fix engram residue (broca, activity, growth)

**Statut :** commite 2026-05-18, 2 commits (`61b9c3c` + `a72008d`), pas encore push.
**Contexte :** decouvert pendant les tests post-deploy v1.1.5 lorsque Broca `ask`
retournait systematiquement vide alors que `/broca/feed` montrait bien les events.
Audit complet : `docs/dev-notes/engram-residue-audit.md` (145 occurrences "engram"
classees dans 7 categories).

5 changements minimaux dans la couche kleos-lib pour passer le tag canonique a
`"kleos"` cote write, tout en preservant `"engram"` en lecture pour les rows
deja stockees :

- `kleos-lib/src/activity.rs:261` : fanout broca, `service: "engram"` -> `"kleos"`
- `kleos-lib/src/services/broca.rs:1209` : `known_services` ajoute `"engram"` comme alias
- `kleos-lib/src/intelligence/growth.rs:424` : `GrowthReflectRequest.service` `"engram"` -> `"kleos"`
- `kleos-lib/src/intelligence/growth.rs:91` : match arm `"engram" | "kleos"`
- `kleos-lib/src/intelligence/growth.rs:407` : SELECT `source LIKE '%-growth'` (au lieu de strict `'engram-growth'`)

Le commit 1 (`61b9c3c` broca+activity) est upstream-friendly (fix actif d'un bug
visible). Le commit 2 (`a72008d` growth + docs) est local-only par prudence
(touche un tag stocke en DB, upstream pourrait resister meme si techniquement
safe via le LIKE retro-compat).

---

## Patch 14 -- LLM thinking-mode toggle

**Fichiers :**
- `kleos-lib/src/llm/mod.rs` (fonction helper `think_enabled()`)
- `kleos-lib/src/llm/types.rs` (champ `OllamaConfig.think: Option<bool>`)
- `kleos-lib/src/llm/local.rs` (body injection, priorite override)
- `kleos-lib/src/intelligence/llm.rs` (struct `OllamaRequest.think`)
- `kleos-lib/src/services/broca.rs` (`call_llm_endpoint` merge `think` dans body)
- `kleos-sidecar/src/main.rs` (lit `KLEOS_SIDECAR_LLM_THINK` -> `cfg.think`)

**Statut upstream :** Absent. Modeles thinking (Qwen3, deepseek-r1, gpt-oss:20b)
renvoient un champ `thinking` separe et un `response` souvent vide quand on hit
`/api/generate` (Ollama natif) -- comportement documente dans
`docs/dev-notes/dreamer-llm-contract.md`. Sans flag explicite cote client, le
dreamer/Broca/Chiasm/sidecar deviennent muets quand l'operateur swap pour un
modele thinking.
**Date :** 2026-05-18 (post deploy v1.1.5, decouverte via consultation peer
claude-peers `desktop-7b2civn-crawl4ai-rag` sur le choix de modele).

### Intention

Permettre a l'operateur de basculer le mode thinking on/off **sans recompiler**
quand il swap pour un modele reasoning-capable (typiquement Qwen3:8b). Default
safe = thinking off (= behavior actuel pour les non-thinking models comme
`llama3.2:3b`).

### Pourquoi pas upstream

Le code upstream construit ses bodies Ollama sans le champ `think`, ce qui fait
qu'Ollama applique le default du modele (true pour les modeles reasoning,
ignore pour les autres). Tant qu'upstream cible les modeles non-thinking, c'est
invisible. Patch candidat PR upstream legitime mais pas critique pour leur
distribution -- garde local pour l'instant.

### Comment

Hierarchie de priorite (la plus specifique gagne) :

| Niveau | Source | Periode de validite |
|---|---|---|
| 1 | `OllamaConfig.think = Some(true \| false)` | Override programmatique (test, init explicite) |
| 2 | `KLEOS_SIDECAR_LLM_THINK` env var | Sidecar process only (lu dans `kleos-sidecar/src/main.rs`) |
| 3 | `LLM_THINK` env var | Tous les autres consommateurs kleos-lib (dreamer, Broca, Chiasm) |
| 4 | `false` | Default safe (aucune var set) |

Toutes les vars sont booleennes parsees via `matches!(s.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")`. Anything else, including unset, = `false`.

Pattern aligne sur Patch 11 (`KLEOS_SIDECAR_OLLAMA_*` overrident les
`OLLAMA_*` generiques) -- le sidecar a son propre namespace pour decoupler son
choix de modele/endpoint/thinking du serveur Kleos qui peut etre tres different
(Windows host vs LXC Linux, GPU local vs remote).

### Ce qui a ete fait

```rust
// kleos-lib/src/llm/mod.rs (helper expose au workspace)
pub fn think_enabled() -> bool {
    std::env::var("LLM_THINK")
        .ok()
        .map(|s| matches!(s.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

// kleos-lib/src/llm/types.rs (OllamaConfig accepte un override)
pub struct OllamaConfig {
    // ... champs existants ...
    /// Per-config thinking-mode override. None = fallback sur LLM_THINK env var.
    pub think: Option<bool>,
}

// kleos-lib/src/llm/local.rs (resolution de la priorite)
let think = self.config.think.unwrap_or_else(super::think_enabled);
let body = serde_json::json!({
    // ...
    "think": think,
});

// kleos-lib/src/intelligence/llm.rs (struct OllamaRequest field explicite)
struct OllamaRequest {
    // ...
    think: bool,
    // ...
}
let body = OllamaRequest {
    // ...
    think: crate::llm::think_enabled(),
    // ...
};

// kleos-lib/src/services/broca.rs (merge dans body generique)
let body_value = {
    let mut v = serde_json::to_value(&body)
        .map_err(|e| format!("LLM body serialization failed: {e}"))?;
    if let serde_json::Value::Object(ref mut map) = v {
        map.entry("think".to_string())
            .or_insert_with(|| serde_json::Value::Bool(crate::llm::think_enabled()));
    }
    v
};
let mut req = BROCA_LLM_CLIENT.post(url).json(&body_value);

// kleos-sidecar/src/main.rs (override env sidecar-scope)
if let Ok(v) = std::env::var("KLEOS_SIDECAR_LLM_THINK") {
    cfg.think = Some(matches!(
        v.to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    ));
}
```

Comportement attendu :

| Env positionnees | Sidecar | Serveur (Broca/Chiasm/Dreamer) |
|---|---|---|
| Aucune | `think: false` | `think: false` |
| `LLM_THINK=true` | `think: true` (fallback) | `think: true` |
| `KLEOS_SIDECAR_LLM_THINK=false`, `LLM_THINK=true` | `think: false` (override sidecar) | `think: true` |
| `KLEOS_SIDECAR_LLM_THINK=true`, `LLM_THINK=false` | `think: true` (override sidecar) | `think: false` |

### Comment reproduire le fix

```bash
# Sur une base fraiche (rebase sans ce patch)

# 1. helper public
grep -n "pub fn think_enabled" kleos-lib/src/llm/mod.rs
# Doit retourner une seule ligne. Sinon, ajouter la fonction (cf. snippet ci-dessus).

# 2. struct OllamaConfig
grep -n "pub think: Option<bool>" kleos-lib/src/llm/types.rs
# Doit etre present + ajoute dans Default::default() : `think: None`.

# 3. body injection LocalModelClient
grep -n '"think": think' kleos-lib/src/llm/local.rs
# Doit etre present, precede de `let think = self.config.think.unwrap_or_else(super::think_enabled);`.

# 4. struct OllamaRequest dreamer
grep -n "think: bool" kleos-lib/src/intelligence/llm.rs
# Doit etre present + le caller call_llm doit populer `think: crate::llm::think_enabled()`.

# 5. call_llm_endpoint merge broca
grep -n 'entry("think".to_string())' kleos-lib/src/services/broca.rs
# Doit etre present (or_insert pour ne pas ecraser un caller qui set deja think).

# 6. override sidecar
grep -n "KLEOS_SIDECAR_LLM_THINK" kleos-sidecar/src/main.rs
# Doit etre present, juste apres les overrides KLEOS_SIDECAR_OLLAMA_URL/MODEL.

# Validation
cargo check -p kleos-lib -p kleos-sidecar
# Doit passer 0 erreur.
```

### Configuration recommandee post-deploy

**LXC 121 (`/etc/kleos/kleos.env`)** -- ne PAS positionner `LLM_THINK` si on
tourne sur un modele non-thinking (default `llama3.2:3b`). Si swap vers
`qwen3:8b` ou autre reasoning model, laisser quand meme `LLM_THINK` UNSET ou
`=false` puisque Kleos lit le champ `response` Ollama et le thinking serait
perdu -- on veut bypass.

**Poste Windows** -- meme regle : ne PAS positionner `KLEOS_SIDECAR_LLM_THINK`
si on swap le sidecar vers `qwen3:8b` ou autre reasoning. Default false bypass
le thinking et garde `response` non-vide.

Mettre `LLM_THINK=true` uniquement si :
- Un nouveau caller dans Kleos sait lire le champ `thinking` separe (pas le cas
  actuellement nulle part)
- L'operateur veut explicitement les chains-of-thought dans les logs serveur
  pour debug (rare et tres bruyant)

### Note design : pourquoi pas inverser le default ?

Default false a ete choisi pour ne pas casser le behavior actuel quand un
operateur upgrade Kleos sans toucher a son env file. Si un jour un modele
reasoning devient le default upstream (peu probable a court terme), inverser
le default necessitera une migration d'env vars cote operateurs existants. Cf.
discussion peer claude-peers 2026-05-18.

---

## Patch 14b -- Mirror `think` -> `reasoning_effort` (Ollama OpenAI-compat workaround)

**Date** : 2026-05-19
**Statut** : DEPLOYE sur LXC 121, validation end-to-end OK
**Fichier touche** : `kleos-lib/src/services/broca.rs` (function `call_llm_endpoint`,
~13 lignes ajoutees autour des lignes 732-756)

### Probleme

Le Patch 14 injecte `think: false` (param natif Ollama) dans le body de chaque
appel LLM via `call_llm_endpoint`. Test empirique 2026-05-19 sur qwen3:8b-ctx16k :

| Endpoint | Param | Latence Broca ask | Reasoning genere |
|---|---|---|---|
| `/v1/chat/completions` | `think:false` | 47s | ~750 chars (gaspilles) |
| `/api/chat` natif | `think:false` | 5s | 0 chars |

Le param `think` est **silencieusement ignore** par Ollama sur l'endpoint
OpenAI-compat (`/v1/chat/completions`), confirme par l'issue upstream
[ollama/ollama#14820](https://github.com/ollama/ollama/issues/14820)
(closed mars 2026).

### Solution officielle (Ollama upstream)

L'endpoint OpenAI-compat honore le param OpenAI-standard `reasoning_effort`
(string) :

- `"high"`, `"medium"`, `"low"` -> thinking ON (avec effort variable)
- `"none"` -> thinking OFF

Mapping interne Ollama (cf. `openai/openai.go` upstream) : `reasoning_effort`
est traduit vers le champ `Think` interne.

### Patch applique

```rust
// kleos-lib/src/services/broca.rs:732-756 (call_llm_endpoint)
let body_value = {
    let mut v = serde_json::to_value(&body)
        .map_err(|e| format!("LLM body serialization failed: {e}"))?;
    if let serde_json::Value::Object(ref mut map) = v {
        let think = crate::llm::think_enabled();
        map.entry("think".to_string())
            .or_insert_with(|| serde_json::Value::Bool(think));
        let effort = if think { "high" } else { "none" };
        map.entry("reasoning_effort".to_string())
            .or_insert_with(|| serde_json::Value::String(effort.to_string()));
    }
    v
};
```

Les deux params sont injectes systematiquement. Chaque endpoint Ollama
ignore silencieusement celui qu'il ne reconnait pas :

- `/v1/chat/completions` -> lit `reasoning_effort`, ignore `think`
- `/api/chat` natif -> lit `think`, ignore `reasoning_effort`

Pas de regression possible.

### Mesure post-deploiement (LXC 121, qwen3:8b-ctx16k via OpenAI-compat)

```
Broca ask end-to-end : 47s (avant 14b) -> 6s (apres 14b)
Reasoning tokens     : ~750 -> 0
```

### Pourquoi pas refactorer vers /api/chat natif ?

Initialement le plan etait un Patch 15 refactorisant 6 call sites
(`llm_narrate`, `ask_plan_call`, `ask_summarize_call`, `chiasm::generate_plan`,
`loom::execute_llm_step`) pour supporter le format natif Ollama.
Cout estime : ~300 lignes, 1h30 d'effort. La decouverte de
`reasoning_effort` reduit le patch a 3 lignes pour le meme gain de latence.

### Reference

- Issue Ollama : https://github.com/ollama/ollama/issues/14820
- Test empirique : Kleos #2136 (decouverte), #2148 (validation)

---

## Patch 14c -- Extend `reasoning_effort` injection to LocalModelClient / Loom / Atoms

**Date** : 2026-05-19
**Statut** : code applique, deploye sur LXC 121, validation end-to-end OK via `/skills/capture`
**Fichiers touches** :
- `kleos-lib/src/llm/mod.rs` (helper `inject_openai_compat_reasoning` ajoute, +18 lignes)
- `kleos-lib/src/llm/local.rs` (LocalModelClient.call, body devient `mut`, appel helper)
- `kleos-lib/src/services/loom.rs` (execute_llm_step, idem)
- `kleos-lib/src/handoffs/atoms.rs` (extract_llm, idem)
- Spec agent-forge : `spec_882f64c7`
**Niveau delta upstream** : chirurgical (helper additif + 3 call sites 2 lignes chacun)

### Probleme

Patch 14b avait corrige l'injection `reasoning_effort` dans `broca::call_llm_endpoint`
uniquement. Trois autres call sites construisent des payloads OpenAI-compat directement
sans passer par ce helper :

1. `kleos-lib/src/llm/local.rs::LocalModelClient.call` (utilise par `/skills/*` et le
   Phase 5 `/context include_inference=true` du Patch 16).
2. `kleos-lib/src/services/loom.rs::execute_llm_step` (workflow Loom de type `llm`).
3. `kleos-lib/src/handoffs/atoms.rs::extract_llm` (extraction des atoms session
   handoff).

Sur LXC 121 (Qwen3 via `/v1/chat/completions`) ces 3 chemins envoyaient `think: false`
mais pas `reasoning_effort`. Resultat : Qwen3 ecrit son output dans le reasoning block
ignore par kleos-server, et `LocalModelClient.call` retourne erreur `ollama returned
empty response` apres ~5-6s.

Symptome observable post-deploy Patch 16 : `/skills/capture` echoue avec 500
`Internal error: ollama returned empty response`.

### Solution

Factoriser le snippet Patch 14b dans un helper `pub(crate)`
`inject_openai_compat_reasoning(body: &mut serde_json::Value)` cote `kleos-lib::llm`,
puis l'appeler depuis les 3 sites. broca.rs:758 reste inline (Patch 14b deploye, on
evite de le toucher pour minimiser le delta supplementaire).

```rust
// kleos-lib/src/llm/mod.rs
pub(crate) fn inject_openai_compat_reasoning(body: &mut serde_json::Value) {
    if let serde_json::Value::Object(ref mut map) = body {
        let think = think_enabled();
        map.entry("think".to_string())
            .or_insert_with(|| serde_json::Value::Bool(think));
        let effort = if think { "high" } else { "none" };
        map.entry("reasoning_effort".to_string())
            .or_insert_with(|| serde_json::Value::String(effort.to_string()));
    }
}
```

Helper idempotent : `or_insert_with` preserve toute valeur deja posee par le caller.
No-op sur `serde_json::Value` non-Object. Le param est ignore silencieusement par
Ollama si l'endpoint cible est `/api/generate` natif au lieu de `/v1/chat/completions`,
donc l'appel est sans effet de bord sur les autres deployments.

### Validation post-deploy

```
POST /skills/capture (description: "Deploy a Rust binary to a remote LXC...")
-> HTTP 200 en 32s, slug="deploy-rust-binary-to-lxc", evolution_type="captured", skill_id=1
```

Avant 14c : 500 "ollama returned empty response" en 6s.

### Cas non corriges (intentionnel)

| Call site | Statut | Raison |
|---|---|---|
| `broca.rs:758` | Inline 14b deploye | Eviter d'editer du delta deja en prod |
| `broca.rs:1147+`, `broca.rs:1330+` | Passent par `call_llm_endpoint` | 14b heritee transitivement |
| `chiasm/tasks.rs:682,691` | Passent par `broca::call_llm_endpoint` | 14b heritee transitivement |

### Reference

- Memoire Kleos #2323 (implementation), #2332 (validation empirique)
- Spec agent-forge : `spec_882f64c7`

**Architecture actuelle :** `main` = upstream/main exact (synchronise via fetch +
fast-forward), `local/patches` = branche topic VOCSAP avec ~8 commits semantiques
au-dessus de main. Plus de merge `--allow-unrelated-histories` historique.

### Procedure de rebase recommandee (variante topic branch -- adoptee v1.1.5)

```bash
# 1. Sync upstream sur main
git fetch upstream main
git checkout main
git merge --ff-only upstream/main
git push origin main

# 2. Tags de backup avant rebase
DATE=$(date +%Y-%m-%d)
git tag "backup/local-patches-before-rebase-vX.Y.Z-$DATE" local/patches
git tag "backup/main-before-rebase-vX.Y.Z-$DATE" main
git push origin --tags

# 3. Branche topic + rebase (preserve local/patches intacte pendant le travail)
git checkout -b local/patches-fixupstream local/patches
git rebase main
# Resoudre les conflits patch par patch -- skip ceux absorbes (diff vide).

# 4. Validation
cargo check --workspace --all-features
# Note: cargo check workspace echoue sur Windows pour les crates serveur
# (openssl-sys cross-compile). Faire check par crate plateforme :
#   Windows: -p kleos-sh -p agent-forge -p kleos-sidecar -p kleos-cred -p kleos-approval-tui
#   WSL musl: -p kleos-server -p kleos-cli -p kleos-mcp -p kleos-credd

# 5. Apres validation, aligner local/patches sur la branche topic
git checkout local/patches
git reset --hard local/patches-fixupstream
git push --force-with-lease origin local/patches
git branch -d local/patches-fixupstream

# 6. Cleanup tag eventuel (apres deploy reussi)
# git tag -d backup/main-before-rebase-vX.Y.Z-$DATE  # optionnel
```

**Pourquoi pas un `git merge --ff-only`** : le rebase a reecrit l'histoire, donc
`local/patches-fixupstream` n'est PAS un descendant lineaire de `local/patches`.
Le ff-only echoue avec "Not possible to fast-forward". `reset --hard` est l'operation
adequate puisqu'on veut que `local/patches` adopte la nouvelle histoire.

### Verification rapide : patches deja absorbes par upstream

```bash
# Patches Windows (1, 2, 5A, 5B, 9, 10)
git show main:agent-forge/Cargo.toml | grep "cfg(windows)"
git show main:kleos-sidecar/Cargo.toml | grep "cfg(windows)"
git show main:kleos-approval-tui/Cargo.toml | grep "cfg(windows)"
git show main:kleos-sh/src/main.rs | grep "cfg(not(unix))"
git show main:kleos-cred/src/bin/derive-db-key.rs | grep "cfg(unix)"
# Patch 8B -- embedding backend
git show main:kleos-server/src/main.rs | grep "EMBEDDING_BACKEND"
# Patch 7 -- auth kleos_ prefix
git show main:kleos-lib/src/auth.rs | grep "kleos_\|split_once"
# Patch 8C -- SPA prefix routing
git show main:kleos-server/src/routes/gui/mod.rs | grep "starts_with.*spa"
```

Si la commande retourne du contenu : le patch est absorbe upstream, **supprimer**
le commit local correspondant (drop pendant le rebase interactif). Sinon : le
commit local s'applique normalement.

### En cas de conflit gros patch (ex: refactor architectural upstream)

Cf. exemple `kleos-cli activity` ci-dessus : si un patch fait reference a une
API qui a change upstream (struct extrait dans une crate, champ devenu prive,
signature modifiee), il faut **adapter** plutot que checkout brut. Inspecter
la nouvelle API publique via `git show main:<crate>/src/<file>.rs | grep "pub "`,
puis reecrire le commit local.

---

## Patch 15 -- Dynamic LLM prompt overlay (`KLEOS_LLM_PROMPT_REPOSITORY`)

**Date** : 2026-05-19
**Statut** : code commite sur `local/patches-dynamic-prompt` (HEAD `a84221a`). Lots 1 a 6 termines, Lot 7 (cette doc) en cours.
**Submodules associes** :
- `prompts-overrides/` -> `https://github.com/VOCSAP/Kleos.prompts` (overrides VOCSAP, prive)
- `wiki/Configuration.md` -- doc operateur publique

### Intention

Permettre a un operateur Kleos de surcharger n'importe quel prompt LLM hardcode dans le binaire `kleos-server` (system + user template) **sans recompilation**, via un dossier de fichiers `.txt` sur disque. Ouvre la voie a un tuning de prompts par operateur, par modele, par langue ou par incident. Refactor candidat a PR upstream : aucune regression si l'env var n'est pas definie ni le dossier present.

### Pourquoi

Session 2026-05-19 : on a decouvert qu'un prompt embedded (Broca `ask_plan` system) documente `service="broca"` comme DEFAULT pour les questions d'activite. Mais `actions.service` en DB stocke `"kleos"` (post-fix Patch 13) ou `"engram"` (legacy), **jamais `"broca"`**. Resultat : POST `/broca/ask` retournait `raw=[]` silencieusement parce que le LLM emettait `service=broca` -> 0 ligne SQL match. Sans mecanisme d'override, chaque ajustement de prompt requiert un cycle code -> commit -> build WSL -> deploy LXC complet (typique 15-20 minutes par iteration), trop long pour iterer sur le wording d'un prompt face a un nouveau modele.

Plus largement, les 19 prompts LLM identifies dans le codebase (broca x3, chiasm x1, skills x5, extraction x2, memory x2, growth x4, prompts builder x1, loom fallback x1) sont tous **calibres pour les modeles d'upstream** (en l'occurrence llama3.2:3b ou la famille gpt-4). Pour le deploiement VOCSAP qui tourne sur qwen3:8b-ctx16k (Modelfile derive contexte 16384), certains prompts produisent des reponses sous-optimales et il faut pouvoir les ajuster sans recompiler.

### Solution

#### Mecanisme overlay

```rust
// kleos-lib/src/llm/prompts.rs
pub fn load_prompt(id: &str, embedded_default: &'static str) -> Cow<'static, str>;
pub fn load_pair(prefix: &str, def_sys: &'static str, def_user: &'static str)
    -> (Cow<'static, str>, Cow<'static, str>);
pub fn load_and_render(id: &str, default: &'static str, vars: &Value) -> String;
```

Resolution cascade pour le repo override :
1. `KLEOS_LLM_PROMPT_REPOSITORY` (env var, path explicite) si defini
2. `${KLEOS_DATA_DIR}/prompts` (ou `${ENGRAM_DATA_DIR}/prompts`) si le dossier existe
3. Aucun -- embedded defaults via `include_str!()` bundle dans le binaire au build

Cache : `Arc<String>` partage entre callers, invalidation `mtime` apres TTL 5s. Edits propages sans restart de `kleos-server`.

Interpolation : `kleos-lib/src/llm/template.rs::interpolate(template, &vars)` rend les placeholders `{{path.to.field}}` avec un `serde_json::Value`. Variable manquante -> chaine vide (degrade gracieusement).

#### Layout source-tree

```
kleos-lib/prompts/<service>/<purpose>/{system,user}.txt
```

Chaque `.txt` est embedde dans le binaire via `include_str!()`. Le contenu est byte-equivalent au literal Rust qu'il remplace, donc le binaire produit est identique en taille et en comportement par defaut. Le fichier `.txt` n'est PAS distribue avec le binaire -- il sert uniquement au build.

#### Layout filesystem override

```
${root}/<service>/<purpose>/{system,user}.txt
```

Meme structure cote LXC ou cote dev. La fonction `load_pair(prefix, ...)` construit automatiquement les sous-paths `<prefix>/system` et `<prefix>/user`. Pas de fichier present = fallback embedded.

#### Submodule prompts-overrides

Pour VOCSAP, les overrides actifs vivent dans `https://github.com/VOCSAP/Kleos.prompts` (prive). Le main repo reference ce submodule via `prompts-overrides/` (racine). Cote LXC, l'operateur fait simplement :

```bash
cd /var/lib/kleos
git clone https://github.com/VOCSAP/Kleos.prompts prompts
# Updates :
git -C /var/lib/kleos/prompts pull
```

Le sub embarque son propre `README.md` et `CLAUDE.md` avec catalog detaille et intentions par prompt.

### Migration progress

| Lot | Commit | Prompts |
|---|---|---|
| 1 -- foundations + pilot broca/ask_plan | `04eabaa` | 1 |
| 2 -- broca + chiasm | `e994c69` | 3 |
| 3 -- skills | `14b921b` | 5 |
| 4 -- extraction + memory | `3d6ab46` | 4 |
| 5 -- growth | `41ad013` | 4 |
| 6 -- loom fallback | `a84221a` | 1 |
| 7 -- catalog + doc | (en cours) | 0 |

**17 call sites surchargeables au total**. 2 explicitement hors scope :
- `prompts/agent_header` (build_living_prompt) : builder dynamique multi-sections, externalisation requerrait un templating engine.
- `brain/oracle` : dead code (`#[allow(dead_code)]`).

### Mesure end-to-end (LXC 121, qwen3:8b-ctx16k via /v1/chat/completions)

Test pilote broca/ask_plan, sequence :

1. Baseline (binaire avec embedded fix, aucun fichier override) :
   ```
   plan: {agent: 'Kleos', limit: 50, service: 'kleos', since: None}
   ```
2. Override force `service=loom, limit=3` via `/var/lib/kleos/prompts/broca/ask_plan/system.txt`, sleep 6s :
   ```
   plan: {agent: None, limit: 3, service: 'loom', since: None}
   ```
3. Hot-reload force `service=thymus, limit=11`, sleep 6s :
   ```
   plan: {agent: None, limit: 11, service: 'thymus', since: None}
   ```
4. Cleanup, retour baseline.

**Trois inferences distinctes, AUCUN restart de kleos-server, TTL 5s respecte.** Le cache mtime invalidation + cascade env est operationnel.

### Pourquoi pas DB / pas de cronjob FS->DB

Discute avec l'operateur 2026-05-19 :
- DB ajoute un couplage inutile pour un workflow ou les overrides changent rarement (semaines/mois).
- Le filesystem permet `vim` + `git diff` + `git log` natifs, plus naturel pour iterer.
- Le cache mtime fait quasiment aussi vite qu'une lecture DB (50 us au cold miss vs ~5-8s par appel LLM = 0.001% d'overhead).
- Cronjob FS -> DB serait du double-bookkeeping pour zero gain.

### Reference

- Module : `kleos-lib/src/llm/prompts.rs` + `kleos-lib/src/llm/template.rs`
- Catalog detaille : `docs/dev-notes/llm-prompts-catalog.md`
- Doc operateur : `wiki/Configuration.md` section "LLM Prompts Overlay (VOCSAP Patch 15)"
- Sub overrides : `https://github.com/VOCSAP/Kleos.prompts` (CLAUDE.md + README.md)
- Plan d'execution : `~/.claude/plans/swirling-yawning-twilight.md`
- Memoires Kleos : #2185 (pilote validation), #2197 (architecture finale), #2200 (hot-reload validation), #2208 (lots 1-6 complete)

---

## Patch 16 -- Extension overlay aux suffixes et a `context/inference`

**Date** : 2026-05-19
**Statut** : code complet, deploye sur LXC 121, validation empirique partielle (growth 4/4 services + skills/capture OK; context/inference indirect via skills/capture)
**Fichiers touches** :
- `kleos-lib/src/intelligence/growth.rs` (helper `service_prompt_paths` ajoute, `reflect` re-route via suffix+user)
- `kleos-lib/src/skills/evolver.rs` (3 const remplaces par 9 accessor functions, 9 call sites adaptes)
- `kleos-lib/src/context/mod.rs` (Phase 5 Inference utilise `load_pair` + `interpolate`)
- 19 nouveaux fichiers prompts sous `kleos-lib/prompts/` :
  - `growth/{kleos,claude_code,eidolon,default}_reflection/{system_suffix,user}.txt` (8)
  - `skills/{fix,derive,capture}_prompt/{name,desc,code}_user_suffix.txt` (9)
  - `context/inference/{system,user}.txt` (2)
- `docs/dev-notes/llm-prompts-catalog.md` (35 ids overlayables total : 17 Patch 15 + 18 Patch 16)
- Specs agent-forge : `spec_7c400216` (Lot 1 growth), `spec_56c24f77` (Lot 2 skills), `spec_8153fc3c` (Lot 3 context)
**Niveau delta upstream** : refactor (3 fichiers Rust touches dans des fonctions upstream) + additif (19 nouveaux files)

### Probleme

Patch 15 avait externalise les **persona** prompts (system) et les **user templates principaux** (5 cas), mais 3 categories restaient hardcodees :

1. **Rules systeme** : `growth.rs:209-233` ajoutait 6 bullet rules au persona via `format!("{}{}", system, rules)`. Identiques sur les 4 services growth.
2. **Shot suffixes** : `evolver.rs:34-36` 3 const `NAME/DESC/CODE_SHOT_SUFFIX` reutilises 9 fois (3 fonctions x 3 phases).
3. **Paire `context/inference`** : `context/mod.rs:1019-1029` Phase 5 LLM inference hardcodait `system_prompt` et le format `user_prompt`.

### Solution (convention Option C)

Tous les nouveaux fichiers colocalises par `<service>/<purpose>/` -- pas de dossier `rules/` separe. Deux nouveaux suffixes de fichier :

- `system_suffix.txt` : appende au system par le caller via `format!("{}\n{}", system.trim_end(), suffix.trim_end())`.
- `<phase>_user_suffix.txt` : appende au user par le caller via `format!("...\n\n{}", suffix.trim())`.

Suffix vide neutralise la regle correspondante (escape hatch intentionnel). Reutilisation des helpers Patch 15 `load_prompt`, `load_pair`, `load_and_render`, `template::interpolate`.

Refactor `growth.rs::reflect` : factorisation du routage service via `service_prompt_paths()` qui retourne `{system,suffix,user}_{id,default}`. Compatibilite preservee : `get_prompt_for_service` reste appelle pour le system (alignement Patch 15) et delegue desormais a `service_prompt_paths`.

Refactor `evolver.rs` : 3 const remplaces par 9 const `*_USER_SUFFIX_DEFAULT` + 9 accesseurs `fix_name_user_suffix()`, etc. Chaque fonction (`fix_skill`, `derive_skill`, `capture_skill`) charge ses 3 suffixes en debut puis les injecte dans les 3 `format!`.

Refactor `context/mod.rs:1019` : `load_pair("context/inference", ...)` + `template::interpolate(user_tmpl, vars)` avec `{"query": ..., "top_facts": ...}`.

### Validation

- Build : `cargo build` 0 erreurs (10 warnings pre-existants ECDH cred/bootstrap)
- Tests unitaires : `cargo test -p kleos-lib` 827/827 pass, 4 ignored, zero regression
- Empirique post-deploy :
  - `growth/reflect` 4/4 services (kleos, claude-code, eidolon, default) -> observations 1-3 phrases 1ere personne
  - `skills/capture` -> slug kebab-case, evolution_type="captured" en ~32s (3 LLM calls)
  - `context/inference` -- LocalModelClient valide indirectement via skills/capture (meme code path). Endpoint `/context` bloque par cap upstream 30s lorsque cold cache + LLM call > 30s (cf. Patch 16b).

### Convention

| Suffixe fichier | Concat cote caller | Exemples ids |
|---|---|---|
| `system.txt` | tel quel | `broca/ask_plan/system`, `growth/kleos_reflection/system` |
| `user.txt` | `interpolate(template, vars)` | `broca/ask_plan/user`, `growth/kleos_reflection/user` |
| `system_suffix.txt` | `format!("{}\n{}", system.trim_end(), suffix.trim_end())` | `growth/*/system_suffix` |
| `<phase>_user_suffix.txt` | `format!("...\n\n{}", suffix.trim())` | `skills/fix_prompt/name_user_suffix` etc. |

### Reference

- Plan d'execution : `~/.claude/plans/mossy-launching-origami.md`
- Cartographie initiale : `docs/dev-notes/llm-prompts-rules-todo.md` (gitignored)
- Catalog detaille : `docs/dev-notes/llm-prompts-catalog.md`
- Sub overrides : `https://github.com/VOCSAP/Kleos.prompts` (CLAUDE.md + README.md mis a jour)
- Memoires Kleos : #2287 (passation pre-Patch-16), #2295 (Lot 1 growth), #2296 (Lot 2 skills), #2297 (Lot 3 context), #2299 (recap code), #2302 (doc 3 fichiers), #2305 (politique upstream), #2332 (validation empirique)

---

## Patch 16b -- `KLEOS_CONTEXT_TIMEOUT_SECS` env var pour /context

**Date** : 2026-05-19
**Statut** : code complet, deploiement attendu (operateur ajoute la variable dans `/etc/kleos/kleos.env` avant restart)
**Fichier touche** : `kleos-server/src/routes/context/mod.rs` (one const + one fn + 1 ligne)
**Spec agent-forge** : `spec_b896f973`
**Niveau delta upstream** : additif (constante hardcodee devient configurable, default identique)

### Probleme

Le router `/context` (upstream S7-26) applique un `TimeoutLayer::with_status_code(REQUEST_TIMEOUT, Duration::from_secs(30))` hardcode. Sur LXC 121 avec Qwen3 :
- cold semantic cache : ~14s
- LLM call inference Patch 16 : ~15s
- total > 30s -> HTTP 408

Le default global `KLEOS_REQUEST_TIMEOUT_SECS=1800` (server.rs:19) est ignore par cette route specifique.

### Solution

Ajouter `DEFAULT_CONTEXT_TIMEOUT_SECS = 30` (preserve upstream behaviour) + `context_timeout()` qui lit `KLEOS_CONTEXT_TIMEOUT_SECS` env var et fallback sur la constante :

```rust
const DEFAULT_CONTEXT_TIMEOUT_SECS: u64 = 30;

fn context_timeout() -> Duration {
    let secs = std::env::var("KLEOS_CONTEXT_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_CONTEXT_TIMEOUT_SECS);
    Duration::from_secs(secs)
}
```

Router conserve `S7-26` comment + ajoute "Patch 16b: configurable via KLEOS_CONTEXT_TIMEOUT_SECS, default 30". Default identique au comportement upstream quand l'env var est unset.

### Deploiement operateur

Sur LXC 121, dans `/etc/kleos/kleos.env` :
```
KLEOS_CONTEXT_TIMEOUT_SECS=60
```

Restart kleos-server. Le cap passe a 60s, l'inference Patch 16 entre dans le budget.

### Validation

- Build : `cargo check -p kleos-server` 0 erreurs.
- Default behaviour preserve (env var unset -> 30s, identique upstream).
- Operateur peut ajuster sans rebuild.

### Reference

- Spec agent-forge : `spec_b896f973`
- Memoire Kleos #2332 (decouverte du blocker post-Patch 16)

---

## Patch 17a -- activation `KLEOS_USE_CHUNK_VECTOR_SEARCH` + backfill vector

**Date** : 2026-05-20
**Statut** : deploye sur LXC 121
**Fichiers touches** : aucun (purement operationnel)
**Niveau delta upstream** : **zero delta** (env var deja prevue par upstream commit `4b02bfe`, simplement non activee jusqu'ici)

### Probleme

`POST /context` retournait HTTP 408 a 60 s sur LXC 121 avec 2620 memories actives. Diagnostic via `GET /admin/vector_health` :
- `lance_row_count = 434` sur 2620 memories actives (84% non indexees)
- `chunk_lance_row_count = 0`
- `vector_sync_pending_count = 0` (worker drain mais ne backfill pas l'historique)

Cause : `use_chunk_vector_search = false` par defaut (`kleos-lib/src/config.rs:581`), et la feature opt-in n'avait jamais ete activee depuis son introduction upstream le 2026-04-28 (commit `4b02bfe`). Les 2186 memories sans vecteur Lance tombaient sur le fallback `vector_search` SQLite (sqlcipher decrypt par page, I/O-bound).

### Solution

1. Ajouter `KLEOS_USE_CHUNK_VECTOR_SEARCH=1` dans `/etc/kleos/kleos.env` (backup `/etc/kleos/kleos.env.bak-20260520-103231` cree avant edit).
2. `systemctl restart kleos-server` -- le loader ouvre `chunk_vector_index` au boot tenant.
3. `POST /admin/backfill_chunks` -- backfill complet en deux passes (30 min + 10 min, le TimeoutLayer serveur coupe a 30 min, repassage gere le reliquat ; 354 primary + 355 chunks finalises en 2eme passe, 0 failures).

### Validation post-deploy

- `lance_row_count` : 434 -> 2652 (~100% des memories actives)
- `chunk_lance_row_count` : 0 -> 2216
- `POST /search query="Patch 16"` : 319 ms (canal vector actif)
- `POST /context include_static=false` : 8.78 s HTTP 200 (le bottleneck residuel Phase Static est traite par Patch 17b)

### Reference

- Memoires Kleos #2726 (deploy backfill), #2728 (decouverte Phase Static bottleneck residuel)

---

## Patch 17b -- durcir `get_static_memories` (filtre archived + importance + cap configurable)

**Date** : 2026-05-20
**Statut** : code complet, build WSL + deploy LXC 121 attendus
**Fichier touche** : `kleos-lib/src/context/deps.rs` (1 const + 1 fn + clause WHERE modifiee)
**Niveau delta upstream** : chirurgical (1 fichier source, ~20 lignes ajoutees + 8 modifiees)

### Probleme

Apres Patch 17a (vector index alimente), `POST /context` default (include_static=true) continue de timeout HTTP 408 a 60 s. La Phase 1 de `assemble_context_inner` (`kleos-lib/src/context/mod.rs:509-573`) re-embedde chaque memory statique via Ollama bge-m3 (~50 ms par appel sur LXC 116). Sur LXC 121, `get_static_memories` retourne **2176 statics** au lieu de la dizaine attendue : 2176 x 50 ms = ~110 s, depasse le TimeoutLayer 60 s avant meme le premier event SSE.

Cause racine : `get_static_memories` (`kleos-lib/src/context/deps.rs:70`) ne filtre PAS `is_archived` dans sa clause WHERE. Le dreamer (`kleos-lib/src/intelligence/growth.rs:357`) insere chaque growth observation avec `is_static = 1` ET `is_archived = 1`. Intent upstream : le brouillon archive devient actif uniquement apres promotion explicite en insight (`POST /growth/materialize`, importance bump 7 -> 8, archived remis a 0). Mais le filtre `is_archived` manque cote query, donc les brouillons remontent comme statics actifs.

Comparaison avec les autres call sites du flag `is_static = 1` dans la codebase :

| Fichier | filtre `is_archived` | filtre `importance` |
|---|---|---|
| `context/deps.rs:72` (Phase Static, **bug**) | non | non |
| `pack.rs:57` | =0 | non |
| `gate/mod.rs:386, 571` | non | >=8 |
| `personality.rs:1061` | non | LIMIT 20 |

`pack` et `gate` sont disciplines. `context/deps.rs` ne l'est pas : c'est un oubli upstream.

### Solution

Trois changements dans `get_static_memories` :

1. **Filtre `AND is_archived = 0`** : exclut les brouillons growth, aligne avec `pack.rs:57`.
2. **Filtre `AND importance >= 8`** : ne retient que les insights promus, aligne avec `gate/mod.rs:386, 571`.
3. **`LIMIT ?1` avec valeur configurable** via env var `KLEOS_CONTEXT_STATIC_LIMIT` (default 50), suivant le pattern Patch 16b :

```rust
const DEFAULT_CONTEXT_STATIC_LIMIT: usize = 50;

fn context_static_limit() -> usize {
    std::env::var("KLEOS_CONTEXT_STATIC_LIMIT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_CONTEXT_STATIC_LIMIT)
}
```

ORDER BY est etendu a `importance DESC, source_count DESC, created_at DESC` pour retenir les insights les plus importants, recurrents, recents si tie.

### Effet attendu

- Statics retournees : 2176 -> ~0 actuellement (aucun insight promu en DB sur LXC 121), <= 50 dans le futur
- Phase Static : ~110 s -> <2.5 s meme pire cas
- `POST /context` (default) : timeout 60 s -> sous 10 s

### Deploiement operateur

Build WSL : `cargo build --release --target x86_64-unknown-linux-gnu -p kleos-server`. Puis scp + restart sur LXC 121.

Optionnel : override le default via `/etc/kleos/kleos.env` :
```
KLEOS_CONTEXT_STATIC_LIMIT=30
```

### Validation

- Build : `cargo check -p kleos-lib` doit passer 0 erreurs.
- Default behaviour preserve dans le sens "small per-user" (env var unset -> LIMIT 50).
- Tests existants `deps.rs` non touches.
- Operateur peut ajuster sans rebuild.

### Conditions de retrait

Si upstream merge une PR qui filtre `is_archived` dans `get_static_memories` (et/ou ajoute un parametre `only_promoted`), le Patch 17b devient redondant et peut etre retire au prochain rebase. Surveiller les commits sur `kleos-lib/src/context/deps.rs` upstream.

### Reference

- Memoires Kleos #2728 (decouverte cause racine), #2729 (mecanisme promotion), #2763 (distinction backfill vs Phase Static), #2767 (comportement dreamer by-design)

---

## Patch 17c -- durcissement prompt growth + filter file dynamique

**Date** : 2026-05-20
**Statut** : code complet (build WSL + deploy LXC 121 attendus), prompt override prets dans submodule prompts-overrides
**Fichiers touches** :
- `kleos-lib/src/intelligence/growth.rs` (1 fn ajoutee + validate_observation etendu, ~45 lignes)
- `prompts-overrides/growth/<service>/system_suffix.txt` x 4 (durcissement)
- `prompts-overrides/growth/<service>/user.txt` x 4 (format guidance)
- `prompts-overrides/growth/reject_patterns.txt` (nouveau, opt-in)

**Niveau delta upstream** :
- `growth.rs` : additif pur (nouvelle fn `growth_reject_patterns`) + chirurgical minimal (~5 lignes ajoutees dans `validate_observation`)
- Submodule prompts-overrides : zero delta upstream

### Probleme

Apres Patch 17b (filtre `get_static_memories`), le dreamer continue de generer des brouillons de mauvaise qualite :
- ~58 doublons stylistiques en 100 brouillons (memes faits, formulations differentes)
- Verbosity moyenne 200 chars, 80% de pattern "Je remarque/I noticed que..."
- Validateur upstream (`validate_observation`) trop laxe : seul `NOTHING` exact, `I don't`, `There is nothing` sont rejetes.

Les overlays Patch 15 / 16 existaient mais aucune surcharge sur le `system_suffix` ou `user` des 4 services growth -- on tournait sur les defaults embedded qui n'imposent aucune structure stricte.

### Solution -- deux couches complementaires

**Couche A -- prompt override (zero delta upstream)** : 4 services `{kleos, claude_code, eidolon, default}_reflection` recoivent un `system_suffix.txt` durci qui :
- biaise par defaut vers `NOTHING`
- impose la structure `<fact>. <localisation>. <implication>.` (max 250 chars)
- exige une localisation litterale presente dans l'input (anti-hallucination)
- liste les openings interdits multilingues (`I noticed`, `Je remarque`, etc.)
- formate des examples de bonne et mauvaise forme

Et un `user.txt` qui rappelle le format attendu directement apres le `{{context}}`.

Tests via Ollama direct (qwen3:8b-ctx16k @ temp=0.7, `reasoning_effort: none`) sur 7 scenarios montrent :
- Verbosity divisee par 2 (100 vs 200 chars)
- 2/2 `NOTHING` propres sur contextes vides
- 5/7 sur format `fact.loc.impl.` respecte
- 1/7 hallucination de localisation residuelle, 1/7 quote residuelle (limites du modele)

**Couche B -- filter file externe** : nouveau fichier `prompts-overrides/growth/reject_patterns.txt` lu a chaque appel de `validate_observation` (resolution identique aux prompts : `KLEOS_LLM_PROMPT_REPOSITORY` puis `$KLEOS_DATA_DIR/prompts/growth/reject_patterns.txt`). Format : un substring par ligne, case-insensitive, `#` commentaire. Si match dans l'observation -> reject (traite comme NOTHING).

Initial set de patterns inclut placeholders ` unknown.`, ` general.`, ` system.`, ` terminal.`, label prefixes `implication:`, `fact:`, et quotes inline. **Hot-tunable** : pas de cache, modification effective au prochain cycle dreamer (~30 min). Permet d'ajouter de nouveaux modes d'hallucination sans rebuild.

### Garde-fous

- Si le filter file est absent : aucun pattern applique, comportement identique a upstream. Filter purement opt-in.
- Si pattern trop large : risque de drop d'observations legitimes -> garder la liste conservatrice. Le doc dans le fichier le rappelle.
- Les forbidden openings (`I noticed que...`) sont **doublement** filtrees : prompt-side (LLM essaie de ne pas les emettre) + filter-side (rejet si le LLM bypass la regle).

### Deploiement operateur

1. Build WSL : `cargo build --release --target x86_64-unknown-linux-gnu -p kleos-server`
2. Deploy LXC 121 (scp + systemctl restart)
3. Push submodule : `git -C prompts-overrides add growth && git -C prompts-overrides commit && git push`
4. Pull cote LXC 121 : `git -C /var/lib/kleos/prompts pull`
5. TTL cache 5s prend effet, prochain cycle dreamer (~30 min) utilise les overrides

### Conditions de retrait

- Si upstream merge un `validate_observation` strict equivalent : le filter-side de Patch 17c devient redondant.
- Si upstream introduit ses propres reject patterns par config : aligner les noms d'env vars et retirer le helper local.

### Reference

- Memoires Kleos #2822 (workflow prompt override Patch 17c), #2790 (review brouillons par sub-agent)
- Submodule `prompts-overrides` (`VOCSAP/Kleos.prompts`) : section `growth/` et `reject_patterns.txt`

---

## Patch 17d -- anti-recursion consolidation (Couche A)

**Date** : 2026-05-20
**Statut** : code complet, build WSL + deploy LXC 121 attendus
**Fichier touche** : `kleos-lib/src/intelligence/consolidation.rs` (clause WHERE de `find_consolidation_candidates`, +1 ligne SQL, +9 lignes de commentaire explicatif)
**Niveau delta upstream** : chirurgical minimal

### Probleme

Le mecanisme de consolidation de Kleos (`kleos-lib/src/intelligence/consolidation.rs::sweep` -> `find_consolidation_candidates` -> `consolidate`) cree un memory `source='consolidation', is_static=1, is_archived=0, is_latest=1` par cluster de memories similaires. Sur LXC 121, observe le 2026-05-20 :

- 1924 memories `source='consolidation'` actives en DB
- ~500 nouvelles par jour depuis le 2026-05-17
- Tous les outputs ont `is_consolidated = 0` (jamais consumes par un sweep superieur)

Cause racine : la query `find_consolidation_candidates` (lignes 204-215) filtre `is_forgotten = 0 AND is_latest = 1 AND is_archived = 0` mais **ne filtre PAS `is_consolidated = 0`**. Pourtant `consolidate()` ligne 131 fait bien `UPDATE memories SET is_consolidated = 1 WHERE id = source_id` sur les sources d'une consolidation. Le flag est pose mais jamais consulte cote candidat.

Verification independante par sub-agent Explore (memoire #2851) : confiance 92% que c'est un bug structurel (aucun signal d'intention multi-niveaux contraire). Verification upstream Ghost-Frame/Kleos : aucun fix existant, recents commits sur ce fichier sont du nettoyage Phase 5 (drop user_id).

### Solution

Ajouter dans la clause WHERE de `find_consolidation_candidates` :

```sql
AND ms.is_consolidated = 0 AND mt.is_consolidated = 0
```

Effet : un memory deja consume par une consolidation precedente est exclu des candidates futures. Les outputs neufs de consolidation (qui ont `is_consolidated = 0` par defaut SQL) **restent eligibles** -> preserve la possibilite multi-niveaux future (inspire mnemo dreamer 3-phase, cf. `docs/dev-notes/consolidation-multi-level-dream-todo.md`).

NB : un fix alternatif aurait ete `AND ms.source != 'consolidation' AND mt.source != 'consolidation'`. Abandonne car il aurait bloque le multi-niveaux.

### Conditions de retrait

Si upstream merge un fix equivalent (ajout du filtre `is_consolidated` dans la query), le Patch 17d devient redondant et peut etre retire au prochain rebase. Surveiller commits sur `kleos-lib/src/intelligence/consolidation.rs` upstream.

### Limites connues / non traite par ce patch

Le Patch 17d Couche A **ne resout pas** :

- **L'idempotence intra-niveau** : si le meme cluster revient au sweep suivant (parce que d'autres liens similarity ont change autour mais le cluster cible reste stable), on cree un nouveau consolidated identique au precedent. Solution B (versioning) traitera ce sujet. Cf. `docs/dev-notes/consolidation-versioning-solution-b-todo.md`.
- **Le nettoyage des 1924 outputs deja accumules** : separate, Couche C (SQL UPDATE direct) traite ca en one-shot.
- **Le multi-niveaux propre** : pre-requis sur Solution B + Solution D dediee. Cf. `consolidation-multi-level-dream-todo.md`.

### Reference

- Memoires Kleos #2848 (analyse mecanisme), #2851 (verification sub-agent + upstream clean), #2861 (revision plan post-feedback)
- TODO files : `docs/dev-notes/consolidation-versioning-solution-b-todo.md`, `docs/dev-notes/consolidation-multi-level-dream-todo.md`

---

## Patch 18 -- allowlist d'outils kleos-mcp (`KLEOS_MCP_TOOL_ALLOWLIST`)

### Symptome

`kleos-mcp` expose une registry MCP de 474 routes canoniques + aliases (~600 entrees `tools/list`) cote client. La majorite (`admin.*`, `identity*`, `auth_keys*`, `security.*`, services Syntheos internes ...) n'est pas appelable utilement par un agent LLM. Pollution forte de la registry cote Claude Code, ralentissement de la selection d'outils, augmentation de la surface d'erreur.

Audit /audit LXC 121 sur 14 jours : 45 paths distincts utilises = 9.5 % du catalogue ; top 8 = 92 % du trafic.

### Approche

Niveau **additif pur** sur `kleos-mcp/src/tools.rs::registry()` : lecture de l'env var `KLEOS_MCP_TOOL_ALLOWLIST` (CSV de patterns glob-lite), filtre des routes au moment d'emettre la `tools/list`. Defaut (var unset/vide) = comportement upstream strictement identique.

Pourquoi cette approche plutot que toucher `kleos-client/src/routes.rs::ROUTES` :
- `ROUTES` est la constante canonique partagee avec le runtime client HTTP. La toucher modifierait aussi le dispatcher kleos-client, hors scope.
- Filtre `registry()` cote MCP, pas `dispatch()` (permissif). Le but est de cacher les tools cote LLM, pas de durcir un boundary de securite (cote `kleos-server` scope check).
- Matcher manuel (`*`, suffix `.*`, exact) -- 15 lignes -- pour eviter d'ajouter une dep crate `glob`/`globset` qui elargit le delta.

### Fichiers touches

- `kleos-mcp/src/tools.rs` : ajout `parse_allowlist`, `matches_pattern`, `allowed` ; filtre dans `registry()` ; 7 tests unitaires inline (matcher + parsing + registry integration).
- `docs/dev-notes/kleos-mcp-routes-inventory.csv` (etape 1 du plan)
- `docs/dev-notes/kleos-mcp-routes-classified.csv` (etape 2)
- `docs/dev-notes/kleos-mcp-routes-usage-30d.txt` (etape 3)
- `docs/dev-notes/kleos-mcp-profiles.md` (etape 4)
- `docs/dev-notes/extract-routes-inventory.py`, `docs/dev-notes/classify-routes.py` (outillage etapes 1-2)

### Niveau delta

**Additif pur** : aucune ligne upstream supprimee ni renommee. La fn `registry()` recoit 3 lignes d'appel a `allowed(...)` en plus. Le reste est de nouvelles fonctions et un bloc `#[cfg(test)] mod tests`. Aucune nouvelle dependance Cargo.toml.

### Convention de syntaxe `KLEOS_MCP_TOOL_ALLOWLIST`

- Var unset ou vide -> comportement upstream (toutes les routes).
- CSV de patterns separes par virgules. Whitespace trimme. Entries vides ignorees.
- Match exact : `memory.store`.
- Suffix wildcard : `memory.*` (matche `memory.store`, `memory.recall`, ... ; matche aussi `memory` bare).
- `*` seul matche tout.
- Pas d'autre forme (`?`, `[abc]`, `**`, prefix wildcard).
- Filtre porte sur les **noms canoniques**. Aliases suivent leur canonical.

### Tests

Unitaires (`cargo test -p kleos-mcp --lib --features 'kleos-lib/bundled-sqlite' tools::`) :
1. `matches_exact` -- match exact OK, non-match OK.
2. `matches_suffix_wildcard` -- `memory.*` matche `memory.store`, `memory`, mais pas `memories.recall` ni `memorystore`.
3. `matches_star_alone` -- `*` matche tout.
4. `allowed_with_empty_allowlist_is_permissive` -- `None` = passe-tout.
5. `allowed_with_patterns_filters` -- combinaison patterns.
6. `parse_allowlist_handles_whitespace_and_empties` -- parsing CSV avec whitespace et entries vides.
7. `registry_includes_aliases_only_for_allowed_canonicals` -- integration `registry()` : `memory.store` allowlist -> canonical + alias `memory_store` presents ; `memory.recall` absent.

Tests serialisent via `static ENV_LOCK: Mutex<()>` pour eviter les races sur `std::env::set_var` en parallele (cargo test --test-threads par defaut).

### Conditions de retrait

Si upstream Ghost-Frame absorbe le patch (candidate PR upstream legitime, beneficie a tous les forks consommateurs MCP), retirer du fork. Sinon, ce patch est stable : aucune evolution attendue tant que `ROUTES` ne change pas drastiquement de structure.

### Reference

- Plan parent : `docs/dev-notes/kleos-mcp-allowlist-plan-todo.md` (6 etapes)
- Reference exploration : `docs/dev-notes/kleos-mcp-usage.md`
- Profils proposes : `docs/dev-notes/kleos-mcp-profiles.md`
- Memoires Kleos #2866 (env vars), #2867 (architecture 474 routes), #2876 (plan)

---

## Hooks VOCSAP -- decalage permanent vs upstream

**Contexte general** : upstream Ghost-Frame a retire le bundle `hooks/*` du repo dans son cycle "repo hygiene" (cf. Patch 12). VOCSAP conserve `hooks/full/*.sh` + `hooks/simple/*.sh` et les fait evoluer pour rester compatibles avec le code serveur courant (routes Axum, signatures d'auth, conventions env vars). Cette section consolide toutes les modifications cumulatives. **Tant qu'upstream ne re-introduit pas de hooks, le decalage est permanent et n'a pas de cible de retrait.**

### Inventaire des fichiers concernes (au 2026-05-21)

```
hooks/full/
  lib-eidolon.sh                       # helper shared (URL/key resolution)
  session-start-kleos.sh               # SessionStart bootstrap
  session-end.sh                       # SessionEnd (renomme cote deploy: session-end-kleos.sh)
  enforce-agent-forge.sh               # PreToolUse Write/Edit, gate spec_task
  enforce-kleos-search.sh              # PreToolUse, gate kleos-cli search
  track-agent-forge.sh                 # PostToolUse, marker file pour spec/verify
  mnemonic-observe.sh                  # PostToolUse, fire-and-forget vers kleos-sidecar
  user-prompt-lean.sh                  # UserPromptSubmit, context injection lean
  eidolon-supervisor-drain-pending.sh  # PreToolUse, drain /supervisor/pending (NOUVEAU 2026-05-21)
hooks/simple/
  session-start.sh, session-end.sh, user-prompt.sh, mnemonic-observe.sh
```

### Historique cumulatif des modifications

#### 2026-05-20 -- Coherence avec kleos-server routes

Source : memoire Kleos #2871 (analyse) + #2875 (3 fixes deployes).

| Fichier:ligne | Avant | Apres | Pourquoi |
|---|---|---|---|
| `enforce-agent-forge.sh:85-94` | `POST /gate/check {tool_name, tool_input.file_path}` | `POST /gate/check GateCheckRequest{command, agent, tool_name, context, skip_approval=true}` | Body avant ne matchait pas le type serde cote serveur (`kleos-lib/src/gate/mod.rs:33-49`), desserialization 422. `skip_approval=true` car le hook gere son propre state-file gate. |
| `session-end.sh:60-66` | `POST /gate/complete {session_id, summary}` | `POST /gate/complete-latest {session_id, output, known_secrets:[]}` | La route `/gate/complete` attend `gate_id`, pas `session_id`. La route correcte pour terminer par session est `/gate/complete-latest` (`CompleteLatestBody`). Renomme `summary` -> `output`. Restaure l'enforcement Engram-store post-session. |
| `session-start-kleos.sh:210-220` | `GET /growth/materialize?service&limit&max_bytes` | bloc commente avec TODO | La route serveur attend `POST {observation_id i64}` et materialise UNE observation. Semantique completement differente d'un export markdown agrege. GROWTH.md ne sera plus rafraichi par SessionStart tant qu'une vraie route d'export n'existe pas (candidat PR upstream `GET /growth/digest`). |

Sources verifiees au 2026-05-20 : `kleos-lib/src/gate/mod.rs:33-49`, `kleos-server/src/routes/gate/types.rs:11-24`, `kleos-server/src/routes/growth/mod.rs:20-25`.

#### 2026-05-21 -- Bloc ensure_eidolon_running + drain hook + cascade env vars

Source : memoires Kleos #2918, #2919, #2920, #2924, #2925.

**A. `session-start-kleos.sh` -- ajout du bloc `ensure_eidolon_running`**

Insertion apres `rm -f $STATE_DIR/engram-searched`. Detecte si le process `eidolon-supervisor.exe` tourne via `tasklist.exe`. Sinon, charge `KLEOS_API_KEY` (cascade env -> `~/.config/eidolon/kleos-api-key.txt`), pose `KLEOS_SERVER_URL` + `HOME` puis lance le binaire detache via `powershell.exe Start-Process -WindowStyle Hidden`. Logs binaire dans `~/.claude/logs/eidolon-supervisor.{out,err}.log`. Non bloquant -- toute erreur est logguee et ignoree.

**B. `session-start-kleos.sh` -- patches A+B `kleos-cli`**

Lignes 218-225 et 241-262. Les flags `--json`, `--quiet`, `--budget` n'existent pas sur `kleos-cli list` ni `kleos-cli context` (verifie via `--help` : seuls `--limit` et `--offset`). Reecriture :
- `list --limit 5` (sans flags fantome), output texte brut affecte directement a `RECENT_MEMORIES` (le format `#ID [score] content` est deja lisible, on retire le parsing python obsolete).
- `context "..." --limit 8`, output JSON parse en python pour extraire `memories[].category+content`.

**C. Nouveau hook `eidolon-supervisor-drain-pending.sh`**

PreToolUse standalone, matcher `.*`, timeout 5s. Lit `session_id` depuis stdin JSON, source `lib-eidolon.sh`, GET `/supervisor/pending?session_id=...`. Parse la reponse : exit 2 si au moins une violation Critical en attente (block tool call avec message stderr), exit 0 + stderr sinon, exit 0 silent si aucune violation. Erreurs reseau / parse -> exit 0 (jamais bloquer sur panne d'infra).

**D. Cascade env vars URL+KEY sur 3 hooks**

Alignee sur le pattern upstream `kleos-sh/src/main.rs:310-313`.

- URL : `KLEOS_SERVER_URL -> KLEOS_URL -> ENGRAM_EIDOLON_URL -> EIDOLON_URL -> default http://127.0.0.1:4200`.
- KEY : `KLEOS_API_KEY -> EIDOLON_API_KEY -> cred get eidolon -> ~/.config/eidolon/kleos-api-key.txt`.

Fichiers touches :
- `lib-eidolon.sh` -- nouveau header, `_EIDOLON_URL` resolu via cascade, `eidolon_key()` reecrit avec 4 niveaux.
- `session-end.sh:127-138` -- `EIDOLON_URL_END` + `EIDOLON_KEY_END` alignes sur le meme pattern.
- `session-start-kleos.sh:109-126` -- bloc `ensure_eidolon_running` adopte la meme cascade pour `export KLEOS_API_KEY` et `export KLEOS_SERVER_URL`. Default `http://127.0.0.1:4200` (au lieu du hardcode `192.168.10.21:4200`).

Default URL legacy `localhost:7700` supprime partout : le service standalone "Eidolon" n'existe plus, les routes (`/gate/*`, `/activity`, `/prompt/generate`, `/growth/*`, `/supervisor/*`) sont toutes sur `kleos-server`.

**E. README.md dans `eidolon-supervisor/`**

Ajout d'un README operateur (10K) : env vars table, default rules + VOCSAP custom set 9 rules JSON copy-pastable, severity guide, architecture 3-canaux (`/supervisor/inject`, `/inbox`, `/axon/publish`), procedure de deploiement Linux/Windows/interactif, checks operationnels (SQL + curl), stubs `#[allow(dead_code)]` non implementes (`drift`, `scope`). Niveau delta upstream : additif pur sur dossier upstream, candidat PR upstream legitime.

### Registration cote `~/.claude/claude-config/settings.json` (etat 2026-05-21)

Les hooks repo sont copies (pas symlinks) dans `~/.claude/hooks/`. Etat reel apres retrait par l'operateur des 5 hooks non encore audites :

| Event | Matcher | Hook | Type | Statut |
|---|---|---|---|---|
| `SessionStart` | `""` | `graphify-check.ps1` | externe | actif |
| `SessionStart` | `""` | `session-start-kleos.sh` | VOCSAP | actif |
| `SessionEnd` | `""` | `session-end-kleos.sh` | VOCSAP | actif |
| `UserPromptSubmit` | `^/octo:` | `multi-llm-dispatch.sh` | externe | actif |
| `PreToolUse` | `Bash` | `rtk-rewrite.sh` | externe | actif |
| `PreToolUse` | `Bash` | `git-commit-guard.py` | externe | actif |
| `PreToolUse` | `Write\|Edit\|MultiEdit` | `file-write-scanner.py` | externe | actif |
| `PreToolUse` | `Write\|Edit\|MultiEdit` | `enforce-agent-forge.sh` | VOCSAP | actif (audite 2026-05-20) |
| `PreToolUse` | `.*` | `eidolon-supervisor-drain-pending.sh` | VOCSAP | actif |
| `PostToolUse` | `.*` | `mnemonic-observe.sh` | VOCSAP | actif (audite + patche 2026-05-21) |
| `PostToolUse` | `Bash` | `post-tool-kleos-prompt.sh` | VOCSAP | actif (audite + patche 2026-05-21) |
| `UserPromptSubmit` | `""` | `user-prompt-lean.sh` | VOCSAP | actif (audite + patche 2026-05-21) |
| `PostToolUse` | `.*` | `track-agent-forge.sh` | VOCSAP | actif (audite OK 2026-05-21, aucun patch) |
| `PreToolUse` | `.*` | `enforce-kleos-search.sh` | VOCSAP | actif (audite + patche 2026-05-21, gate BLOQUANT) |
| `PreToolUse` | `Bash\|Write\|Edit\|MultiEdit` | `kleos-sh.exe --claude-hook` | upstream Ghost-Frame | actif (active 2026-05-21, gate BLOQUANT via /gate/check + /approvals) |

**Fix connexe 2026-05-21 -- session-start-kleos.sh : bloc Mnemonic Node.js legacy remplace par lancement detache du binaire Rust `kleos-sidecar.exe`**. L'ancien bloc (lignes 155-167) cherchait `~/.local/lib/mnemonic/index.ts` (Node.js legacy upstream Ghost-Frame) qui n'a jamais existe sur les postes VOCSAP -> log "Mnemonic binary not found", sidecar jamais lance. Nouveau bloc `ensure_kleos_sidecar_running` calque sur `ensure_eidolon_running` (PowerShell `Start-Process` detache, cascade URL `KLEOS_URL -> KLEOS_SERVER_URL -> ENGRAM_EIDOLON_URL -> EIDOLON_URL`, fallback `KLEOS_API_KEY` via fichier `~/.config/eidolon/kleos-api-key.txt`). Niveau delta : chirurgical sur fichier VOCSAP-only. Necessite que `KLEOS_SIDECAR_WATCH` soit `true|false` (pas `0|1`) cote env operateur -- clap `bool` strict, contrairement a `KLEOS_SIDECAR_LLM_THINK` qui accepte `1|true|yes|on`.

**Hooks deployes mais retires en attente d'audit (2026-05-21)** :

| Hook | Event prevu | Raison du retrait |
|---|---|---|
| `enforce-kleos-search.sh` | PreToolUse `.*` | **Audite 2026-05-21 -> PATCH applique + remis en service (matcher elargi a `.*`)**. Gate BLOQUANT (exit 2) qui force un kleos-cli (search/context/recall/recall-due) avant tout tool Write/Edit/Bash non-bootstrap. 3 patches chirurgicaux : (a) regex Bash etendue de `search` a `(search\|context\|recall\|recall-due)` car `context` est la commande primaire recommandee par KLEOS.md (et n'aurait pas pose le stamp avant patch), (b) ajout d'un case `mcp__kleos__*` always-allow + auto-stamp pour rendre le profil Patch 18 (kleos-mcp Standard) operationnel cote workflow, (c) message d'erreur reformule pour mentionner `kleos-cli context` et l'alternative MCP. Matcher elargi de `Bash` (historique #2927) a `.*` pour aligner avec l'intent du hook ("blocks ALL tool calls" -- commentaire ligne 2). Stamp file path stable `~/.claude/session-env/engram-searched` (cleared par session-start-kleos.sh:64). Detail dans `docs/dev-notes/hooks-audit-enforce-kleos-search-todo.md`. |
| `user-prompt-lean.sh` | UserPromptSubmit `""` | **Audite 2026-05-21 -> PATCH applique + remis en service**. Hook UserPromptSubmit (5 regles MANDATORY + recall top-3 memoires). Patch chirurgical 3 axes : (a) flags fantomes `--json --quiet` sur kleos-cli search remplaces par `kleos-cli context` (JSON natif), (b) header Bearer `KLEOS_SIDECAR_TOKEN` ajoute conditionnellement au POST `/recall` (sinon 401 silencieux), (c) cascade env vars `KLEOS_* -> ENGRAM_*` (fallback legacy) sur CLI/API_KEY/SIDECAR_URL/SIDECAR_TOKEN. Mentions "Engram" -> "Kleos" dans regles 2 et 4 pour alignement avec session-start-kleos.sh. Caveat ops (hors hook) : `/recall` retourne 502 sans `KLEOS_NET_ALLOW_PRIVATE=1` cote sidecar (SSRF guard sur 192.168.10.21 RFC1918), le fallback CLI prend le relais. Detail dans `docs/dev-notes/hooks-audit-user-prompt-lean-todo.md`. |
| `post-tool-kleos-prompt.sh` | PostToolUse Bash | **Audite 2026-05-21 -> PATCH applique + remis en service**. Hook 100% local (aucun appel reseau), state-machine sur marker `~/.claude/session-env/last-bash-error` (Bash error -> Bash success -> prompt store Kleos). Patch chirurgical 1 ligne : format JSON output passe de `{"type":"text","text":...}` (schema non reconnu, system-reminder silently dropped) a `{"hookSpecificOutput":{"hookEventName":"PostToolUse","additionalContext":...}}` aligne sur session-start-kleos.sh et user-prompt-lean.sh. Detail dans `docs/dev-notes/hooks-audit-post-tool-kleos-prompt-todo.md`. |
| `track-agent-forge.sh` | PostToolUse `.*` | **Audite 2026-05-21 -> verdict OK (aucun patch) + remis en service**. Hook 100% local, state-machine sur 4 marker files dans `~/.claude/session-env/`. Transitions verifiees runtime : spec-task/log-hypothesis -> ecrit `agent-forge-active`, verify/challenge-code/session-diff -> ecrit markers correspondants, log-outcome -> cleanup. CLI sous-commandes verifiees contre `agent-forge --help`. Aliases MCP legacy preserves. Mineur documente : 3 markers (verified/challenged/diffed) produits mais jamais lus par `enforce-agent-forge.sh` (seul `agent-forge-active` consomme). Dead-code latent, probable vestige feature "completion gating" non deployee. Detail dans `docs/dev-notes/hooks-audit-track-agent-forge-todo.md`. |
| `mnemonic-observe.sh` | PostToolUse `.*` | **Audite 2026-05-21 -> PATCH applique + remis en service**. Route `/observe` et payload `{tool, tool_name, summary, content}` compatibles `ObserveBody` (kleos-sidecar/src/routes.rs:293-304). Patch chirurgical : (a) cascade `KLEOS_SIDECAR_URL -> ENGRAM_SIDECAR_URL -> http://127.0.0.1:7711`, (b) Bearer `KLEOS_SIDECAR_TOKEN` injecte conditionnellement dans le subprocess curl. Validation E2E : POST avec Bearer -> `{accepted:true, pending:N}` ; sans Bearer -> HTTP 401. Detail dans `docs/dev-notes/hooks-audit-mnemonic-observe-todo.md`. |

Chacun fera l'objet d'une analyse dediee (audit ligne par ligne contre le code kleos-server + kleos-sidecar Rust courant, comme fait pour `session-start-kleos.sh` en cette session) avant remise en service.

### Conditions de retrait

Aucune. Tant qu'upstream ne re-introduit pas le bundle `hooks/*` avec une logique equivalente, ces fichiers vivent en permanence dans `local/patches`. Une partie peut etre proposee en PR upstream (notamment le drain hook qui complete la route `/supervisor/pending` deja presente upstream sans drainer).

---

## Patch 19 -- kleos-mcp stdio newline framing

### Symptome

Cote client Claude Code, branchement de `kleos-mcp` via `.mcp.json` produit :
```
debug: Starting connection with timeout of 30000ms
debug: Connection timeout triggered after 30011ms (limit: 30000ms)
error: Connection failed: MCP server "kleos" connection timed out after 30000ms
```

Tout client MCP conforme (Claude Code, Claude Desktop) timeout 30s sur `initialize`. Le binaire est inutilisable comme MCP server, alors qu'il est explicitement vendu comme tel.

### Cause racine

`kleos-mcp/src/transport/stdio.rs` implementait le framing **LSP / Language Server Protocol** (`Content-Length: NN\r\n\r\n{body}`) au lieu du framing **MCP stdio** (newline-delimited JSON) requis par la spec (`https://modelcontextprotocol.io/specification/2025-03-26/basic/transports#stdio`).

Origine probable : copie-colle d'un template Language Server Rust (rust-analyzer, tower-lsp, etc.) sans verifier que MCP a un framing different. La confusion est facile : "JSON-RPC sur stdio" -> LSP dans la tete d'un dev qui en a fait avant.

Diag empirique par peer `desktop-7b2civn-kleos-8` (test A/B sur kleos-mcp.exe 1.1.2) :
- newline-delimited stdin -> stdout 0 byte, jamais EOF (le serveur attend Content-Length).
- LSP stdin (`Content-Length: NN\r\n\r\n{body}`) -> reponse en 6ms.

Git log confirme : commits `2dcebf0` (creation initiale par Ghost-Frame), `7dee90d` (complete route registry), `92a94bc` (consolidation pipeline + auth middleware integration tests). Aucun patch VOCSAP sur ce fichier avant Patch 19.

### Approche

Patch chirurgical sur `kleos-mcp/src/transport/stdio.rs` uniquement. Niveau **chirurgical**, candidat PR upstream legitime (bug fix conforme spec, aucune option configurable VOCSAP-specifique). Pas d'env var ajoutee.

- `read_message<R: BufRead>` : `read_line` + cap sur `line.len()` AVANT parser (defense OOM reelle) + skip blank lines + `trim_end_matches(\n|\r)` (tolerance CRLF defensive sur Windows pipes) + `serde_json::from_str` + return `Some(Value)`. EOF -> `Ok(None)`.
- `write_message<W: Write>` : `serde_json::to_vec` + write_all body + write_all `b"\n"` + flush. Pas de CR.
- `const MAX_MCP_MSG_SIZE: usize = 10 * 1024 * 1024` preservee (SEC-C4 upstream), deplacee au top du fichier pour visibilite.
- `serve(app: App)` : signature publique inchangee.

Choix de l'approche (a) `BufRead::read_line` plutot que (b) `serde_json::Deserializer::from_reader().into_iter::<Value>()` :
- (a) permet d'enforcer le cap sur la taille brute AVANT le parser. (b) complique le cap par-message.
- (a) skip blank lines defensives en `continue` direct.
- (a) garde "1 ligne = 1 message" lisible cote PR upstream (la spec MCP est ecrite en ces termes).

### Fichiers touches

- `kleos-mcp/src/transport/stdio.rs` : refactor `read_message`/`write_message` (additif net : +22 -27 sur les fns existantes), plus ajout d'un nouveau module `#[cfg(test)] mod tests` avec 8 tests (gap upstream comble).

Inchanges :
- `kleos-mcp/src/transport/http.rs` (utilise `axum::Json<Value>`, hors scope).
- `kleos-mcp/src/transport/mod.rs`, `kleos-mcp/src/lib.rs`, `kleos-mcp/src/main.rs`, `kleos-mcp/tests/integration.rs`.

### Tests

8 nouveaux tests inline dans `kleos-mcp/src/transport/stdio.rs::tests` :

1. `read_message_parses_one_line` -- ligne JSON valide -> Value.
2. `read_message_tolerates_crlf` -- ligne CRLF -> Value.
3. `read_message_skips_blank_lines` -- skip "\n\n" -> ligne suivante.
4. `read_message_returns_none_on_eof` -- input vide -> Ok(None).
5. `read_message_returns_none_after_trailing_blank` -- input "\n" puis EOF -> Ok(None).
6. `read_message_rejects_oversized_line` -- ligne > MAX_MCP_MSG_SIZE -> Err contains "exceeds max".
7. `write_message_emits_compact_json_plus_newline` -- 1 seul `\n`, pas de CR, body roundtrippable.
8. `round_trip_write_then_read` -- write puis read -> meme Value, second read -> None.

Resultat : `cargo test -p kleos-mcp --features 'kleos-lib/bundled-sqlite'` -> **18 passed** (8 stdio + 7 tools::tests Patch 18 + 3 integration).

### Validation empirique

Smoke tests CLI Windows (Git Bash) :

| Test | Resultat |
|---|---|
| Run 1 : framing newline, no allowlist | 2 lignes JSON, 0 `Content-Length`, 513 tools (sans filtre) |
| Run 2 : framing newline + allowlist Standard | 2 lignes JSON, **81 tools** (84% reduction), 6/6 MUST_PRESENT, 0/4 MUST_ABSENT |
| Run 3 : negative LSP framing (anti-regression) | exit 101, stderr `"expected value at line 1 column 1"` (parser refuse `Content-Length:`) |

Validation peer kleos-8 (test stdio direct sur binaire Patch 18) avait deja montre 513 -> 81 tools avec framing LSP. Patch 19 preserve ce comportement avec framing newline conforme spec.

### Niveau delta

**Chirurgical** sur fichier upstream pur. Aucune dependance Cargo.toml ajoutee. Aucune env var.

### Conditions de retrait

Drop du patch local lorsque l'une des conditions suivantes est vraie :

1. PR upstream Ghost-Frame merge un commit equivalent qui remplace le framing LSP par newline-delimited.
2. Absorption semantique non identique (refactor a `mcp-sdk-rs` ou equivalent) -> resolution naturelle au rebase via `git rebase --skip` (diff devient vide).

PR upstream legitime : (i) bug qui rend le binaire inutilisable cote tout client MCP conforme, (ii) aucune option configurable VOCSAP-specifique, (iii) aligne le code avec la spec officielle. Probabilite d'acceptation tres elevee.

Titre suggere pour la PR : `fix(kleos-mcp): use newline-delimited JSON framing per MCP spec`.

### Reference

- Spec MCP stdio : `https://modelcontextprotocol.io/specification/2025-03-26/basic/transports#stdio`
- Memoires Kleos #2940 (decouverte + analyse), #2933 (validation binaire Patch 18 OK), #2931 (peer outcome)

---

## Patch 19b -- Cascade operator-first pour blocked + require_approval gate patterns

### Symptome / motivation

La GateConfig (`kleos-lib/src/config.rs::GateConfig`) charge les patterns de
deny **une seule fois au boot** depuis deux sources : defaults Rust (10
patterns destructifs hardcodes) + override CSV via env var
`ENGRAM_EIDOLON_GATE_BLOCKED_PATTERNS`. Aucun hot-reload, aucun fichier
dedie, aucun moyen d'**ecraser** entierement les defaults sans recompiler
ou redemarrer.

Manque aussi un cran intermediaire entre "allowed" et "blocked" : un pattern
qui declenche `requires_approval=true` + creation d'une entree
`/approvals/pending` consommable par `engram-approval-tui`, sans bloquer
silencieusement. Le seul declencheur actuel etait `has_secret_placeholders`
apres credd resolve.

### Approche

Nouveau module additif `kleos-lib/src/gate/approval_patterns.rs` (~110
lignes hors tests) qui expose une primitive `load(file, env, defaults)`
implementant une cascade **exclusive** :

1. Fichier dedie (presence = priorite absolue, MEME vide). Defaut auto-
   resolu sous `${KLEOS_DATA_DIR}/gate/<categorie>.txt` quand
   `KLEOS_DATA_DIR` est defini ; override explicite via
   `ENGRAM_EIDOLON_GATE_<categorie>_FILE`.
2. Env var (set, MEME chaine vide = priorite). Existante pour
   `blocked_patterns` (`ENGRAM_EIDOLON_GATE_BLOCKED_PATTERNS`) ;
   nouvelle pour `require_approval_patterns`
   (`ENGRAM_EIDOLON_GATE_REQUIRED_APPROVAL_PATTERNS`).
3. Defaults Rust (10 patterns pour `blocked_patterns`, `Vec::new()` pour
   `require_approval_patterns`).

Format fichier (commun aux deux familles) : 1 pattern par ligne, lignes
`#` et blanches ignorees, `trim()` applique. Cache lecture **TTL 5s**
par chemin (parite avec `kleos-lib/src/llm/prompts.rs::TTL_SECS` pose
par Patch 17c).

Insertion d'un bloc **2bis** dans `check_command_with_context` apres le
deny et avant le bloc SSH : quand un pattern `require_approval` matche,
le gate stocke avec status DB `pending_approval` (nouvelle valeur, statut
DB est `TEXT` libre donc pas de migration) et retourne
`requires_approval=true` pour que le caller hand-off vers
`engram-approval-tui`.

Le hardcoded `check_dangerous_patterns` (`validator.rs:23-294`) reste
**intouchable** : garde-fou ultime, non configurable par design.

### Niveau de delta

- `kleos-lib/src/gate/approval_patterns.rs` (NEW) -- additif pur, 9 tests
  inline + cache OnceLock.
- `kleos-lib/src/gate/mod.rs` -- (1) declaration `pub mod`, (2) extension
  signature `check_command_with_context` (nouveau param
  `require_approval_patterns: &[String]`), (3) insertion bloc 2bis, (4) 4
  tests Patch 19b inline. Chirurgical + additif.
- `kleos-lib/src/config.rs` -- 3 nouveaux champs `GateConfig`
  (`require_approval_patterns`, `blocked_patterns_file`,
  `require_approval_patterns_file`) avec `#[serde(default)]`, helper
  `gate_data_file()`, 4 nouveaux env loaders. Additif pur, defaults
  preservent comportement upstream.
- `kleos-server/src/routes/gate/mod.rs` -- caller `/gate/check`
  re-route les 2 familles via `approval_patterns::load(...)`. Backward-
  compat : sans fichier ni env, defaults bottent comme avant. Chirurgical.

Niveau global : chirurgical + additif pur, candidat **PR upstream** si
Ghost-Frame adopte la notion de patterns 3-niveaux.

### Fichiers touches

| Path | Action |
|---|---|
| `kleos-lib/src/gate/approval_patterns.rs` | NEW (loader + cache + 9 tests) |
| `kleos-lib/src/gate/mod.rs` | extension signature + bloc 2bis + 4 tests |
| `kleos-lib/src/config.rs` | 3 champs `GateConfig` + helper + 4 env loaders |
| `kleos-server/src/routes/gate/mod.rs` | route les 2 familles via cascade |
| `docs/dev-notes/local-patches.md` | cette section |
| `CLAUDE.md` projet | mise a jour section Convention env vars |

### Env vars + paths fichiers (resume)

| Variable | Defaut | Effet |
|---|---|---|
| `KLEOS_EIDOLON_GATE_BLOCKED_PATTERNS` | unset | CSV legacy, niveau 2 de la cascade pour `blocked_patterns` |
| `KLEOS_EIDOLON_GATE_BLOCKED_PATTERNS_FILE` | `${KLEOS_DATA_DIR}/gate/blocked_patterns.txt` | Path explicite, niveau 1 |
| `KLEOS_EIDOLON_GATE_REQUIRED_APPROVAL_PATTERNS` | unset | CSV nouveau, niveau 2 pour `require_approval_patterns` |
| `KLEOS_EIDOLON_GATE_REQUIRED_APPROVAL_PATTERNS_FILE` | `${KLEOS_DATA_DIR}/gate/require_approval_patterns.txt` | Path explicite, niveau 1 |

Tous les prefixes `KLEOS_*` sont traduits en `ENGRAM_*` au boot par
`config::migrate_env_prefix()`, convention VOCSAP.

### Tests

- Unit : `cargo test -p kleos-lib --features bundled-sqlite gate::approval_patterns::`
  -- 9 cas (cascade tous niveaux, vide intentionnel x2, parser comments,
  parser CSV, cache TTL).
- Integration : `cargo test -p kleos-lib --features bundled-sqlite gate::tests::patch19b_`
  -- 4 cas (`require_approval_pattern_returns_pending_approval`,
  `blocked_pattern_still_wins_over_require_approval`,
  `no_match_passes_through_unchanged`,
  `pending_secrets_still_set_after_no_approval_match`).
- Empirique post-deploy : POST `/gate/check` avec command matchant un
  pattern require_approval, verifier creation entree
  `/approvals/pending` et acceptation depuis `engram-approval-tui`.

### Workflow operateur

```bash
# Initial deploy LXC 121
ssh root@192.168.10.21
mkdir -p /var/lib/kleos/gate
cat > /var/lib/kleos/gate/require_approval_patterns.txt << 'EOF'
# Patterns qui declenchent une approval interactive via engram-approval-tui
# Hot-tunable, TTL 5s
apt install
apt-get install
docker rm
docker rmi
systemctl restart
git push
npm publish
EOF
chmod 644 /var/lib/kleos/gate/require_approval_patterns.txt
systemctl restart kleos-server

# Live edit anytime, no restart
echo "kubectl apply" >> /var/lib/kleos/gate/require_approval_patterns.txt
# Le gate prend en compte sous 5s

# Override total blocked_patterns (skip defaults Rust)
touch /var/lib/kleos/gate/blocked_patterns.txt
# -> 0 pattern bloque cote config (check_dangerous_patterns hardcoded reste actif)
```

### Conditions de retrait

Ce patch pourrait etre absorbe upstream si Ghost-Frame introduit une notion
de "patterns a 3 niveaux" (allow / approval / deny) avec format fichier
hot-tunable. Le code etant isole (1 module + 3 champs config + 1 bloc
inseree + 1 nouveau status DB TEXT-libre), un rebase contre une convention
upstream proche serait simple.

### Extension glob '*' (Patch 19b suite)

Le matcher historique `check_blocked_patterns` etait `contains`
case-insensitive seul. L'extension ajoute le support du wildcard `*` dans
la meme fn (chirurgical ~25 lignes ajoutees au validator.rs) :

- Pattern sans `*` -> backward-compat (contains case-insensitive), aucun
  changement comportement observable sur configs existantes.
- Pattern avec `*` -> split en segments, chaque segment non-vide doit
  apparaitre dans `command` en ordre. `*` seul match toute commande non-vide.
- Exemples : `systemctl *`, `apt *install*`, `* | bash`.

Affecte les deux familles (`blocked_patterns` + `require_approval_patterns`)
puisqu'elles partagent le meme matcher. 5 tests inline `patch19b_glob_*`
couvrent backward-compat, glob simple, glob multiples ordonnes, glob seul,
bords (consecutive et boundary stars).

### Patch 19c -- timeout env var + blocked_patterns externalises + extra_dangerous Windows

Trois ameliorations interdependantes posees sur la cascade gate apres
le fix contract long-poll de Patch 19b.

#### 1. APPROVAL_TIMEOUT_SECS configurable via env var

Pattern Patch 16b. La const `APPROVAL_TIMEOUT_SECS = 120` est preservee
en tant que default pour ne pas casser les callers externes. Une nouvelle
fn `approval_timeout_secs()` lit l'env var
`KLEOS_EIDOLON_GATE_APPROVAL_TIMEOUT_SECS` (apres migration KLEOS_*
-> ENGRAM_* au boot par `config::migrate_env_prefix`) avec fallback sur
la const + `warn!` log si la valeur n'est pas parseable. Le call site
`routes/gate/mod.rs:check_handler` utilise la fn au lieu de la const.

Motivation : 120s est court pour l'usage reel (operateur qui ne regarde
pas l'ecran en continu). L'env var permet de tuner sans rebuild. Cote
LXC 121, set dans `/etc/kleos/kleos.env` :
```env
KLEOS_EIDOLON_GATE_APPROVAL_TIMEOUT_SECS=600
```
Cote hook Windows, garder `KLEOS_SH_APPROVAL_TIMEOUT_SECS` >= timeout
serveur + marge (e.g. 660 ou plus).

#### 2. Externalisation des 10 defauts BLOCKED_PATTERN dans submodule

Les 10 defauts historiques de `GateConfig::default()` (destruction
filesystem root, mkfs, dd if=, fork bomb, reboot/shutdown/halt,
`/dev/sda`, `chmod -R 777 /`) sont copies dans
`gate-rules/blocked_patterns.txt` du submodule. Avec ce fichier present
(meme vide), la cascade s'arrete au niveau fichier -- l'operateur peut
commenter ou retirer chaque pattern.

Headers explicites dans le fichier rappellent que `check_dangerous_patterns`
hardcoded reste actif au-dessus (garde-fou ultime).

#### 3. Nouvelle cascade `extra_dangerous_patterns` operateur-extensible

Motivation : `check_dangerous_patterns` (kleos-lib/src/gate/validator.rs)
couvre les paths Linux (`rm -rf /home`, `/var`, `/etc`, `/usr`, `/opt`,
`/boot`, mkfs, dd if=, etc.) mais rien de Windows-friendly. Sous Windows,
`Remove-Item -Recurse C:\Windows\System32`, `format C:`,
`bcdedit /delete`, `Clear-Disk`, etc. passaient sans gate hardcoded.

Solution additive sans toucher au hardcoded :

- `GateConfig` +2 champs `extra_dangerous_patterns: Vec<String>` +
  `extra_dangerous_patterns_file: Option<PathBuf>`, defaults `Vec::new()`
  et None (opt-in).
- Env loaders `KLEOS_EIDOLON_GATE_EXTRA_DANGEROUS_PATTERNS{,_FILE}`,
  auto-resolus sous `${KLEOS_DATA_DIR}/gate/extra_dangerous_patterns.txt`.
- Signature `check_command_with_context` etendue (8 -> 9 params)
  avec `extra_dangerous_patterns: &[String]`. Bloc insere via `.or_else`
  entre `check_dangerous_patterns` (hardcoded) et `check_blocked_patterns`
  (config). Reason etiquettee `"Command matched extra dangerous pattern:"`
  pour tracabilite.
- Hard deny : `allowed=false`, `requires_approval=false`, status DB
  `blocked`. Pas de hand-off TUI -- pour du soft gate operateur, voir
  `require_approval_patterns.txt`.
- Caller `routes/gate/mod.rs:check_handler` charge la 3e cascade via
  `approval_patterns::load(...)`, resolve via credd, puis passe le slice
  au pipeline.

Le fichier initial `gate-rules/extra_dangerous_patterns.txt` (~40 patterns
Windows conservatifs) couvre :
- System directories (`*system32*`, `*syswow64*`, Remove-Item/rd/del
  sur `C:\Windows`)
- Volume / disk (`format c:`, `format-volume`, `clear-disk`,
  `diskpart*clean`, `remove-partition`)
- Boot (`bcdedit /delete`, `bcdedit /set*safeboot`, `reagentc /disable`)
- Registry (`reg delete hklm`, `remove-itemproperty*hklm:`)
- Services + scheduled tasks (`remove-service`, `sc.exe delete`,
  `schtasks /delete`)
- Power (`stop-computer`, `shutdown /s`, `shutdown /r /t 0`)
- WSL / Hyper-V (`wsl --unregister`, `wsl --shutdown`, `hyper-v*remove-vm`)
- Domain (`gpupdate /force /boot /sync`, `dsadd`, `dsrm`)

#### Fichiers touches (Patch 19c)

| Path | Action | Niveau |
|---|---|---|
| `kleos-lib/src/gate/mod.rs` | fn approval_timeout_secs() + signature 9 params + bloc extra dans `.or_else` + 5 tests | chirurgical |
| `kleos-lib/src/config.rs` | +2 champs `GateConfig`, +2 env loaders, defaults preserves | additif |
| `kleos-server/src/routes/gate/mod.rs` | caller charge 3e cascade + credd resolve + appel `approval_timeout_secs()` | chirurgical |
| `gate-rules/blocked_patterns.txt` | NEW (submodule 0201a7a) | doc/config |
| `gate-rules/extra_dangerous_patterns.txt` | NEW (submodule 0201a7a) | doc/config |
| `docs/dev-notes/local-patches.md` | cette section | doc |

#### Tests

- Unit : `cargo test -p kleos-lib --features bundled-sqlite gate::tests::patch19c_`
  -- 5 cas (timeout default, env override, unparseable fallback, extra
  pattern match avec reason specifique, extra no-match passe a blocked,
  hardcoded short-circuit sur l'extra).
- Total 51 tests `gate::` OK Windows.

#### Spec agent-forge

`spec_c33dc173` completed, 3/3 verify steps OK
(`cargo check kleos-server`, `cargo test gate::tests::patch19c_`,
`cargo test gate::`).

#### Conditions de retrait

Le point 1 (timeout env var) est candidat PR upstream legitime --
pattern generique et utile pour tout deploiement. Les points 2 et 3
restent operateur-specifiques (defaut empty / patterns Windows) et
peuvent etre absorbe upstream uniquement si Ghost-Frame adopte la notion
de fichier de cascade par categorie.

### Submodule gate-rules + workflow operateur

Pour faciliter l'edition des fichiers cascade niveau 1 sans SSH directement
dans le LXC, le repo `VOCSAP/Kleos.gates-rules` est monte en submodule
`gate-rules/` dans le repo Kleos (pattern Patch 15 pour les prompts).

Cote LXC 121, le contenu du repo est clone *directement* dans le path
attendu par `gate_data_file()` :

```bash
# Initial deploy (une seule fois)
ssh root@192.168.10.21 "git clone https://github.com/VOCSAP/Kleos.gates-rules.git /var/lib/kleos/gate"

# Workflow d'edition
cd gate-rules
$EDITOR require_approval_patterns.txt
git add require_approval_patterns.txt
git commit -m "tune(gate): <pattern change>"
git push origin main

# Propagation LXC 121
ssh root@192.168.10.21 "git -C /var/lib/kleos/gate pull"
# Le gate prend en compte sous 5s sans restart kleos-server (cache TTL).

# Bump pointer cote main repo (optionnel, pour tracer la version active)
cd <kleos-root>
git add gate-rules
git commit -m "chore: bump gate-rules to <hash>"
```

Le clone et le submodule pointent vers le meme repo `main` -- aucune
divergence possible tant qu'on commit toujours via le submodule local.

---

## Patch 20 -- supervisor schema repair + append-only manifests + long-poll proper + rate-limit per-key + clients Retry-After (2026-05-22)

### Symptome

`engram-approval-tui` sur LXC 121 affiche en continu `ERROR: HTTP 429 Too Many
Requests` alors que la liste `Pending Approvals` est vide et qu'aucune
operation gate n'est en cours. Les logs `kleos-server` montrent :

```
ERROR ... path=/supervisor/pending ... kleos_server::error:
Database error: no such column: claimed_at
```

repete a chaque poll long-poll (~60s) du hook claude-code qui interroge
`/supervisor/pending`.

### Cause racine -- violation de la regle append-only

`TENANT_MIGRATIONS` (`kleos-lib/src/db/tenant_migrations.rs:27-280`) est
declaree append-only ("Never renumber, never edit a past entry") mais la
position v48 a ete reaffectee au merge VOCSAP <- upstream Ghost-Frame :

1. Avant le 2026-05-07, le binaire en prod (fork VOCSAP) avait
   `TENANT_MIGRATIONS[v48] = memories_community_id`.
2. Le 2026-05-07 07:44:40 UTC, les tenants `2` et `handoffs` appliquent
   correctement cette v48 (log explicite :
   `applying tenant migration 48 (memories_community_id)`), `INSERT INTO
   schema_migrations VALUES (48)` est committed.
3. Au merge upstream du commit `d71342e` (2026-05-05) puis `a0880ee`
   (2026-05-13), la position v48 a ete reaffectee a
   `supervisor_injections_fix_schema` et l'ancienne `memories_community_id`
   deplacee en v51.
4. Sur tous les tenants existants depuis 2026-05-07, le runner
   `run_tenant_migrations` voit `current_version >= 48` -> skip la nouvelle
   v48 -> les ALTERs sur `supervisor_injections` (ajout `rule_id`,
   `claimed_at`, reconstruction de l'index partiel) ne s'executent jamais.
   La table reste en etat v46.
5. L'endpoint `/supervisor/pending` (`kleos-server/src/routes/supervisor/
   mod.rs:85-92`) execute `UPDATE supervisor_injections SET claimed_at =
   ... WHERE claimed_at IS NULL RETURNING ...` -> SQL fail
   `no such column: claimed_at` -> 500 a chaque poll.
6. Le polling agressif (et le bug TUI sur Retry-After, cf. plus bas)
   saturent le bucket `rate_limits` keye sur `user:2`, declenchant le 429
   visible cote operateur. Le 429 finit par cascader sur toutes les
   requetes du meme user_id (kleos-cli, hooks, TUI). Symptome final :
   apparente saturation tous endpoints alors que la cause profonde est
   localisee a `/supervisor/pending`.

Bug secondaire decouvert pendant le diagnostic : `engram-approval-tui`
(`kleos-approval-tui/src/main.rs:94-130`) ignore le header `Retry-After`
sur les reponses 429. Il retape immediatement, ce qui maintient le bucket
de rate-limit du serveur plein indefiniment. Verifie via `SELECT * FROM
rate_limits WHERE key = 'user:2'` -> count=18, fenetre en glissement
continu.

### Reparation manuelle prod 2026-05-22

DB tenants patchees a la main via sqlcipher (serveur stoppe) :

```sql
PRAGMA key = "x'<KLEOS_DB_KEY>'";
ALTER TABLE supervisor_injections ADD COLUMN rule_id TEXT NOT NULL DEFAULT '';
ALTER TABLE supervisor_injections ADD COLUMN claimed_at TEXT;
DROP INDEX IF EXISTS idx_supervisor_injections_pending;
CREATE INDEX idx_supervisor_injections_pending
    ON supervisor_injections(user_id, session_id)
    WHERE claimed_at IS NULL;
```

Applique sur `/var/lib/kleos/tenants/2/kleos.db` et
`/var/lib/kleos/tenants/handoffs/kleos.db` (seuls tenants actifs constates
via `find /var/lib/kleos -maxdepth 3 -name 'kleos.db'`).

Pour debloquer la session de diagnostic (TUI client buggy maintenait le
429 in-flight), purge ponctuelle du bucket :

```sql
DELETE FROM rate_limits WHERE key IN ('user:2','ip:192.168.10.100');
```

Test post-fix : `GET /supervisor/pending?session_id=...` retourne
`{"claimed":0,"injections":[],"session_id":"..."}` HTTP 200.

### Implementation Patch 20 (deployee)

Le patch part 1 (commit `9a7a0f1`) a livre :

- Migration tenant v55 `supervisor_injections_repair` idempotente via
  `table_has_column` guards (re-applique `ALTER TABLE ADD COLUMN rule_id`,
  `ADD COLUMN claimed_at`, et reconstruit le partial index avec
  `WHERE claimed_at IS NULL`). NO-OP sur tenants deja au schema
  post-Patch 20.
- Fichiers manifests append-only :
  - `kleos-lib/src/db/tenant_migrations.manifest` (55 entrees, derniere
    `55: supervisor_injections_repair`).
  - `kleos-lib/src/db/migrations.manifest` (63 entrees, derniere
    `63: handoff_atoms`).
- Tests CI append-only :
  `tenant_migrations::tests::tenant_migrations_obey_append_only_manifest`
  et `migrations::tests::migrations_obey_append_only_manifest` qui
  comparent byte-par-byte chaque entree (version, description) avec son
  manifest et explosent au CI sur toute mutation d'une entree publiee.
  Verts : 96 tenant_migrations + 9 migrations.

Le patch part 2 livre :

- **Long-poll proper sur `/supervisor/pending`** : nouveau champ
  `AppState.supervisor_notifiers: Arc<RwLock<HashMap<i64, watch::Sender<()>>>>`
  lazy par tenant. `pending_handler` boucle (UPDATE-RETURNING immediat ->
  si vide, `tokio::time::timeout(deadline, rx.changed())` sur le
  `watch::Receiver` du user -> retry). `inject_handler` signale le user
  apres l'INSERT. Helper `supervisor_longpoll_timeout()` lit
  `KLEOS_SUPERVISOR_LONGPOLL_TIMEOUT_SECS` (defaut 30s). Scope
  per-tenant : signal pour user A ne reveille pas les long-polls de
  user B. Memoire ~64 octets par tenant actif, pas de cleanup necessaire
  au depart.
- **Rate-limit middleware key par api_key** (Design C, fix semantique
  de `ApiKey.rate_limit` existant) :
  `kleos-server/src/middleware/rate_limit.rs:163` passe de
  `format!("user:{}", user_id)` a `format!("key:{}", ctx.key.id)`.
  Aucune migration de donnees (champ rate_limit deja per-key dans
  `api_keys`).
- **Client TUI** (`kleos-approval-tui/src/main.rs`) :
  - Helpers `http_longpoll_timeout()` (lit
    `KLEOS_HTTP_LONGPOLL_TIMEOUT_SECS`, defaut 60s) et
    `parse_retry_after()` (lit le header, fallback 60s).
  - `App` recoit un champ `last_429_until: Option<Instant>`.
  - `fetch_pending()` et `decide()` : si `last_429_until` est actif,
    return immediat avec countdown affiche, sinon hit serveur ;
    sur 429 received, calcul `last_429_until = now + parse_retry_after`,
    affiche backoff. L'UI reste responsive (le sleep ne bloque pas la
    boucle ratatui).
  - `reqwest::Client` instancie avec `.timeout(http_longpoll_timeout())`
    pour exploiter le long-poll serveur.
- **Client `kleos-cli`** (`kleos-cli/src/hook.rs`) :
  - Helpers `http_longpoll_timeout()`, `supervisor_in_backoff()`,
    `supervisor_record_backoff()` (cache file
    `$XDG_CACHE_HOME/kleos/supervisor-backoff-until.unix` ou fallback
    `$HOME/.cache`).
  - `handle_user_prompt` : skip le drain si backoff actif ; sur 429
    detecte via le message d'erreur du client crate
    (`get_with_timeout` expose `String`), `supervisor_record_backoff(60)`
    et fail-open.
- **Hooks bash** (`hooks/full/lib-eidolon.sh`) :
  - `_EIDOLON_DEFAULT_TIMEOUT` lit `KLEOS_HTTP_LONGPOLL_TIMEOUT_SECS`
    (defaut 60s).
  - Cache file `$XDG_CACHE_HOME/kleos/eidolon-backoff-until.unix`.
  - Helpers `_eidolon_in_backoff()` et `_eidolon_record_backoff()`.
  - `eidolon_call()` : skip si backoff, sinon capture le code HTTP
    via `-w "%{http_code}"` et le header via `-D <tempfile>`. Sur 429,
    parse `Retry-After:` et record backoff. Preserve la semantique
    `-sf` historique (return 1 + body vide sur non-2xx) pour fail-open
    des hooks consommateurs.

### Convention defaults env vars

- `KLEOS_SUPERVISOR_LONGPOLL_TIMEOUT_SECS` (server) -- defaut 30s.
- `KLEOS_HTTP_LONGPOLL_TIMEOUT_SECS` (clients) -- defaut 60s.

Doc utilisateur : voir le `CLAUDE.md` projet section "Convention env vars"
et `wiki/Configuration.md` (sous-section "Long-poll timeouts (Patch 20)").

Operateur 2026-05-22 a configure server=30 (LXC 121
`/etc/kleos/kleos.env`) et client=30 (Windows env). Setup serre mais
fonctionnel ; en cas de timeout reseau cote client, monter
`KLEOS_HTTP_LONGPOLL_TIMEOUT_SECS` a 60 ou plus sur le poste Windows.

### Patch 20b (2026-05-22) -- bug `key:0` et architecture TUI non-bloquante

Deux points decouverts immediatement apres le deploy Patch 20 :

1. **Bug Design C `key:0`** : les helpers `open_access_context()` et
   `synthetic_key_for_identity_with_scopes()` (`kleos-server/src/middleware/auth.rs:80,109`)
   stampent tous deux `ApiKey.id = 0` (pas de row backing). Le keying
   `format!("key:{}", id)` collapsait alors **toutes** les requetes
   synthetiques (open access + chaque PIV identity) dans le meme bucket
   `key:0`, pire que l'ancien `user:{user_id}` qui isolait au moins par
   tenant.

   Fix : dans `rate_limit_middleware` (`middleware/rate_limit.rs:172`), si
   `ctx.key.id == 0`, fallback sur `format!("synth:user:{}", ctx.user_id)`.
   La branche `key:{id}` regulier (cles d'`api_keys`) est inchangee.

2. **TUI bloque pendant le long-poll** : la boucle `fetch_pending().await`
   bloquait pendant 30s (server long-poll timeout), rendant la TUI
   non-reactive aux touches. Pour un workflow d'approbation a 120s, ca
   ne va pas.

   Refactor `kleos-approval-tui/src/main.rs` : `fetch_pending` et
   `decide` deviennent des `tokio::task::JoinHandle<FetchOutcome>` /
   `JoinHandle<DecideOutcome>` lances en arriere-plan. Champs `App.fetch_handle`
   et `App.decide_handle`. Methode `App::poll_handles()` (non-bloquante,
   `JoinHandle::is_finished` + `await` immediat si fini) appelee a chaque
   tick. Methode `App::try_start_fetch()` (non-bloquante) re-arme un
   fetch des qu'aucun n'est en vol et que la fenetre 429 est expiree.
   La boucle principale `event::poll(tick)` passe d'un tick 1s a 100ms
   pour rester reactive (l'argument CLI `--poll-ms` reste mais la
   semantique change : 100ms = latence max sur les keypresses, pas
   periode de polling reseau).

Tests : `cargo check -p kleos-server` et `-p kleos-approval-tui`
restent verts. agent-forge `hyp_d48abd7a` cover.

Reste a deployer : rebuild WSL kleos-server, scp + chmod 755 + restart
LXC 121 ; rebuild Windows MSVC kleos-approval-tui, copy vers
`%USERPROFILE%\.cargo\bin\engram-approval-tui.exe`.

### Patch 20c (2026-05-22) -- long-poll proper sur `/approvals/pending`

Symptome post-deploy Patch 20b : la TUI relancee voit immediatement
"Rate-limited by server" malgre le refactor non-bloquant et le cooldown
500ms. Verification table `rate_limits` : `ip:192.168.10.100 count=28`
en moins d'une minute, depasse la limite preauth IP de 20/min.

Cause racine : **Patch 20 a long-polle `/supervisor/pending` mais pas
`/approvals/pending`**. Le handler `list_pending_handler` de
`kleos-server/src/routes/approvals/mod.rs` etait reste tres simple :
expire_stale + list_pending + return. Avec une queue vide il retournait
en < 100ms. La TUI, qui poll cet endpoint (pas /supervisor/pending),
enchainait alors les fetches au rythme du cooldown 500ms, soit
~120 hits/min, blowing past le pre-auth IP 20/min.

Fix : appliquer le meme pattern long-poll que `/supervisor/pending`,
en reutilisant **le watch::Sender existant `state.approval_notify`**
(deja signale par `create_handler` et `decide_handler`). Pas besoin
de creer un nouveau notifier per-tenant : le notify est global mais le
handler re-query la DB filtree par `auth.user_id` a chaque wake-up,
donc une notif cross-tenant ne fait qu'un round-trip vide -- moins
couteux qu'un busy-poll cote client.

Nouvelle env var `KLEOS_APPROVALS_LONGPOLL_TIMEOUT_SECS` (defaut 30),
miroir de `KLEOS_SUPERVISOR_LONGPOLL_TIMEOUT_SECS`. Le meme
`KLEOS_HTTP_LONGPOLL_TIMEOUT_SECS` cote client couvre les deux
endpoints avec la marge `client > server`.

Aucun changement client. La TUI Patch 20b fonctionne tel quel avec
Patch 20c : la connexion HTTP reste ouverte jusqu'a 30s, fetch
suivant cooldown 500ms + retour long-poll, donc ~2 fetches/min en
steady state au lieu de ~120. agent-forge hyp_4b2e8103.

### Lecons

- **Verifier `TENANT_MIGRATIONS` au merge upstream**. Tout commit upstream
  qui insere ou renomme une entree dans `TENANT_MIGRATIONS` ou `MIGRATIONS`
  doit etre verifie avant absorption. Si position deja appliquee en prod,
  ajouter en queue (nouvelle version >= max+1) avec un nom different.
- **Le statut `schema_migrations` n'est pas une preuve d'application**.
  La ligne `(version, applied_at)` est INSERT separe (auto-commit SQLite).
  Si la migration body est devenue idempotent / no-op apres coup, la
  ligne reste mais le schema est en derive.
- **Le 429 cote operateur est souvent la consequence, pas la cause**.
  Diagnostic systematique : verifier `/var/log/kleos-server.log` pour les
  ERROR repetitives sur un meme endpoint avant de blamer le rate-limit.

---

## Patch 21 -- bridge bidirectionnel `gate_requests` <-> `approvals` (2026-05-22)

### Symptome

Le pipeline `require_approval_patterns` du Patch 19b posait `gate_requests.status='pending_approval'` + insert dans le HashMap in-memory `state.pending_approvals` + notify `approval_notify`, mais **n'ecrivait rien dans la table `approvals`**. Or la TUI `engram-approval-tui` consomme `GET /approvals/pending` qui fait `SELECT FROM approvals WHERE status='pending'`. Resultat : tout match de pattern produisait un `gate_requests` invisible cote operateur, le caller `/gate/check` restait bloque 600s puis recevait `denied -- approval timed out or rejected`. Aucun consommateur alternatif des `gate_requests pending_approval` n'existait dans le repo (verifie via inspection live : TUI poll uniquement `/approvals/pending`, `eidolon-supervisor` push-only, GUI Svelte n'expose pas le flow).

### Approche

Niveau **chirurgical + additif** (cf. table de delta `rust-upstream-fork.md`) : ajouter une colonne nullable `gate_id INTEGER` a `approvals` via une nouvelle migration v56/v64 idempotente (guard `table_has_column`), faire ecrire le bloc 2bis du Patch 19b dans `approvals` via un helper `create_approval_with_gate`, et faire relayer le `decide_handler` cote `/approvals/{id}/decide` vers `state.pending_approvals.tx.send(approved)` + `respond_to_gate` quand l'approval a un `gate_id` non null.

Le helper `read_gate_id_for_approval` est tolerant aux deploiements pre-migration (retourne `Ok(None)` si la colonne n'existe pas), ce qui evite une fenêtre de panne entre boot et premiere migration.

Le filtre `respond_to_gate` et `mark_gate_timed_out` est elargi de `status='pending'` a `status IN ('pending', 'pending_approval')` pour permettre la transition terminale des gates Patch 19b (sans ca, les rows restaient en `pending_approval` apres timeout).

### Fichiers touches (delta)

- `kleos-lib/src/db/tenant_migrations.rs` -- entry `TenantMigration v56` + fn `apply_schema_v56_approvals_gate_id` avec guard `table_has_column`.
- `kleos-lib/src/db/tenant_migrations.manifest` -- append `56: approvals_gate_id`.
- `kleos-lib/src/db/migrations.rs` -- entry `Migration v64` + fn `run_migration_approvals_gate_id` + constant `MIGRATION_APPROVALS_GATE_ID` + bloc dispatch dans `apply_pending_migrations`.
- `kleos-lib/src/db/migrations.manifest` -- append `64: approvals_gate_id`.
- `kleos-lib/src/approvals/types.rs` -- champ `pub gate_id: Option<i64>` sur `Approval` (skip_serializing_if=None pour preserver le wire format).
- `kleos-lib/src/approvals/mod.rs` -- nouvelle fn `create_approval_with_gate`, refactor de `create_approval` vers helper interne `create_approval_inner`, INSERT etendu avec fallback "no such column" (defense en profondeur), fn `read_gate_id_for_approval`, mise a jour `row_to_approval` pour initialiser `gate_id: None`.
- `kleos-lib/src/gate/mod.rs` -- elargir filtre `status` dans `respond_to_gate` et `mark_gate_timed_out` a `IN ('pending', 'pending_approval')`.
- `kleos-server/src/routes/gate/mod.rs` -- bloc 2bis appelle `create_approval_with_gate` apres notify, log warn (non bloquant) si echec.
- `kleos-server/src/routes/approvals/mod.rs` -- import `read_gate_id_for_approval`, `respond_to_gate`, `ApprovalDecision`. `decide_handler` lit gate_id avant decide, puis si non null : appelle `respond_to_gate` (DB) + `state.pending_approvals.tx.send(approved)` (wake-up).

### Tests

- `cargo test -p kleos-lib --features bundled-sqlite --lib`: 110 passed, 1 ignored (test_tenant_isolation existant), 0 failed. Inclut `tenant_migrations_obey_append_only_manifest`, `migrations_obey_append_only_manifest`, `every_static_migration_is_dispatched`.
- `cargo check -p kleos-server`: 0 erreurs, 10 warnings preexistants.
- Build release WSL + deploy LXC 121 + test E2E : a faire par l'operateur (build serveur jamais lance depuis Claude, cf. memoire Kleos #3023).

### Test E2E attendu post-deploy

```bash
# Terminal A : TUI sur poste operateur
engram-approval-tui

# Terminal B : caller bloquant qui matche un pattern
curl -sf -m 660 -H "Authorization: Bearer $KEY" http://192.168.10.21:4200/gate/check \
  -d '{"command":"systemctl status fake","agent":"test","skip_approval":false,"tool_name":"Bash"}'

# Attendu : entree apparait dans la TUI sous <500ms (long-poll Patch 20c reveille).
# Appuyer 'a' : caller debloque sous ~200ms avec {"allowed":true, "gate_id":N, ...}
```

### Leviers de desactivation (hot, sans rebuild)

1. **Vider le fichier de patterns** : `truncate -s 0 /var/lib/kleos/gate/require_approval_patterns.txt`. Cache TTL 5s. Plus aucun match -> bloc 2bis jamais traverse.
2. **Raccourcir le timeout** : `KLEOS_APPROVAL_TIMEOUT_SECS=5` dans `/etc/kleos/kleos.env`, restart kleos-server. Caller debloque en 5s (denied).
3. **Ne pas demarrer la TUI** : audit trail en DB, timeout naturel. Comportement identique a aujourd'hui.

### Patch 21.1 (2026-05-22) -- restriction du bridge aux pattern matches

**Symptome post-deploy Patch 21** : chaque commande Bash via le hook `kleos-sh` PreToolUse de Claude Code declenche le bloc 2bis (tool_name=Bash est dans `TOOLS_REQUIRING_APPROVAL = [Bash, Write, Edit, WebFetch, WebSearch]`), donc Patch 21 INSERT dans `approvals` pour toutes mes Bash. La TUI affichait `Pending: 1` en boucle pour des commandes innocentes (ex: `ssh root@... grep ...`). Le `respond_to_gate` declenche par le decide TUI echouait Conflict 409 ("gate request 545 is already allowed") parce que le path normal de `check_command_with_context:281` pose status=`allowed`, pas `pending_approval`.

**Approche** : un seul `if pattern_triggered_approval { ... }` wrapper autour de l'appel `create_approval_with_gate`. Niveau additif pur (~5 lignes), conservateur du comportement upstream pour `TOOLS_REQUIRING_APPROVAL` (HashMap + notify + wait timeout 600s silencieux, jamais visible TUI -- exactement le comportement pre-Patch 21). Seules les commandes matchant un `require_approval_patterns` explicite cree une row dans `approvals` -> TUI visible -> decide debloque correctement.

**Fichiers touches** : `kleos-server/src/routes/gate/mod.rs` (bloc 2bis lignes ~261-310 sous Patch 21).

agent-forge hypothesis: `hyp_aa454644`.

**Lecon** : Patch 21 ne contenait pas de regression de son scope, mais exposait un comportement upstream pre-existant. Cross-call-site verification (cf. `.claude/rules/kleos-patching-discipline.md`) doit aussi inclure les conditions de declenchement amont, pas juste les call sites du symbole touche. Le bloc 2bis avait deux entrees (TOOLS_REQUIRING_APPROVAL OR pattern_triggered_approval), Patch 21 a uniforme le traitement sans distinguer -- Patch 21.1 corrige.

### Conditions de retrait

- **PR upstream candidate** : le decouplage des bus est anterieur a Patch 19b. Toute version upstream qui adopte le bridge similaire ou refactore le pipeline gate vers une file unique permet de drop Patch 21.
- **Alternative possible** : refactor TUI sur `state.pending_approvals` via nouvelle route `/supervisor/inflight` (Option 3 du TODO), abandonnerait Patch 21. Pas envisage actuellement (perte audit trail au restart).

---

## Patch 23 -- cooldown adaptatif TUI sur le re-fetch /approvals/pending (2026-05-22)

### Symptome

Live test E2E Patch 21.1 sur LXC 121 tenant 2 : un seul approval pending matchant `systemctl *` produit un 429 HTTP cote TUI en quelques secondes. Wireshark capture **17 GET /approvals/pending** depuis le poste operateur 192.168.10.100 dans une fenetre de test (60s), pour un seul approval en attente. Le bucket `preauth_rate_limit_middleware` IP (hardcoded 20/min, cf. `kleos-server/src/middleware/rate_limit.rs:15`) sature -> 429 -> impossible d'approuver via TUI -> deadlock UX.

### Cause racine

Le serveur Patch 20c retourne **immediatement** quand `list_pending` n'est pas vide (correct par design : si une approval existe, le client doit la voir maintenant, pas attendre 30s). Cote TUI Patch 20b, le `MIN_REFETCH_INTERVAL = 500ms` etait calibre pour le scenario cascade (erreur reseau qui retourne instantanement, cf. rule `~/.claude/rules/rust-async-tokio.md` section "Cooldown anti-cascade"), pas pour le scenario steady-state ou une approval reste affichee 10-30s en attente de decision operateur. Resultat : 500ms cooldown x ~120 = 120 calls/min en pire cas (en pratique ~60 a cause de la latence reseau).

C'est une dette UX exposee par Patch 21.1 (le bridge a rendu visible le polling agressif qui existait deja en local-state).

### Approche

Cooldown adaptatif selon l'etat de la queue cote client :

- **Queue vide** (`self.approvals.is_empty()`) -> cooldown 500ms (`KLEOS_APPROVAL_TUI_REFETCH_BUSY_MS`, default `DEFAULT_REFETCH_BUSY_MS = 500`). Inchange vs Patch 20b. Le long-poll cote serveur tient 30s en steady state, le 500ms protege uniquement contre les erreurs instantanees (DNS fail, connection refused).
- **Queue non-vide** (`count > 0`) -> cooldown 5s (`KLEOS_APPROVAL_TUI_REFETCH_PENDING_MS`, default `DEFAULT_REFETCH_PENDING_MS = 5000`). Calibre sur le temps de reflexion typique operateur 10-30s. Reduit la charge d'un facteur 10 (~4-6 calls pour un decide cycle 30s au lieu de ~60).
- **Sur action operateur reussie** (`DecideOutcome::Ok`) : `last_fetch_completed_at = None` + `fetch_handle.abort()` pour bypass le cooldown 5s et avoir un refetch immediat post-decide -> UI reactive.

Niveau **chirurgical** (~30 lignes ajoutees), additif sur kleos-approval-tui uniquement, candidat PR upstream.

### Fichiers touches

- `kleos-approval-tui/src/main.rs` : remplace la constante `MIN_REFETCH_INTERVAL` par deux constantes `DEFAULT_REFETCH_BUSY_MS` + `DEFAULT_REFETCH_PENDING_MS`, ajoute helpers `refetch_busy_interval()` / `refetch_pending_interval()` qui lisent les env vars, branche le cooldown adaptatif dans `try_start_fetch`, bypass dans `apply_decide_outcome` (`DecideOutcome::Ok`).

### Tests

- `cargo check -p kleos-approval-tui` : 0 erreurs, 10 warnings preexistants.
- Build operateur : `cargo build --release -p kleos-approval-tui` (Windows MSVC, pas WSL Linux) puis copie du binaire `engram-approval-tui.exe` dans `%USERPROFILE%\.cargo\bin\`.

### Conditions de retrait

- **Patch 24 SSE** (planifie) : remplacement complet du polling par Server-Sent Events. Zero polling -> Patch 23 deviendrait obsolete. Voir `docs/dev-notes/sse-vs-longpoll-decision-todo.md` pour le design et la reflexion architecturale qui motive cette refonte.
- Si upstream Ghost-Frame adopte SSE ou un autre mecanisme push, drop Patch 23 + Patch 24.

### Leviers de tuning sans rebuild

Sur le poste operateur, ajuster en live via env vars avant de relancer la TUI :

```
KLEOS_APPROVAL_TUI_REFETCH_BUSY_MS=500     # default, queue vide
KLEOS_APPROVAL_TUI_REFETCH_PENDING_MS=5000 # default, queue non vide
```

Pour un poste partage entre plusieurs sessions ou pour relacher davantage la charge serveur, monter `_PENDING_MS` a 10000 (10s). Le delta UX est negligeable (5s de retard a l'apparition d'une nouvelle approval pendant qu'on en reflechit a une autre).

agent-forge hypothesis : `hyp_567c06dd`.

### Lecons

- **Decouplage de bus producers/consumers** : avant d'ajouter un producer sur un nouveau statut/etat, mapper tous les consumers downstream (TUI, supervisor, GUI, autre client) via grep negatif `INSERT INTO <table-consumer>` depuis les sites producers. Cf. Kleos memoire #3019.
- **`status = 'pending'` filter trop strict** : Patch 19b a introduit `'pending_approval'` sans elargir les filtres existants dans `respond_to_gate` et `mark_gate_timed_out`. Pattern general : quand on ajoute un nouveau status terminal ou intermediaire, grep tous les `WHERE status =` du domaine pour aligner.
- **Helper "no such column" fallback** : pour les fonctions qui consomment une colonne ajoutee par une migration future, retourner `Ok(None)` plutot que crasher. Permet aux services de boot sans attendre le runner de migration.

agent-forge spec_id : `spec_0f75e1e0`.

---

## Patch 25 -- regex matcher + per-cascade whitelist + subcommand splitting shell-aware (2026-05-22)

### Symptome motivateur

Memoire Kleos #3020 : le matcher gate Patch 19b utilisait un `contains` case-insensitive avec wildcard `*` optionnel. Consequence : `kleos-cli store "...mentions git push..."` matchait le pattern `git push` de `require_approval_patterns.txt` parce que la chaine litterale etait dans la cmdline visible par le hook. Resultat operateur : approval pending creee pour un simple store de memoire, friction UI inutile.

### Approche

3 changements ordonnes par dependance logique, niveau **chirurgical + additif pur** :

1. **Matcher regex-based** (`kleos-lib/src/gate/validator.rs`) -- nouvelle struct `CompiledPatternSet { set: RegexSet, sources: Vec<String> }` qui wrap un `regex::RegexSet` avec une heuristique de detection au load :
   - Pattern avec metachar regex (`^`, `$`, `[`, `(`, `\`, `+`, `?`, `{`, `|`, `.`) -> regex pure.
   - Sinon -> glob-lite (literals + `*` wildcard), auto-converti en regex anchored `^pattern$` avec `*` -> `.*`.
   - Prefixe `(?i)` ajoute automatiquement pour preserver le case-insensitive de pre-Patch 25 (sauf si pattern commence par `(?` -- operateur peut override).
   - Helper `check_compiled(command, &CompiledPatternSet) -> Option<String>` retourne la source matchee (pour le reason logging).
   - Regex invalide -> log warn + skip (pas de crash).
   - Empty set -> retourne `None` toujours -> aucune exemption (semantique vide).
2. **Whitelist par cascade** (`kleos-lib/src/gate/mod.rs`) -- nouvelle fn publique `check_command_with_whitelist` qui etend `check_command_with_context` avec 2 params `blocked_whitelist_patterns: &[String]` et `require_approval_whitelist_patterns: &[String]`. Logique : un pattern whitelist matche sur une sous-commande -> cette sous-commande est EXEMPTE de SA cascade. `check_dangerous_patterns` hardcoded **ET** `extra_dangerous_patterns` restent **non-exemptables** (decision operateur 2026-05-22 : extra_dangerous est surchargeable par AJOUT uniquement, pas par EXCLUSION).
3. **Subcommand splitter shell-aware** (`kleos-lib/src/gate/validator.rs::split_into_subcommands`) -- splitte la cmdline en sous-commandes via shell connectors (`&&`, `||`, `;`, `|`, `&` suffix, backticks, `$()`, `<()`, `>()`) en respectant les quotes POSIX (via `shell-words` crate pour validation, walker char-by-char pour la position-aware split). Heredocs (`<<EOF`, `<<-EOF`, `<<'EOF'`, `<<"EOF"`) sont pre-scannees et leur body est exclus du splitter (data, pas commande). Backticks, `$()`, `<()`, `>()` evalues recursivement sur leur contenu interne (capped a depth 5, log warn si depasse). Tokenize error policy lue depuis `KLEOS_EIDOLON_GATE_ON_TOKENIZE_ERROR` (defaut `deny` -> retourne `blocked` avec raison "cmdline malformed").

### Niveau de delta upstream

- **`kleos-lib/src/gate/validator.rs`** : additif pur (300+ lignes en bas du fichier, zero touche aux fonctions existantes `pattern_matches`, `check_blocked_patterns`, `check_dangerous_patterns`).
- **`kleos-lib/src/gate/mod.rs`** : `check_command_with_context` (signature inchangee) delegue maintenant a `check_command_with_context_inner` avec whitelists vides. Nouvelle fn publique `check_command_with_whitelist` ajoutee. Le caller upstream ne casse pas, le nouveau caller (kleos-server) appelle `_with_whitelist`. Backward compat 100% sauf rupture semantique des patterns glob-lite (cf. ci-dessous).
- **`kleos-lib/src/config.rs`** : 4 nouveaux fields sur `GateConfig` (mirror de Patch 19b) + 4 env loaders.
- **`kleos-server/src/routes/gate/mod.rs`** : 2 nouveaux blocs de chargement whitelist + 2 nouveaux blocs de credd-resolve + appel a `check_command_with_whitelist` au lieu de `check_command_with_context`.
- **`gate-rules/`** : 2 nouveaux fichiers `blocked_whitelist.txt` + `require_approval_whitelist.txt` (vides + header explicatif). Pas de `extra_dangerous_whitelist.txt` (cascade non-exemptable).

### Rupture semantique documentee

La conversion glob-lite produit `^pattern$` (anchored) au lieu du `contains` pre-Patch 25. Patterns sans metachar et sans `*` deviennent strict-match. **Migration patterns existants** :

| Avant Patch 25 (contains) | Apres Patch 25 (anchored) | Pour preserver contains |
|---|---|---|
| `apt install` | `^apt install$` | `apt install*` ou `*apt install*` |
| `git push` | `^git push$` | `git push*` ou `*git push*` |
| `systemctl *` | `^systemctl .*$` | inchange (deja `*` present) |

L'operateur doit auditer `/var/lib/kleos/gate/{blocked,require_approval,extra_dangerous}_patterns.txt` apres deploy et migrer les patterns sans `*` qui devraient rester en contains semantic. Logging INFO automatique au load pour chaque pattern glob-lite suggere la migration.

### Fichiers touches

- `Cargo.toml` (workspace root) : `shell-words = "1.1"` ajoute dans `[workspace.dependencies]`.
- `kleos-lib/Cargo.toml` : `shell-words = { workspace = true }` ajoute.
- `kleos-lib/src/gate/validator.rs` : bloc Patch 25 (~300 lignes additives) avec `CompiledPatternSet`, `compile_patterns`, `check_compiled`, `is_obvious_regex`, `glob_lite_to_regex`, `TokenizeErrorPolicy`, `SplitError`, `split_into_subcommands`, `tokenize_error_policy`, helpers heredoc + substitution.
- `kleos-lib/src/gate/mod.rs` : `check_command_with_context` delegue, `check_command_with_whitelist` ajoute, `check_command_with_context_inner` privee avec cascade Patch 25 complete (split + worst-result-wins aggregation).
- `kleos-lib/src/config.rs` : 4 fields ajoutes a `GateConfig` + 4 env loaders.
- `kleos-server/src/routes/gate/mod.rs` : 2 whitelist loaders + 2 credd-resolve blocs + appel `check_command_with_whitelist`.
- `gate-rules/blocked_whitelist.txt` + `gate-rules/require_approval_whitelist.txt` : creation initiale (header + exemples commentes).
- `CLAUDE.md` : doc des nouvelles env vars (Patch 25, NEW).

### Env vars

- `KLEOS_EIDOLON_GATE_BLOCKED_WHITELIST_PATTERNS` (CSV) + `_FILE` (path) : whitelist cascade blocked.
- `KLEOS_EIDOLON_GATE_REQUIRE_APPROVAL_WHITELIST_PATTERNS` (CSV) + `_FILE` (path) : whitelist cascade require_approval.
- `KLEOS_EIDOLON_GATE_ON_TOKENIZE_ERROR` (`allow` | `deny`, default `deny`) : policy quand shell-words tokenize echoue.

### Tests

25 nouveaux tests unitaires + integration ajoutes dans `kleos-lib/src/gate/validator.rs` et `kleos-lib/src/gate/mod.rs` couvrant :

- Detection heuristique format regex/glob-lite + conversion `^pattern$` + `(?i)` case-insensitive.
- Subcommand splitter : connectors `&&`/`||`/`;`/`|`/`&`, quotes simples/doubles, escape, backticks, `$()`, `<()`, `>()`, heredoc body skip, depth cap recursion.
- Tokenize error : policy Allow vs Deny.
- Whitelist : exemption uniquement sur sa cascade, non-overridable `check_dangerous_patterns` et `extra_dangerous_patterns`.
- Cascade integration : `kleos-cli store "git push" && git push` -> requires approval (1 subcommand whitelisted, 1 non).

### Conditions de retrait

- Upstream Ghost-Frame absorbe la generalisation regex matcher + whitelist + splitter (candidat PR upstream legitime).
- Ou Patch 24 SSE (planifie) refond le pipeline approval -> Patch 25 peut etre simplifie (les whitelist + splitter restent utiles, mais le subcommand parsing pourrait migrer cote client TUI).

### Lecons

- **Backward compat partielle suffit si rupture documentee** : la rupture glob-lite anchored vs contains casse 1 test sur 51, l'operateur accepte le trade-off parce qu'il elimine le bug `kleos-cli store "git push"`. Migration via `*pattern*` est mecanique.
- **`(?i)` prefixe par defaut > to_lowercase upstream** : preserve la perf (1 prefixe sur la regex compilee une fois) ET permet l'override `(?-i)` au cas par cas.
- **shell-words pour validation, walker custom pour split** : `shell-words::split` ne preserve pas les positions originales necessaires au split + reconstruction. Pattern : utiliser `shell-words::split` pour DETECTER un tokenize valide, puis walker char-by-char pour le split position-aware.
- **Non-overridable par decision operateur, pas par technique** : `extra_dangerous_patterns` aurait pu accepter une whitelist techniquement, mais l'operateur a explicitement decide que la cascade est additive uniquement. Documenter cette decision dans le patch + dans CLAUDE.md.

agent-forge spec_id : `spec_aa233554`, hypothesis : `hyp_e1d4c948`.

---

## Patch 26 -- WARN tracing sur reject 429 (preauth + per-key) (2026-05-23)

### Symptome

Operateur observe un HTTP 429 cote TUI `engram-approval-tui` apres deploy
Patch 25, sans qu'aucune trace WARN/ERROR n'apparaisse dans
`/var/log/kleos-server.log`. La fonction `too_many_requests()` dans
`kleos-server/src/middleware/rate_limit.rs` retourne le 429 silencieusement :
les deux bras `Ok(false)` (preauth IP a `PREAUTH_IP_LIMIT = 20/min` hardcoded,
et per-key avec keying `key:N` ou `synth:user:N` apres fallback Patch 20b)
emettent directement la reponse sans tracing. Seul le bras `Err(e)` log via
`tracing::error!`. Impossible de discriminer preauth IP saturation vs per-key
saturation sans dechiffrer la table `rate_limits` (qui requiert le
`cipher_compatibility` specifique au bundle WSL et qui mismatch souvent avec
le binaire `sqlcipher` du host LXC).

### Approche

Niveau **chirurgical** (~12 lignes ajoutees, 2 lignes touchees) : ajouter
un `tracing::warn!(bucket, limit, path[, cost])` dans chacun des deux bras
`Ok(false)`. Le champ `bucket = %key` est la donnee discriminante : `ip:X`
revele preauth IP, `key:N` revele per-key vrai bearer, `synth:user:N` revele
AuthContext synthetique (open access ou PIV).

Pas d'env var, pas de changement comportemental, pas de nouveau bind. Le
verbose-level de tracing reste pilote par `RUST_LOG` upstream (les WARN
sont emis par defaut). Choix justifie : on accepte le bruit potentiel sous
attaque DDoS volontaire (le serveur loguera chaque IP rejetee), qui reste
borne par `PREAUTH_IP_LIMIT * 60 * connections_uniques` lignes/heure --
acceptable vu le caractere transient des attaques pre-auth.

### Fichiers touches

- `kleos-server/src/middleware/rate_limit.rs` :
  - `preauth_rate_limit_middleware` bras `Ok(false)` (l.110-122) : ajout
    bloc `tracing::warn!` avec `bucket`, `limit = PREAUTH_IP_LIMIT`,
    `path`.
  - `rate_limit_middleware` bras `Ok(false)` (l.192-214) : ajout bloc
    `tracing::warn!` avec `bucket`, `limit`, `cost`, `path`.

### Validation

Post-deploy : declencher un 429 connu (boucle `kleos-cli search` rapide ou
TUI en pending soutenu) et grep le log :

```bash
ssh root@192.168.10.21 "tail -F /var/log/kleos-server.log | grep -E 'reject \(429\)'"
```

Verification attendue : ligne WARN avec `bucket=ip:192.168.10.X` (preauth
IP saturated), OU `bucket=key:2` (per-key bearer principal saturated), OU
`bucket=synth:user:N` (PIV/open-access synthetic). Permet de cibler le bon
levier de fix (env var preauth, dedicated bearer, ou isolation synthetic).

### Niveau delta

**Chirurgical** (additif pur dans deux bras existants, zero refactor, zero
changement de signature). Le hardcode `PREAUTH_IP_LIMIT` n'est pas touche
-- une eventuelle config env var `KLEOS_PREAUTH_IP_LIMIT` serait un Patch
distinct.

### Conditions de retrait

Upstream Ghost-Frame absorbe l'observabilite (PR candidat naturel,
generaliste, sans dette de retro-compat). Sinon le patch reste leger
indefiniment.

agent-forge hypothesis : `hyp_9e17cf63`.

---

## Patch 27 -- kleos-sh extension Write/Edit/MultiEdit pseudo-command (2026-05-23)

### Symptome

Le matcher PreToolUse de `~/.claude/settings.json:273` declare
`Bash|Write|Edit|MultiEdit` pour le hook `kleos-sh.exe --claude-hook
--agent claude-code`. Or `parse_claude_hook_stdin()` dans
`kleos-sh/src/main.rs` n'extrait que `tool_input.command`. Les payloads
Write (`{file_path, content}`), Edit (`{file_path, old_string,
new_string}`) et MultiEdit (`{file_path, edits}`) n'ont pas de champ
`command` -- la fonction retourne `None`, le main exit `0` sans emit de
JSON sur stdout, et Claude Code interprete comme un allow silencieux.
**Resultat : toutes les ecritures et editions de fichier natives Claude
echappent integralement au gate Kleos** (patterns, brain check, audit
log, bridge approvals Patch 21).

### Approche

Niveau **chirurgical** (~50 lignes ajoutees, 1 fonction touchee, 0
refactor). Extraction de la logique de selection dans un helper pur
`extract_command_for_tool(&Value, Option<&str>) -> Option<String>` pour
permettre le test unitaire sans subprocess + stdin pipe.

Pour Write/Edit/MultiEdit, on synthese une **pseudo-commande**
`"<verb>:<file_path>"` (verb = lowercase tool_name) et on l'injecte
dans le champ `command` existant de `GateCheckRequest`. Le serveur
applique sa cascade Patch 25 (regex auto-detect + whitelist +
require_approval + dangerous hardcoded) sur cette string. **Zero
changement cote `kleos-server` / `kleos-lib`** : la cascade traite
`write:/etc/foo.env` comme n'importe quelle commande matchable par
regex.

Cote `gate-rules/` submodule, ajout de patterns dedies dans les fichiers
existants (pas de nouveau fichier) :

- `blocked_patterns.txt` : `^(?:write|edit|multiedit):.*\.env$`,
  `.ssh`, `.pem`, `.key`, `credentials.*\.(json|yaml|yml|toml)$`.
- `require_approval_patterns.txt` : `\.claude/settings\.json`,
  `\.claude/hooks/`, `gate-rules/.*\.txt`, `/etc/*`.

Hot-tunable via cache TTL 5s, identique au flow Patch 19b.

### Fichiers touches

- `kleos-sh/src/main.rs` :
  - `parse_claude_hook_stdin` : appel a `extract_command_for_tool`
    (vs in-line inline `.get("command")` upstream).
  - Nouveau helper `extract_command_for_tool` (~30 lignes, match sur
    `tool_name` avec bras Bash / Write / Edit / MultiEdit / `_`).
  - 6 nouveaux tests unitaires `patch27_extract_*` (Bash regression,
    Write/Edit/MultiEdit pseudo-command, unknown tool returns None,
    missing required field returns None). Total kleos-sh : 4 -> 10
    tests, tous passent (`cargo test -p kleos-sh`).
- `gate-rules/blocked_patterns.txt` : section Patch 27 (~10 lignes
  pattern + commentaires).
- `gate-rules/require_approval_patterns.txt` : section Patch 27
  (~8 lignes pattern + commentaires).
- `CLAUDE.md` racine : ajout Patch 27 dans la liste active.

### Validation locale

```
cargo test -p kleos-sh
# test result: ok. 10 passed; 0 failed; 0 ignored
```

### Validation post-deploy attendue

1. Rebuild `kleos-sh.exe` (Windows MSVC, build local poste operateur).
2. Replace `~/.cargo/bin/kleos-sh.exe` cote poste.
3. Push submodule `gate-rules/` (commit + push origin main) puis
   `git -C /var/lib/kleos/gate pull` cote LXC 121.
4. Test E2E :
   - Tool Write `/tmp/test.env` -> doit recevoir `permissionDecision:
     "deny"` (matche `^(?:write|edit|multiedit):.*\.env$`).
   - Tool Write `/tmp/safe.txt` -> doit passer (pas de pattern matche).
   - Tool Edit `~/.claude/settings.json` -> doit creer une approval
     dans la TUI engram-approval-tui (bridge Patch 21 + 21.1).
5. Bash inchange (regression-check) : `cat /tmp/safe.txt` passe, `rm
   -rf /` toujours bloque par hardcoded dangerous.

### Niveau delta

**Chirurgical** (additif pur sur la branche du match, helper testable
extrait). Zero changement cote `kleos-server` / `kleos-lib`. Zero env
var nouvelle. Zero migration DB.

### Limites assumees

1. Pas de matching contenu (content_preview). Un Edit qui injecte
   `password=foobar` ne matche pas un pattern `(?i)password`. A scoper
   dans un Patch ulterieur avec ajout d'un champ `content_preview` au
   payload + nouveau fichier `content_patterns.txt`.
2. MultiEdit perd la granularite des edits individuels. Acceptable :
   on cible le file_path global, l'operateur peut whitelister par
   fichier.
3. Edge case : fichier dont le nom commence litteralement par `write:`
   ou `edit:` -- improbable, mais documente ici.

### Conditions de retrait

Candidat PR upstream Ghost-Frame (generaliste, sans dette retro-compat
puisque la rebranche Bash est byte-identical au comportement
pre-Patch 27). Le titre PR suggere : "feat(kleos-sh): gate Write/Edit/
MultiEdit via pseudo-command".

agent-forge spec_id : `spec_b6d93363`.

---

## Patch 28 -- KLEOS_PREAUTH_IP_LIMIT + KLEOS_PREAUTH_IP_TRUSTED_FILE (2026-05-23)

### Symptome

Patch 26 (deploye LXC 121 2026-05-23 19:35 UTC) a confirme par WARN tracing
que le HTTP 429 cote operateur poste `192.168.10.100` vient du **preauth IP
rate-limit hardcoded a 20/min** (`kleos-server/src/middleware/rate_limit.rs:15
PREAUTH_IP_LIMIT`). Tally observe immediatement post-deploy (200 dernieres
lignes du log serveur) :

| Path | Rejects | Source dominante |
|---|---|---|
| `/batch` | 90 | `kleos-sidecar` batch_flush |
| `/search` | 2 | `kleos-cli` |
| `/recall` | 2 | `kleos-cli` |
| `/supervisor/pending`, `/store`, `/gate/check`, `/broca/actions`, `/axon/publish`, `/approvals/pending` | 1 chacun | TUI / hooks |

91% des rejects sont alimentes par le sidecar `batch_flush`. Le poste dev
heberge 5+ clients partageant l'IP source (sidecar, TUI, `kleos-cli`,
`kleos-mcp`, hooks Claude Code), ce qui sature naturellement le cap 20/min
en quelques secondes (3 sessions sidecar x 0.5 POST/s = 1.5 req/s = 90/min
steady state, plus retry exp 3x = jusqu'a 270/min en burst).

Le fix structurel cote sidecar (respect `Retry-After` + coalescing + backoff
exponentiel) est le perimeter du peer `kleos-9` (Patch 29 candidat, planifie
dans `docs/dev-notes/sidecar-batch-flush-todo.md`). Patch 28 traite la
moitie serveur : rendre le cap preauth configurable et autoriser une
whitelist d'IPs trusted pour les postes operateur connus.

### Approche

Niveau **chirurgical** (~50 lignes additives), pattern miroir des Patch
existants :

- **Cap configurable** -- pattern Patch 16b (`KLEOS_CONTEXT_TIMEOUT_SECS`) :
  `const DEFAULT_PREAUTH_IP_LIMIT: i64 = 20` (renomme, signal visuel "default
  upstream") + `fn preauth_ip_limit() -> i64` lisant `KLEOS_PREAUTH_IP_LIMIT`
  avec fallback sur le default. Valeurs `<= 0` ou parse-fail fallback
  silencieux (un typo ne peut pas accidentellement desactiver le rate-limit).
- **Trusted IP whitelist par fichier** -- pattern Patch 19b (cascade
  fichier > defaut) : `fn preauth_ip_trusted_file()` resout
  `KLEOS_PREAUTH_IP_TRUSTED_FILE` (override) ou
  `${KLEOS_DATA_DIR}/preauth_ip_trusted.txt` (defaut). `fn
  preauth_ip_trusted_set()` reuse le helper Patch 19b
  `kleos_lib::gate::approval_patterns::load(file, env, defaults)` qui gere
  le cache TTL 5s, parse `#` comments et lignes blanches. On passe `env =
  None` (decision operateur 2026-05-23 : fichier plus maintenable qu'env
  var CSV).
- **Bypass dans `preauth_rate_limit_middleware`** : apres `client_ip_key`,
  strip le prefix `ip:` et test contre `preauth_ip_trusted_set()`. Si
  match, `return next.run(request).await` immediat (zero DB hit, zero
  rate-limit increment). Sinon, appel `preauth_ip_limit()` au lieu de la
  const. WARN log Patch 26 met a jour son champ `limit` pour refleter la
  valeur dynamique.

### Fichiers touches

- `kleos-server/src/middleware/rate_limit.rs` :
  - `use std::collections::HashSet` + `use std::path::PathBuf` +
    `use kleos_lib::gate::approval_patterns` (3 nouveaux imports).
  - `const PREAUTH_IP_LIMIT` -> `const DEFAULT_PREAUTH_IP_LIMIT`.
  - 3 fns helpers : `preauth_ip_limit`, `preauth_ip_trusted_file`,
    `preauth_ip_trusted_set` (~30 lignes documentees).
  - `preauth_rate_limit_middleware` : bypass + `let limit = preauth_ip_limit()`
    + WARN log dynamique (3 lignes additives + 1 ligne modifiee).
- `CLAUDE.md` : 2 nouveaux env vars ajoutes section "Convention env vars",
  1 entree dans la liste Patches actifs.
- `docs/dev-notes/local-patches.md` : cette section.

### Tests

- `cargo check -p kleos-server -p kleos-lib --features kleos-lib/bundled-sqlite` :
  0 errors, 10 warnings (pre-existants `cred::bootstrap` ECDH/PIV fns inutilises
  sur cible Windows, identique pre-Patch 28).
- Pas de test unitaire nouveau cote Patch 28 : le helper `approval_patterns::load`
  est deja teste (12 tests Patch 19b). Les fns helpers Patch 28 sont des
  thin wrappers env-vars + path-resolution suffisamment evidents pour ne pas
  necessiter de test dedie.
- Tests fonctionnels post-deploy LXC 121 :
  1. Env vars unset + fichier absent : comportement upstream identique
     (cap 20/min, WARN reject visible sur burst).
  2. `KLEOS_PREAUTH_IP_LIMIT=500` (via `/etc/kleos/kleos.env` puis
     `systemctl restart kleos-server`) : WARN reject affiche `limit=500` ;
     sidecar sature plus rare.
  3. Creation `${KLEOS_DATA_DIR}/preauth_ip_trusted.txt` avec
     `192.168.10.100` : zero WARN reject pour ce poste, peu importe la
     charge. Cache 5s.

### Niveau delta

**Additif pur** (zero modification de signature, zero impact comportemental
quand les 2 leviers ne sont pas actives, default identique upstream). Le
renommage `PREAUTH_IP_LIMIT` -> `DEFAULT_PREAUTH_IP_LIMIT` est purement
cosmetique (la const n'est utilisee qu'a un seul endroit, le middleware).
Reuse du helper `approval_patterns::load` Patch 19b limite la duplication.

### Conditions de retrait

Candidat **PR upstream Ghost-Frame**. Le besoin "preauth IP configurable +
whitelist trusted" est generaliste (n'importe quel deploiement multi-client
sur une meme IP en a besoin). Si upstream absorbe, le patch s'efface du
fork sans dette. Le helper reuse `kleos_lib::gate::approval_patterns::load`
est specifique au fork (Patch 19b non-upstream pour l'instant), donc en
cas de PR il faudrait soit porter Patch 19b en parallele, soit dupliquer
la logique cache inline.

### Hors scope (Patch 29 candidat, perimeter kleos-9)

- Respect `Retry-After` cote sidecar `flush_pending` (miroir Patch 20/20c TUI).
- Coalescing des sessions cote sidecar (1 POST `/batch` pour N sessions).
- Backoff exponentiel sur 429.

Documente dans `docs/dev-notes/sidecar-batch-flush-todo.md` (a creer par
kleos-9).

agent-forge spec_id : `spec_2764b269`.

---

## Patch 30 -- overlay prompt sidecar gate (id `sidecar/gate/system`) (2026-05-23)

### Symptome

`kleos-sidecar/src/gate.rs:6-22` definit `GATE_SYSTEM_PROMPT` comme une `const &str` hardcoded. Le gate LLM watcher consomme ce prompt pour classer chaque tour assistant de Claude Code en `store|skip`. Le prompt actuel produit beaucoup de faux positifs (FP) :

- Investigation en cours sans conclusion -> `store: true`, importance 7 (devrait etre skip).
- Restate d'instruction operateur -> `store: true` (pollution Kleos).
- Note self-referentielle sur la session courante -> `store: true`.

Simulation Ollama direct (qwen3:8b-ctx16k, reasoning off) sur 8 samples annotes (3 STORE + 5 SKIP) :

| Variante | Precision | Recall | F1 |
|---|---|---|---|
| `v0_original` (prompt hardcoded actuel) | 0.60 | 1.00 | 0.75 |
| `v4_combined` (strict + scale + few-shot) | **1.00** | **1.00** | **1.00** |

v0 cree 2 FP sur 5 SKIP (40% de pollution). v4 cree 0 FP. Le prompt source du sidecar est l'unique levier qui mitige la pollution Kleos a la source -- les autres patches (consolidation, dedup, importance ranking) sont aval.

### Approche

Etendre le systeme overlay Patch 15 (`kleos-lib/src/llm/prompts.rs::load_prompt`) au sidecar **sans modifier la const hardcoded**. La const reste l'embedded default, le file overlay le surcharge quand present. Pattern strictement identique a celui des prompts `broca/*` et `growth/*` deja overlays.

Implementation dans `kleos-sidecar/src/gate.rs::evaluate_single` :

```rust
// Avant (hardcoded direct) :
let response = self.llm.call(GATE_SYSTEM_PROMPT, &truncated, Some(opts)).await...;

// Apres (overlay-aware) :
let system_prompt = prompts::load_prompt("sidecar/gate/system", GATE_SYSTEM_PROMPT);
let response = self.llm.call(&system_prompt, &truncated, Some(opts)).await...;
```

Cascade :
1. `prompts-overrides/sidecar/gate/system.txt` (deploye par operateur cote LXC 121 sous `${KLEOS_DATA_DIR}/prompts/sidecar/gate/system.txt` ou cote poste Windows sidecar via `${KLEOS_DATA_DIR}/prompts/`).
2. Fallback `GATE_SYSTEM_PROMPT` const embedded (zero comportement change).

Cache TTL 5s (`kleos_lib::llm::prompts::TTL_SECS`) -- hot-tune sans restart sidecar.

### Niveau delta upstream

**Additif pur** (~5 lignes, voir table dans CLAUDE.md "Politique d'ecart avec upstream") : 1 import + 1 ligne `load_prompt` + change de `GATE_SYSTEM_PROMPT` -> `&system_prompt` dans le call. La const reste verbatim upstream-alignee.

### Fichiers touches

- `kleos-sidecar/src/gate.rs` -- `use kleos_lib::llm::prompts;` + 2 lignes dans `evaluate_single` (load + use). La const `GATE_SYSTEM_PROMPT` est preservee verbatim.
- `prompts-overrides/sidecar/gate/system.txt` -- nouveau override (v4_combined initial, voir fichier).
- `CLAUDE.md` (gitignore repo) -- ajout `sidecar/gate/system` au catalog overlay.

### Tests

- `cargo check -p kleos-sidecar` : OK (0 errors, 10 warnings pre-existants).
- `cargo test -p kleos-sidecar --lib gate` : 0 passed / 16 filtered (le sidecar n'a pas de tests unit nommes "gate" mais le code compile et les tests filterables restent fonctionnels).
- E2E manuel : restart sidecar, observer dans `kleos-sidecar.out.log` les verdicts `flushing batch through LLM gate ... stored=N skipped=M` pendant 30 min d'usage Claude Code reel. Verifier que les false-positive samples connus (in-progress note, instruction restate, ephemeral status) sont `skipped`.

### Conditions de retrait

Le Patch 15 (parent) est candidat PR upstream. Si upstream absorbe Patch 15 + Patch 16, ce Patch 30 devient un simple ajout au catalog overlay sidecar/gate -- toujours utile. Si upstream ajoute son propre systeme d'overlay du gate sidecar, ce patch s'aligne sur l'API upstream et le const hardcoded reste accessible comme default. Pas de retrait standalone prevu.

### Validation empirique

Methodologie : Ollama direct `192.168.10.16:11434` (zero LXC 121 traffic, evite le rate-limit cascade observe pendant Patch 26+28). Script `/c/Users/Olivier/.claude/tmp/gate_sim.py` (hors-repo) implemente 5 variantes (`v0_original, v1_strict, v2_scale, v3_fewshot, v4_combined`) + scoring precision/rappel/F1 + comparaison per-sample. 8 samples annotes manuellement dans `/c/Users/Olivier/.claude/tmp/samples.json` (3 STORE + 5 SKIP). v4_combined gagne avec score parfait, latence moyenne 4.80s/call vs 3.57s baseline (+35%, sous le timeout 15s du gate).

Biais reconnu : les 8 samples sont construits par l'agent (3 extraits jsonl + 5 synthetiques). Robustesse a renforcer par extension a 20-30 samples piochees aleatoirement dans plusieurs .jsonl recents + annotation par l'operateur. **Le patch d'infrastructure ne depend pas du contenu specifique de l'override** -- l'operateur peut iterer le prompt sans rebuild grace au cache TTL 5s.

---

## Patch 31 -- overlay prompt sidecar compress (id `sidecar/compress/system`) (2026-05-24)

### Symptome

`kleos-sidecar/src/routes.rs:551-560` definit `COMPRESS_SYSTEM_PROMPT` comme une `const &str` hardcoded, consommee par le handler `/compress` (`routes.rs:562`). Ce handler resume un `tool_output` volumineux via Ollama avant stockage memoire. Le prompt n'est appele par AUCUN client actuel (`/compress` est dormant -- mnemonic-observe.sh appelle `/observe` direct, le watcher gate appelle `/store` direct). Mais le mecanisme overlay devrait couvrir TOUS les prompts hardcoded du sidecar, pas seulement le gate, pour preserver le principe "tout prompt hardcoded a un overlay overlayable" introduit par Patch 30.

### Approche

Etendre le systeme overlay Patch 15 (`kleos-lib/src/llm/prompts.rs::load_prompt`) au compress handler, **sans modifier la const hardcoded**. Meme pattern que Patch 30 (gate). La const reste l'embedded default, le file overlay le surcharge quand present.

Implementation dans `kleos-sidecar/src/routes.rs::compress` :

```rust
// Avant (hardcoded direct) :
match llm.call(COMPRESS_SYSTEM_PROMPT, &user_prompt, Some(opts)).await { ... }

// Apres (overlay-aware) :
let system_prompt = prompts::load_prompt("sidecar/compress/system", COMPRESS_SYSTEM_PROMPT);
match llm.call(&system_prompt, &user_prompt, Some(opts)).await { ... }
```

Cascade et cache identiques a Patch 30 (TTL 5s, fallback embedded, zero I/O si overlay absent).

### Niveau delta upstream

**Additif pur** (~5 lignes) : 1 import etendu (`prompts` ajoute a la liste) + 4 lignes (commentaire + load_prompt + change de la reference dans le `call`). La const `COMPRESS_SYSTEM_PROMPT` est preservee verbatim.

### Fichiers touches

- `kleos-sidecar/src/routes.rs` -- import `prompts` + 4 lignes dans le handler `compress`. La const reste verbatim upstream.
- `prompts-overrides/sidecar/compress/system.txt` -- nouveau override INITIAL verbatim copy du hardcoded (zero changement comportemental). Permet d'iterer le prompt sans rebuild quand `/compress` sera grefe dans un caller (hook PostToolUse, mnemonic-observe.sh extension, ou autre).

### Tests

- `cargo check -p kleos-sidecar` : OK (0 errors, 10 warnings pre-existants).
- E2E : `/compress` n'a aucun caller actuel donc pas d'observation runtime possible. La presence du file override + load_prompt resolu correctement au boot (`KLEOS_DATA_DIR=~/.kleos/prompts/sidecar/compress/system.txt` existe) est verifiable via le meme mecanisme que Patch 30 (md5sum du file et lecture via Ollama direct).

### Conditions de retrait

Identique a Patch 30. Si upstream Ghost-Frame absorbe Patch 15 + Patch 16, ce Patch 31 devient un simple ajout au catalog overlay sidecar/compress -- toujours utile. Pas de retrait standalone prevu.

### Pourquoi preventif (zero caller actuel)

- **Coherence** : Patch 30 a fait le travail pour le gate, le meme principe doit couvrir tous les prompts hardcoded du sidecar.
- **Future-ready** : si on decide demain de greffer `/compress` dans `mnemonic-observe.sh` (gain : economie de contexte stocke), le mecanisme overlay sera deja en place. Pas de patch a retro-fitter en urgence.
- **Cout marginal** : ~5 lignes Rust + 1 file override verbatim. Negligeable.

---

## Patch 32 -- agent-forge gain `help` et `schema` sous-commandes (2026-05-24)

### Symptome

`agent-forge` est un CLI a 26 sous-commandes (spec-task, log-hypothesis,
verify, etc.) avec des schemas d'input JSON precis (champs requis, enums
stricts, contraintes "minimum 2 acceptance_criteria", "minimum 3 edge_cases",
"task_type ne tolere pas 'fix' -- attend 'bugfix'"). Aucun mecanisme cote CLI
ne renseigne l'agent IA sur ces contraintes : `agent-forge --help` (clap
auto-genere) ne montre que les **flags** Rust (--input, --output, --db), pas
le **body JSON** attendu. L'agent decouvre les champs par essais successifs :
"Missing required field: task_description" -> ajout -> "Missing required
field: task_type" -> ajout -> "Invalid value: task_type must be one of..." ->
... -> "Minimum 3 edge cases required" -> 5 iterations pour un seul appel.

La rule globale `~/.claude/rules/agent-forge.md` capture les pieges les plus
frequents mais c'est de la doc tribale -- elle peut diverger du code et
n'est pas accessible en session sans lecture explicite par l'agent.

### Approche

Niveau **additif pur** (zero changement comportemental sur les 26
sous-commandes existantes). Patch livre **deux surfaces complementaires** :

- `agent-forge help [<subcommand>]` (humain, texte) -- overview + workflow
  + cheatsheet des 26 sous-commandes, ou schema d'input prose pour une
  sous-commande precise (REQUIRED / OPTIONAL / RETURNS / SIDE EFFECT /
  EXAMPLE). Hardcode en strings dans `tools/help.rs`.
- `agent-forge schema --command <subcommand>` (machine, JSON Schema) --
  derive a la compilation via `schemars` directement sur le struct `*Input`
  que le dispatch path parse reellement. Garanti aligne sur le code.

Les deux ecrivent sur stdout, n'utilisent pas `--input` / `--output`, ne
touchent ni la DB ni le reseau (cout zero pour l'agent qui explore).

Pour permettre `help`/`schema` sans `--input/--output`, les deux flags
deviennent `Option<PathBuf>` au niveau clap, avec validation explicite a
l'execution pour les 26 autres sous-commandes (helper `require_io`). Clap
reserve le nom `help` pour sa sous-commande auto-generee : on la desactive
via `#[command(disable_help_subcommand = true)]` pour liberer le nom (le
flag `--help` reste fonctionnel).

Le pattern hardcode + derive evite le dilemme "doc qui diverge du code" :

- `help` est lisible humain mais peut etre stale -> agent fait `schema` pour
  validation exacte en cas d'erreur repetee.
- `schema` est garanti exact mais moins lisible -> agent prefere `help` en
  premier contact.

Un test `every_known_subcommand_has_schema` force `help::KNOWN_SUBCOMMANDS`
et `schema::for_command` a rester synchronises : ajouter une sous-commande
sans `JsonSchema` derive fait planter le test au CI.

### Fichiers touches

- `agent-forge/src/tools/help.rs` (nouveau, ~440 lignes) : `OVERVIEW`,
  `per_subcmd()`, `KNOWN_SUBCOMMANDS`, 26 const strings, 3 tests.
- `agent-forge/src/tools/schema.rs` (nouveau, ~80 lignes) : `for_command()`
  dispatch via `schema_for!`, 3 tests dont cross-check.
- `agent-forge/src/tools/mod.rs` : ajout `pub mod help;` et `pub mod schema;`.
- `agent-forge/src/main.rs` : `Commands::Help { subcommand }` et
  `Commands::Schema { command }` variants, `--input/--output` deviennent
  `Option<PathBuf>`, `disable_help_subcommand = true`, helpers `run_help` /
  `run_schema` / `require_io`, dispatch stdout avant `Database::open`.
- `agent-forge/Cargo.toml` : `schemars = "0.8"` ajoute.
- `agent-forge/src/tools/{spec,hypothesis,verify,comments,session,think,approaches,skills,stats,ast/repo_map,ast/search}.rs` :
  ajout `use schemars::JsonSchema;` + `#[derive(JsonSchema)]` sur chaque
  struct `*Input` (22 structs totales). Aucun changement de signature ou
  de comportement.
- `~/.claude/claude-config/KLEOS.md` (global) : nouvelle section
  `## 1bis. agent-forge -- structured reasoning workflow` inseree entre
  `kleos-cli` et `kleos-server`, retire agent-forge de la liste des exclus,
  liste les 26 sous-commandes par phase, pointe vers `help` (humain) et
  `schema` (machine). Bandeau staleness etendu pour inclure
  `agent-forge/src/`.
- `CLAUDE.md` projet : entree Patch 32 dans la liste des patches actifs.

### Tests

- `cargo build -p agent-forge` : 0 errors, 5 crates compiles (schemars + 4
  deps transitives), 0 warnings nouveaux.
- `cargo test -p agent-forge --bins` : 11 passed (5 existants
  `tools::approaches::tests` + 3 nouveaux `tools::help::tests` + 3 nouveaux
  `tools::schema::tests`).
- Smoke tests fonctionnels (sortie verifiee) :
  - `agent-forge help` -> overview WORKFLOW + DISCOVERY + 26 sous-commandes,
    exit 0.
  - `agent-forge help spec-task` -> bloc detaille REQUIRED / OPTIONAL /
    RETURNS / EXAMPLE, exit 0.
  - `agent-forge help nonexistent` -> stderr "unknown subcommand" + liste
    des 26 noms valides, exit 2.
  - `agent-forge schema --command spec-task` -> JSON Schema Draft-07 valide
    avec `$schema`, `properties`, `type`. Parseable via `serde_json` (cf.
    `schema::tests::schema_is_valid_json`).
  - `agent-forge schema --command nonexistent` -> stderr similaire au help,
    exit 2.
- Regression : appel `get-spec` avec body invalide retourne envelope
  `{"success": false, ..., "message": "Missing required field: spec_id"}`
  comme pre-Patch 32 (pipeline normal intact).
- Dogfood : verify multi-step contre `spec_6ed1cff9` -> 2/2 pass, marked
  completed via `update-spec`.

### Niveau delta

**Additif pur** sur agent-forge (table de la rule
`~/.claude/rules/rust-upstream-fork.md` -- niveau "additif pur" : nouveau
module, nouvelle fn publique appelee depuis un site upstream non touche).
Cout de rebase upstream estimé tres faible :

- `tools/help.rs` et `tools/schema.rs` sont 100% nouveaux.
- `tools/mod.rs` : 2 lignes ajoutees (`pub mod help;`, `pub mod schema;`),
  zero ligne supprimee.
- 11 fichiers `tools/*.rs` modifies : seulement +1 line par struct (`use
  schemars::JsonSchema;` + ajout au derive `(Deserialize, JsonSchema)`).
  Conflit potentiel uniquement si upstream change la signature d'un struct
  `*Input` -- resolution triviale (preserver le `JsonSchema` dans le
  derive).
- `main.rs` : modifications plus substantielles (variants Help/Schema +
  helpers + flags Option<PathBuf>), mais le code upstream restera
  reconnaissable. Conflit potentiel uniquement si upstream refactor le
  dispatch ou la structure Cli.
- `Cargo.toml` : 1 ligne ajoutee.

### Conditions de retrait

Trois scenarios :

1. **Upstream Ghost-Frame absorbe ce patch via PR** : Patch 32 retire de la
   liste locale, code reste tel quel.
2. **Upstream livre un mecanisme equivalent different** (ex: doc embedded
   different, schema autodecouverte via API serveur) : adapter le code local
   pour s'aligner sur l'API upstream, supprimer le code redondant. Hardcode
   help.rs peut survivre comme catalog supplementaire.
3. **Decision de retrait standalone** : retirer `tools/help.rs`,
   `tools/schema.rs`, le module declaration, les 2 variants, les
   `JsonSchema` derives, et la dep `schemars`. Toutes les modifications sont
   localisables via `git grep "JsonSchema\|tools::help\|tools::schema"` --
   environ 30 sites a retirer.

agent-forge spec_id : `spec_6ed1cff9` (dogfood, cree et cloture via Patch
32 lui-meme).

---

## Patch 33 -- Spaces complete integration (2026-05-25)

### Symptome

Le concept `spaces` existait dans Kleos depuis Ghost-Frame mais a moitie cable :
table `spaces`, colonnes `space_id` indexees sur `memories` et `entities`,
endpoints `/spaces` (POST / GET / DELETE), space `default` auto-cree a la
creation user (`auth_keys/mod.rs:278`). MAIS aucun client ne l'utilisait
jamais.

Sur prod LXC 121 :

- 1 seul space existe (`default` id=2)
- ~3500 memoires existantes en `space_id = NULL` (legacy)
- `conversations` n'avait meme pas la colonne `space_id`
- Aucun endpoint de lecture n'exposait `?space=X`
- kleos-cli / kleos-mcp / hooks ne propageaient pas le space
- Pipeline intelligence (dreamer + sweeps) ignorait totalement le space :
  duplicates / consolidation cross-projet possibles silencieusement

Risque operationnel : lors de l'introduction des `conversations` (skills
`kleos-session-save/load` prevus), le melange cross-projet etait
inevitable (bug similaire a #3028 sur growth).

### Approche

Convention centrale : apres Patch 33, `space_id` est **toujours NOT NULL**
sur tout nouveau write. Le space `default` auto-cree EST la representation
canonique du "cross-projet / non-scope". Les ~3500 memoires legacy NULL
restent visibles en lecture inclusive via une clause additionnelle
transitoire `OR space_id IS NULL`. Naturellement isolees des sweeps par
paire (NULL != NULL en SQL).

Niveau de delta upstream : **additif majoritairement**, avec un patch
chirurgical sur 2 SQL existants (filtres list + post-filter search).

Pieces livrees :

1. **Migration tenant v57** -- ajoute `conversations.space_id` (nullable,
   idempotent, manifest append-only).
2. **Helpers `kleos_lib::space`** (nouveau module) -- `normalize_space_name`,
   `default_space_id` (cache HashMap process), `resolve_or_create_space`,
   `space_belongs_to_user`, `normalize_space_input` (write convention :
   absent / 0 / alias -> default ; id valide -> N ; id invalide -> 400 ;
   name -> resolve_or_create), `resolve_space_filter` (read convention :
   None preserve "no filter" upstream).
3. **Lib types** -- `StoreRequest`, `SearchRequest`, `ListOptions` gagnent
   `pub space: Option<String>` (wire-only) ; `SearchRequest` et
   `ListOptions` gagnent `pub include_unscoped: Option<bool>`.
4. **Handlers memory** -- `store_memory` appelle `normalize_space_input`
   avant `memory::store`. `search_memories`, `explain_search`, `recall`,
   `list_memories` appellent `resolve_space_filter` puis transmettent.
5. **SQL filters lib** -- `memory::list` et `hybrid_search` post-filter
   etendus : `include_unscoped = Some(true)` declenche `space_id IN (?cur,
   ?default) OR space_id IS NULL` ; defaut preserve `= ?cur`.
6. **Anti-leak dreamer** -- `find_duplicates` et
   `find_consolidation_candidates` gagnent `AND ms.space_id = mt.space_id`
   (1 ligne chacun). Empeche dedupe + consolidation cross-projet.
7. **CLI** -- nouveau module `kleos-cli/src/space.rs` (resolve_project_name
   marker > git > cwd, normalize_space_name, determine_space_for_request,
   inject_space_into_body). Flags `--space` / `--space-id` / `--no-space`
   sur Store ; idem + `--include-unscoped` sur Search / Context / List.
   Subcommand `Space { Resolve, List, Ensure }`.
8. **Hook bash** -- nouveau `hooks/full/lib-kleos-space.sh`
   (normalize_space_name, resolve_project_name, ensure_kleos_space_marker,
   write_kleos_space_to_settings, write_kleos_space_per_sid). Wire dans
   `session-start-kleos.sh` : marker `.kleos-space` immuable, env
   `KLEOS_SPACE` exporte, `.claude/settings.json` env.KLEOS_SPACE
   merge via jq, per-sid file `~/.kleos/sessions/<sid>/space_name`.
9. **kleos-mcp** -- `dispatch` lit le per-sid file et injecte `space`
   dans le payload si absent (matche la convention CLI).
10. **Test parity** -- `tests/space-resolution-parity.sh` boucle 7
    scenarios temp et compare `bash resolve_project_name` vs
    `kleos-cli space resolve`. Tout mismatch est un bug.

### Fichiers touches

| Fichier | Niveau delta | Description |
|---|---|---|
| `kleos-lib/src/db/tenant_migrations.rs` + `.manifest` | additif | v57 conversations.space_id |
| `kleos-lib/src/space.rs` | nouveau | helpers convention + cache |
| `kleos-lib/src/lib.rs` | additif | declaration module |
| `kleos-lib/src/memory/types.rs` | additif | champs `space`, `include_unscoped` |
| `kleos-lib/src/memory/mod.rs` | chirurgical | filtre SQL list inclusif |
| `kleos-lib/src/memory/search.rs` | chirurgical | post-filter hybrid_search inclusif |
| `kleos-lib/src/{context,sync,intelligence/correction,ingestion/processors/{raw,extract}}` | additif | `space: None` aux StoreRequest existants |
| `kleos-lib/src/intelligence/duplicates.rs` | chirurgical | `AND ms.space_id = mt.space_id` |
| `kleos-lib/src/intelligence/consolidation.rs` | chirurgical | idem |
| `kleos-server/src/routes/memory/{mod,types}.rs` | chirurgical | normalize + resolve + flags |
| `kleos-server/src/routes/{batch,fsrs,gui,intelligence,onboard,prompts}/mod.rs` | additif | propagate `space: None` aux StoreRequest/SearchRequest existants |
| `kleos-cli/src/main.rs` | refactor local | flags + dispatch + Space subcommand |
| `kleos-cli/src/space.rs` | nouveau | resolve_project_name + helpers |
| `kleos-mcp/src/tools.rs` | additif | auto-inject space per-sid |
| `hooks/full/lib-kleos-space.sh` | nouveau | helper bash partage |
| `hooks/full/session-start-kleos.sh` | additif | wire ensure_kleos_space_marker + exports |
| `tests/space-resolution-parity.sh` | nouveau | test E2E parity |

### Tests

- `cargo test -p kleos-lib --features bundled-sqlite --lib space::` -> 4 tests pass (normalize_strips_accent_and_spaces, normalize_drops_special_chars, aliases_recognized, cache_invalidation_drops_entry).
- `cargo test -p kleos-lib --features bundled-sqlite --lib db::tenant_migrations::tests` -> 96 tests pass (dont `tenant_migrations_obey_append_only_manifest` + `fresh_db_lands_at_latest`).
- `bash tests/space-resolution-parity.sh` -> 7/7 cases OK (test parity bash vs Rust).
- Deploy LXC 121 verification manuelle (cf. plan section Verification).

### Conditions de retrait

Trois scenarios :

1. **Upstream Ghost-Frame absorbe ce patch via PR** : retirer Patch 33 de
   la liste locale, code reste tel quel. Candidat PR upstream apres
   validation prod LXC 121 + observation no cross-space leak sur 7 jours.
2. **Upstream livre un mecanisme equivalent different** (ex: tenant-level
   namespacing, autre table de partitionnement) : aligner les helpers
   et SQL filters sur l'API upstream, supprimer le code redondant.
3. **Decision de retrait standalone** : retirer le module space.rs,
   defaire les changements de StoreRequest/SearchRequest/ListOptions
   (regression upstream-compat possible), retirer les flags CLI et les
   hooks bash. Tous les sites localisables via `git grep
   "space::normalize_space_input\|space::resolve_space_filter\|KLEOS_SPACE"`.

Sujets hors scope de Patch 33 (chantiers futurs documentes dans le plan
`C:\Users\Olivier\.claude\plans\1-pour-viter-le-synchronous-tiger.md`) :

- conversations.rs : helpers + handlers (Section 3 partie 2 reportee)
- growth.rs / list_observations : fix #3028 cross-projet (Section 4
  partie 2)
- brain_query : filtre post-ranking optionnel
- intelligence/contradiction / temporal : refactor structure (fact-based,
  pas pair-based)
- causal / feedback / predictive : audit des INSERT derives en production
- dreamer.rs : boucle externe par space pour growth_reflect 1->N
- Migration NULL legacy -> default (section 11 du plan)

agent-forge spec_id : `spec_5dae3d18` (parent), `spec_94f9a5c5` (section
3 endpoints), `spec_228ecf4f` (wiring), `spec_1ae47040` (section 4
anti-leak).

---

## Patch 33 -- iterations post-deploy (2026-05-25)

Apres le deploy LXC 121 + validation du Patch 33 initial (sections 1, 2,
3 partie 1, 4 partie 1, 6, 7, 8, 9, 10) + Patch 34 (fix /spaces dual-DB),
les sections suivantes ont ete completees dans la meme branche
`local/patch-33-spaces-integration`.

### Patch 33 (5/N) -- Section 3 partie 2 : conversations partitioning

**Commit `6818ab00`**.

`kleos-lib/src/conversations.rs` :

- `Conversation` + `ConversationListItem` exposent `pub space_id:
  Option<i64>`.
- `CONVERSATION_COLUMNS` / `CONVERSATION_LIST_COLUMNS` etendus avec
  `space_id` (index 7 ; `message_count` shifte a 8).
- `CreateConversationRequest`, `BulkInsertRequest`,
  `UpsertConversationRequest` gagnent `space_id` + `space` (wire-only).
- `SearchMessagesRequest` gagne `space_id` + `include_unscoped`.
- `create_conversation` / `bulk_insert_conversation` INSERT colonne
  `space_id`. `upsert_conversation` propage les fields sur le path
  create-fallback.
- `list_conversations` / `list_conversations_by_agent` / `search_messages`
  gagnent params space + filter via helper `build_space_filter(col,
  space_id, include_unscoped, user_id)` (clause inclusive `space_id IN
  (?cur, ?default) OR space_id IS NULL`, ou strict, ou aucun).

`kleos-server/src/routes/conversations/mod.rs` :

- `create` / `bulk_insert` / `upsert` handlers appellent
  `kleos_lib::space::normalize_space_input` avant la fn lib.
- `list` / `search_msgs` handlers appellent `resolve_space_filter` et
  passent (`Option<i64>`, `Option<bool>`) aux fns lib.
- `get_one` handler expose `space_id` dans le JSON output.

`kleos-server/src/routes/conversations/types.rs` :

- `ListConversationsParams` gagne `space` + `space_id` +
  `include_unscoped` (`#[serde(default)]`, back-compat HTTP wire).

### Patch 33 (6/N) -- Section 4 partie 2 : growth partitioning (#3028 fix partiel)

**Commit `b1a4a87...`** (a verifier au prochain push).

`kleos-lib/src/intelligence/growth.rs` :

- `list_observations` signature gagne `space_id: Option<i64>`,
  `include_unscoped: Option<bool>`, `user_id: i64`. SQL filter optionnel
  (None preserve upstream).
- `materialize` lit `source.space_id` et le propage a l'INSERT INTO
  memories (au lieu de NULL hardcoded).
- `reflect` INSERT INTO memories propage `req.space_id` (NULL si non
  fourni = comportement upstream legacy).

`kleos-lib/src/intelligence/types.rs` :

- `GrowthReflectRequest` gagne `pub space_id: Option<i64>`
  (`#[serde(default)]`).

`kleos-server/src/dreamer.rs` :

- 2 sites de construction `GrowthReflectRequest` (lignes 358 et 651)
  gagnent `space_id: None` avec TODO marker pour le refactor outer-loop
  par space (defere).

`kleos-server/src/routes/growth/{mod,types}.rs` :

- `ObservationsQuery` gagne `space` / `space_id` / `include_unscoped`.
- `observations_handler` resout via `resolve_space_filter` et passe le
  triplet a `list_observations`.

`kleos-server/src/routes/prompts/mod.rs` :

- Call site `list_observations` adapte a la nouvelle signature.

Bug #3028 resolution partielle : les observations existantes restent
melangees cross-projet (`space_id = NULL` legacy). Les nouvelles
observations crees avec `req.space_id` rempli iront dans le bon bucket.
Le filtre `GET /growth/observations?space=X&include_unscoped=true`
permet de cibler une projection.

### Items deferes a Patch 35 ou plus tard

| Item | Raison de defer | Reference plan |
|---|---|---|
| `dreamer.rs` outer-loop par space pour growth::reflect | Refactor architecture moyennement large (helpers list_user_spaces + recent_memory_contents_for_space + boucle). Necessite test E2E sur dreamer cycle. | Plan section 4 partie 2 pattern alpha |
| `brain_query` post-ranking filter optionnel | Plan explicite "Brain reste GLOBAL". Sur substrat associatif Hopfield, le partitionnement n'est pas aligne avec la semantique du modele. | Plan section 4 paragraphe Brain |
| `intelligence/contradiction.rs` + `temporal.rs` filter par space | structure fact-based / pattern-based, pas pair-based direct memoire. Refactor non-trivial. | Plan section 4 partie 1 |
| `causal/feedback/predictive` derives INSERT space_id heritage | Plan section 4 mentionnait des lignes (315/177/454/418) qui sont en realite des helpers de tests. Audit confirme : pas de derives production a corriger. | Plan section 4 partie 1 |
| Migration NULL legacy -> default (~3500 memoires) | Chantier dedie avec strategie (UPDATE global vs UPDATE selectif via tags vs DUMP+reinjection). | Plan section 11 |
| KLEOS.md global section 10 "spaces" | Doc operateur, vit dans `claude-config` separe (pas dans ce repo). | Plan section 10 |

---

## Patch 34 -- fix /spaces dual-DB bug (2026-05-25)

### Symptome

Decouvert lors de la validation de Patch 33 (memoire Kleos #4222). Sur LXC
121 post-deploy Patch 33 :

- POST /memory avec `"space": "kleos"` -> normalize_space_input cree la
  row spaces dans le **tenant DB** (`tenants/2/kleos.db`), correct.
- GET /spaces -> retourne uniquement `id=2 name=default` (la row creee
  par `auth_keys/mod.rs:278` au INSERT INTO users).
- sqlcipher de `tenants/2/kleos.db` confirme : 4 spaces presentes
  (`claude-config`, `kleos`, `default`, `smoke-test-bravo`), invisibles
  via l'API.

Cause racine : les 3 handlers `create_space` (l. 355), `list_spaces`
(l. 393), `delete_space` (l. 437) dans `routes/auth_keys/mod.rs`
utilisent l'extractor `State<AppState>` puis `state.db.write(...)` /
`state.db.read(...)`. Mais `state.db` est le **main monolith DB**
(reserve aux tables system-scoped : users, api_keys, audit_log, agents,
app_state). La table `spaces` vit dans le **tenant schema**
(`schema_v44_parity.sql:89-97`), accessible via `ResolvedDb`.

Bug pre-existant upstream Ghost-Frame, rendu visible par Patch 33 quand
les memory handlers ont commence a creer des spaces dans le tenant DB
via `normalize_space_input`.

### Approche

Fix chirurgical : les 3 handlers prennent `ResolvedDb(db): ResolvedDb`
au lieu de `State(state): State<AppState>`, et `state.db` devient `db`
dans les bodies. Niveau **chirurgical** (3 handlers, ~6 lignes
substitutives + 3 commentaires).

Ne touche pas :

- L'auto-creation du space `default` au INSERT INTO users
  (`auth_keys/mod.rs:278`) reste sur `state.db`. C'est un no-op
  effectif post-Patch 33 puisque `kleos_lib::space::default_space_id`
  cree a la demande cote tenant si manquant (INSERT OR IGNORE), donc
  la row main DB orpheline ne bloque rien.
- Le routeur `Router::new().route("/spaces", ...)` reste identique
  (l'extractor est resolu via le type, pas via le mounting).
- Aucune migration de donnees : les spaces orphelines deja creees
  dans le main DB pre-Patch 34 (typiquement `default id=2`) restent
  inertes, pas consultees par le code post-fix.

### Fichiers touches

| Fichier | Niveau delta | Description |
|---|---|---|
| `kleos-server/src/routes/auth_keys/mod.rs` | chirurgical | 3 handlers + 1 import. State<AppState> -> ResolvedDb. state.db -> db. |

### Tests

Build cote operateur via WSL. Verification E2E manuelle post-deploy :

```bash
# Avant : GET /spaces ne montre que id=2 default
curl -sf http://192.168.10.21:4200/spaces -H "Authorization: Bearer $KLEOS_API_KEY"
# Apres deploy Patch 34, doit montrer claude-config + kleos + default + smoke-test-bravo + foo
curl -sf -X POST http://192.168.10.21:4200/spaces \
  -H "Authorization: Bearer $KLEOS_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"name":"foo"}'
curl -sf http://192.168.10.21:4200/spaces -H "Authorization: Bearer $KLEOS_API_KEY"
```

### Conditions de retrait

1. **Upstream Ghost-Frame absorbe ce patch via PR** : retirer Patch 34
   de la liste locale, code reste tel quel. Candidat PR upstream
   immediat (bug objectif, fix minimal, zero impact comportemental
   negatif).
2. **Upstream choisit une autre approche** (ex: deplacer la table
   spaces vers le main DB, garder les handlers cote state.db) : aligner
   le code local sur l'API upstream apres analyse.

Cleanup optionnel post-deploy : la row `default id=2` dans le main DB
(creee par `auth_keys/mod.rs:278` au INSERT INTO users) devient
orpheline. Pas de cleanup automatique dans ce patch ; un script ops
manuel peut faire :

```sql
DELETE FROM spaces WHERE user_id = 1 AND name = 'default';
```

Apres confirmation qu'aucun code legacy ne lit cette row directement
(grep sur `FROM spaces WHERE ... AND name = 'default'` cote main DB :
aucun hit hors auto-creation l. 278).

agent-forge spec_id : `spec_e46b8918` + `hyp_5ed03a1e`.

---

## Patch 35 -- dreamer.rs growth::reflect outer loop par space (2026-05-26)

**Symptome** -- Patch 33 6/N a etendu `GrowthReflectRequest.space_id` et
l'INSERT `growth_observations` cote `kleos-lib`, mais les deux call sites
dans `kleos-server/src/dreamer.rs` (`run_cycle` lignes 339-390 et
`run_cycle_tenants` lignes 645-670) passaient encore `space_id: None`
hardcode avec un TODO Patch 33 explicite. Resultat : les observations
growth generees par le scheduler dreamer pour un user ayant plusieurs
spaces actifs sont produites a partir d'un contexte mixte (faits issus
de N projets melanges) et persistees avec `space_id = NULL`. Bug Kleos
#3028 reste donc partiellement ouvert apres Patch 33 -- les nouveaux
faits stores via `/memory` sont bien partitionnes, mais le pipeline
intelligence background continue de pomper du cross-projet.

**Approche (niveau additif/chirurgical)** -- deux helpers prives ajoutes
en fin du module dreamer :

- `list_user_spaces(db, user_id) -> Result<Vec<Option<i64>>, EngError>`
  : `SELECT DISTINCT space_id FROM memories WHERE user_id = ?1 AND
  is_forgotten = 0 ORDER BY space_id NULLS LAST`. Le bucket legacy
  `NULL` apparait sous forme `None` dans la liste, traite comme un
  space distinct (pas de fusion accidentelle).
- `recent_memory_contents_for_space(db, user_id, space_id, limit)` :
  variant scoped de `recent_memory_contents`. La SQL est branchee :
  `space_id = ?2` pour `Some(id)`, `space_id IS NULL` pour `None`. Le
  helper original (qui ignorait `user_id` -- `_user_id` souligne par
  Patch 33 6/N) est supprime, plus aucun call site ne le consomme.

Les boucles growth deviennent :

```rust
for user_id in &users {
    let spaces = list_user_spaces(db, *user_id).await?; // 1 SELECT par user
    for space_id_opt in &spaces {
        let roll: f64 = rand::random();
        if roll >= GROWTH_REFLECT_CHANCE { continue; } // throttle par paire
        let ctx = recent_memory_contents_for_space(db, *user_id, *space_id_opt, GROWTH_CONTEXT_SIZE).await?;
        if ctx.is_empty() { continue; }
        // merge avec build_dream_context (reste global, brain Hopfield par design)
        let req = GrowthReflectRequest {
            service: "dreamer".to_string(),
            context: merged_ctx,
            existing_growth: None,
            prompt_override: None,
            space_id: *space_id_opt,
        };
        growth::reflect(db, &req, *user_id).await?;
    }
}
```

`build_dream_context` reste calcule une fois par user (le brain
Hopfield est un substrat global par design, cf. plan section 4
paragraphe Brain). Le throttle `GROWTH_REFLECT_CHANCE = 0.2` est
applique par paire `(user, space)`, preservant le profil de charge :
avec N=5 spaces par user, environ 1 reflect par tick par user en
moyenne (loi binomiale).

**Fichiers touches** -- `kleos-server/src/dreamer.rs` uniquement
(2 helpers ajoutes, ancien helper supprime, 2 boucles imbriquees). Zero
touche cote `kleos-lib` (les signatures Patch 33 6/N suffisent). Niveau
**additif/chirurgical** (~80 lignes nettes, dont 2 helpers + 2 boucles
patched + 1 helper retire).

**Tests** -- `cargo check -p kleos-server` : 0 error, 0 warning sur
dreamer.rs apres cleanup du dead helper. Les tests d'integration
(`cargo test -p kleos-server`) plantent rustc sur Windows MSVC avec
`STATUS_STACK_BUFFER_OVERRUN (0xc0000409)` + `datafusion_catalog rlib
cache obsolete` + `invalid metadata files for kleos_lib/kleos_server`.
Aucune erreur ne reference `dreamer.rs`. Validation E2E reelle requise
sur LXC 121 apres build WSL + deploy.

**Conditions de retrait** -- candidat PR upstream apres :
1. Validation E2E sur LXC 121 (verifier que `/growth/observations`
   retourne des observations avec `space_id` peuple sur tenant 2 apres
   un tick dreamer complet).
2. Decision upstream Ghost-Frame d'absorber le concept spaces (cf.
   meme conditions que Patches 33/34).

agent-forge spec_id : `spec_3c68317c` + `hyp_e44b4354`.

---

## Patch 35.1 -- fix regression user_id schema mismatch (2026-05-26)

**Symptome** -- Apres deploy Patch 35 sur LXC 121 (04:47:54 BRT), les
logs montrent un warn a chaque tick dreamer :
`dreamer: failed to enumerate spaces, skipping growth this tick
user_id=<N> error=database error: no such column: user_id in SELECT
DISTINCT space_id FROM memories WHERE user_id = ?1 AND is_forgotten = 0
ORDER BY space_id NULLS LAST`. 3 warns par tick (user 1, user 2 main DB,
user 2 tenant shard). Pipeline intelligence + brain dream + skill
evolution continuent normalement, seul le growth reflect est skipped.

**Cause racine** -- le commit `61346ec0 refactor(tenant): Phase 5.1 -
drop user_id from memory core` (git log dreamer.rs) a supprime la
colonne `memories.user_id` parce que les tenant shards isolent deja par
DB. C'est pourquoi le helper original `recent_memory_contents` ecrivait
`_user_id: i64` souligne (parametre intentionnellement ignore). Patch
35 avait ajoute `WHERE user_id = ?1` sur `memories` sans verifier la
discipline cross-call-site.

**Fix (additif/chirurgical, dreamer.rs uniquement)** -- deux ajustements :

- `list_user_spaces` query la table `spaces` (qui garde `user_id`,
  cf. `kleos-lib/src/space.rs:87`) au lieu de `memories.user_id`.
  Append systematique de `None` (bucket legacy NULL) pour conserver
  la traversee du bucket transitoire.

  ```rust
  SELECT id FROM spaces WHERE user_id = ?1 ORDER BY id
  // puis spaces.push(None);
  ```

- `recent_memory_contents_for_space` retire `user_id` du WHERE (les
  tenant shards isolent deja par DB). Le parametre `_user_id` est
  conserve souligne pour API symmetry et alignement upstream avec le
  helper original supprime.

**Fichiers touches** -- `kleos-server/src/dreamer.rs` uniquement
(2 helpers reecrits, zero changement de signature). Niveau
**chirurgical**, ~15 lignes nettes.

**Tests** -- `cargo check -p kleos-server` : 0 error, 0 warning sur
dreamer.rs (0.61s). Verification E2E : observer les prochains ticks
dreamer apres redeploy, le warn doit disparaitre et `growth_calls`
doit incrementer dans `DreamerStats.totals`.

**Lecon pour les patches futurs** -- toujours grep cross-call-site
les symboles touches (colonnes, methodes) avant d'ecrire une clause
SQL. Le `_user_id` souligne dans l'helper original etait un tell
direct que la colonne n'existait plus. Discipline rappelee dans la
rule `.claude/rules/kleos-patching-discipline.md`.

agent-forge : meme `spec_3c68317c` que Patch 35 (continuite du meme
chantier), nouvelle `hyp_705f30a5`.

---

## Patch 36 -- brain_query post-ranking filter par space (2026-05-26)

**Symptome / motivation** -- Le plan Patch 33 section 4 paragraphe Brain
prevoit que le brain Hopfield reste un substrat associatif **global**
(la dream_cycle est par design partagee entre tous les contextes pour
preserver la dynamique d'attraction), tout en exposant un filtrage
**optionnel cote query** : un caller qui sait dans quel projet il
travaille doit pouvoir restreindre les patterns retournes au scope du
space courant. Patch 33 6/N a livre `space_id` sur `memories` mais
n'a pas touche `/brain/query`, donc le caller ne pouvait pas filtrer.

**Approche (niveau additif/chirurgical, kleos-server uniquement)** --
implementer le filtre **apres** ranking Hopfield, cote handler :

1. Wrapper local `BrainQueryRequest` dans
   `kleos-server/src/routes/brain/types.rs` qui flatten l'upstream
   `BrainQueryOptions` (back-compat strict des payloads existants) et
   ajoute trois champs :

   ```rust
   #[derive(Debug, Deserialize)]
   pub struct BrainQueryRequest {
       #[serde(flatten)]
       pub inner: BrainQueryOptions,
       #[serde(default)]
       pub space: Option<String>,
       #[serde(default)]
       pub space_id: Option<i64>,
       #[serde(default = "default_include_unscoped")]
       pub include_unscoped: bool,
   }
   ```

2. `query_handler` (mod.rs) gagne l'extractor `ResolvedDb`, resolve le
   target via `kleos_lib::space::resolve_space_filter(&db, user_id,
   space_id, space)`, appelle `brain.query` **inchange** avec
   `&body.inner`, puis applique le filtre :

   ```rust
   if let Some(target_id) = target_space {
       let ids: Vec<i64> = result.activated.iter().map(|m| m.id).collect();
       if !ids.is_empty() {
           let space_map = load_memory_space_ids(&db, &ids).await?;
           let include_unscoped = body.include_unscoped;
           result.activated.retain(|m| match space_map.get(&m.id) {
               Some(Some(sid)) => *sid == target_id,
               Some(None) => include_unscoped,
               None => false,
           });
       }
   }
   ```

3. Helper local `load_memory_space_ids(db, &[i64]) -> HashMap<i64,
   Option<i64>>` fait un seul `SELECT id, space_id FROM memories WHERE
   id IN (?,?,...)` via `rusqlite::params_from_iter`. Le set est
   borne par `top_k` brain (typiquement <=50), bien sous la limite
   SQLite 999 placeholders.

**Comportement** :

| Payload | Resultat |
|---|---|
| Aucun `space` / `space_id` | `brain.query` inchange (back-compat strict) |
| `space="kleos"` + defaut `include_unscoped=true` | `activated` retient ceux dont `space_id = id(kleos) OR space_id IS NULL` |
| `space="kleos"` + `include_unscoped=false` | `activated` retient strictement `space_id = id(kleos)` |
| `space_id=N` valide pour le user | meme regle que `space` resolu en N |
| `space_id=N` qui n'appartient pas au user | 400 `InvalidInput` via `resolve_space_filter` |

Le moteur Hopfield voit toujours le set global (preserve la dynamique
d'attraction). Le filtre s'applique apres ranking ; `top_k` peut donc
renvoyer moins de N apres filtre, ce qui est la semantique attendue
("retourne jusqu'a top_k patterns globalement actives qui sont dans ce
space").

**Fichiers touches** -- `kleos-server` uniquement :
- `kleos-server/src/routes/brain/types.rs` : nouveau wrapper
  `BrainQueryRequest` (~30 lignes).
- `kleos-server/src/routes/brain/mod.rs` : import update, signature
  `query_handler` change, post-filter ajoute, helper
  `load_memory_space_ids` (~70 lignes nettes).

Zero touche cote `kleos-lib` (`BrainQueryOptions`, `BrainMemory`,
`BrainQueryResult` restent strictement upstream). Niveau **additif**.

**Tests** -- `cargo check -p kleos-server` : 0 error, 0 warning sur les
fichiers modifies. Verification E2E reportee a build WSL + redeploy
LXC 121.

**Conditions de retrait** -- candidat PR upstream apres :
1. Validation E2E sur LXC 121 (smoke test : `POST /brain/query
   {"query":"...","space":"kleos","include_unscoped":false}` retourne
   un set strictement scope kleos).
2. Absorption upstream du concept spaces (meme prerequis que Patches
   33/34/35).

agent-forge spec_id : `spec_d8ad66d8`.

---

## Patch 37 -- intelligence contradiction + temporal filter par space (2026-05-26)

**Symptome / motivation** -- Le plan Patch 33 section 4 prevoit que
chaque passe paire du pipeline intelligence applique le filtre
`a.space_id = b.space_id` pour eviter les fusions cross-space en
background. Patch 33 partie 1 (commit `588afb78`) a couvert
`duplicates` et `consolidation`. Restaient `contradiction` (pair
detection sur structured_facts) et `temporal` (pair fact contradiction
+ pattern recurrence detection). Sans ce filtre, une contradiction
detectee entre 2 memoires de projets differents serait remontee a tort,
ou un pattern temporel mergerait des memoires cross-projet.

**Approche (niveau chirurgical + refactor local, kleos-lib uniquement)** :

1. `contradiction.rs::scan_all_contradictions` -- ajouter 3 lignes SQL :
   ```sql
   JOIN memories m1 ON m1.id = sf1.memory_id
   JOIN memories m2 ON m2.id = sf2.memory_id
     AND m1.space_id = m2.space_id
   ```
   NULL legacy isole naturellement (NULL = NULL faux en SQL).

2. `temporal.rs::detect_fact_contradictions` -- consommer le parametre
   `_memory_id` (precedemment underscored, signature externe stable),
   ajouter `JOIN memories m_cand ON ... JOIN memories m_new ON m_new.id
   = ?4 WHERE ... AND m_cand.space_id = m_new.space_id` au candidate
   query. 4eme placeholder bind sur `memory_id`.

3. `temporal.rs::detect_patterns` -- refactor local :
   - SQL ajoute `space_id` au SELECT
   - `HashMap<String, Vec<(i64, i64)>>` devient
     `HashMap<(Option<i64>, String), Vec<(i64, i64)>>`
   - Group key change de `category` a `(space_id, category)`
   - Pattern description inchangee (utilise `category` seul, le scope
     space etant implicite via memory_ids qui appartiennent au meme
     bucket)

**Fichiers touches** :
- `kleos-lib/src/intelligence/contradiction.rs` (~10 lignes ajoutees
  dont 7 commentaires) -- scan_all_contradictions JOIN clause.
- `kleos-lib/src/intelligence/temporal.rs` (~30 lignes touchees) --
  detect_fact_contradictions (JOIN + bind param), detect_patterns
  (SELECT + HashMap + loop tuple destructuring).

Zero touche cote `kleos-server` (les signatures publiques
`scan_all_contradictions(db, user_id)`, `detect_fact_contradictions(...,
memory_id, ...)`, `detect_patterns(db)` restent identiques). Callers
intelligence/mod.rs et intelligence/scheduler.rs inchanges.

**Comportement** :

| Cas | Resultat |
|---|---|
| 2 memoires meme space (kleos id=2 et kleos id=2) | Paire detectee normalement (filtre passe) |
| 2 memoires spaces differents (kleos et default) | Paire ignoree |
| 2 memoires NULL legacy | Paire ignoree (NULL = NULL faux), isolation safe-by-default |
| detect_patterns : N memoires meme category split sur 3 spaces | 3 buckets analyses separement, chacun doit atteindre MIN_SAMPLE_SIZE |
| detect_fact_contradictions appele avec memory_id inexistant | Subquery m_new retourne 0 ligne, INNER JOIN echoue, 0 candidat (skip silencieux) |

**Tests** -- `cargo check -p kleos-lib --features bundled-sqlite` :
0 error, 10 warnings (tous pre-existants dans cred/bootstrap.rs).
`cargo check -p kleos-server` : 0 error, 10 warnings (idem).
Validation agent-forge `verify` : 2/2 steps passed.

Tests integration kleos-lib non-executes cote Windows MSVC pour ICE
STATUS_STACK_BUFFER_OVERRUN pre-existant (memoire Kleos #4545,
independant). Validation E2E LXC 121 reportee a build WSL + redeploy.

**Conditions de retrait** -- candidat PR upstream apres :
1. Validation E2E sur LXC 121 (smoke : creer 2 memoires meme category
   dans 2 spaces differents, declencher dreamer scheduler, verifier
   aucun temporal_pattern ne merge les 2 ; idem pour
   scan_all_contradictions via 2 structured_facts cross-space).
2. Absorption upstream du concept spaces (meme prerequis que Patches
   33/34/35/36).

agent-forge spec_id : `spec_cbf300ff`.

---

## Patch 37.1 -- detect_contradictions(memory) filtre par space (2026-05-27)

**Symptome / motivation** -- Patch 37 (commit `f2d0327b`) a applique le
filtre par space sur les passes paire offline (`scan_all_contradictions`,
`detect_fact_contradictions`, `detect_patterns`) mais a oublie la
fonction `detect_contradictions(memory)` (kleos-lib/src/intelligence/
contradiction.rs ligne 22), appelee par le pipeline **online** a chaque
ingestion de memoire. Cette fn compare la new memoire aux
`structured_facts` pre-existants en filtrant uniquement par
`subject+predicate`, sans contrainte de space. Resultat : 2 projets
differents qui partagent un meme triplet subject+predicate (ex: deux
projets qui declarent `agent-forge expose --help`) auraient produit une
contradiction false-positive immediatement a l'ingestion.

**Approche (niveau chirurgical, kleos-lib uniquement)** -- Sibling du
Patch 37 avec le meme pattern que `temporal.rs::detect_fact_contradictions`.
La fn prend deja `memory: &Memory` en parametre et utilise `memory_id`
comme placeholder `?3` dans la query candidate. Pas besoin d'ajouter un
nouveau param : on peut JOIN sur `?3` directement.

Avant :
```sql
SELECT sf.id, sf.object, sf.memory_id, sf.confidence
FROM structured_facts sf
WHERE sf.subject = ?1 AND sf.predicate = ?2
  AND sf.memory_id != ?3
  AND sf.id != ?4
ORDER BY sf.confidence DESC
```

Apres :
```sql
SELECT sf.id, sf.object, sf.memory_id, sf.confidence
FROM structured_facts sf
JOIN memories m_cand ON m_cand.id = sf.memory_id
JOIN memories m_new ON m_new.id = ?3
WHERE sf.subject = ?1 AND sf.predicate = ?2
  AND sf.memory_id != ?3
  AND sf.id != ?4
  AND m_cand.space_id = m_new.space_id
ORDER BY sf.confidence DESC
```

3 lignes JOIN + 1 clause egalite. Signature `pub async fn
detect_contradictions(db: &Database, memory: &Memory) -> Result<Vec<
Contradiction>>` inchangee. Ordre des placeholders inchange.

**Fichiers touches** :
- `kleos-lib/src/intelligence/contradiction.rs` (~15 lignes ajoutees
  dont 9 commentaires) -- query candidate dans `detect_contradictions`.

Zero touche cote `kleos-server` ni autres modules kleos-lib. Callers du
pipeline online (services/memory.rs, intelligence/mod.rs) inchanges.

**Comportement** :

| Cas | Resultat |
|---|---|
| New memoire space=kleos, candidate fact memoire space=kleos | Paire detectee normalement |
| New memoire space=kleos, candidate fact memoire space=default | Paire ignoree (clause egalite faux) |
| New memoire space=kleos, candidate fact memoire space=NULL legacy | Paire ignoree (NULL != kleos) |
| New memoire space=NULL legacy, candidate fact memoire space=NULL legacy | Paire ignoree (NULL = NULL false, safe-by-default) |
| Fact orphelin sans memoire correspondante en DB | INNER JOIN m_cand filtre, fact non considere |
| memory.id absente de la table memories (cas test inconsistant) | JOIN m_new retourne 0 ligne, 0 candidat, skip silencieux |

**Tests** -- `cargo check -p kleos-lib --features bundled-sqlite` :
0 error, 10 warnings (tous pre-existants dans cred/bootstrap.rs).
`cargo check -p kleos-server --features kleos-lib/bundled-sqlite` :
0 error. Validation agent-forge `verify` : 2/2 steps passed.

Tests integration kleos-lib non-executes cote Windows MSVC (ICE
STATUS_STACK_BUFFER_OVERRUN pre-existant -- memoire Kleos #4545,
independant). Validation E2E LXC 121 et smoke FR cross-space contradiction
reportes au build WSL + redeploy.

**Conditions de retrait** -- candidat PR upstream **avec** Patch 37 (les
deux constituent une unite logique : filtre par space sur les 4 passes
contradiction/temporal). Voir aussi conditions Patch 37.

agent-forge spec_id : `spec_9be9c0f3`.

---

## Patch 38 -- i18n core lexicon + 16 sites refactores (2026-05-27)

**Symptome / motivation** -- L'audit `docs/dev-notes/i18n-audit.md` (2026-05-26) a identifie 15 sites Kleos qui hardcodent du vocabulaire anglais (extraction, personality, valence, sentiment, decomposition, hopfield, services/brain, prompts, gate, handoffs). Empiriquement sur LXC 121 : ~2 structured_facts produits pour 2282 memoires, soit un taux <0.1% causé directement par le mismatch entre les regex EN et les memoires majoritairement FR/tech. Le pipeline d'intelligence (contradictions, valence, personality, sentiment) est de facto dead-end fonctionnel hors anglais.

L'operateur a choisi **Option B** : module i18n core lexicon central + couche de normalisation au matching. Plan original dans `~/.claude/plans/je-pr-f-re-b-complet-robust-kazoo.md`.

**Approche** -- Niveau delta `additif + chirurgical multi-sites`, kleos-lib principalement (1 site cote kleos-server). 3 livrables :

1. **Livrable 1** -- Nouveau module `kleos_lib::lexicon` avec API `word_class(lang, class)`, `word_class_alternation`, `supported_languages`, `class_emotion_metadata`. Cascade `KLEOS_LEXICON_REPOSITORY` env > `${KLEOS_DATA_DIR}/lexicon/` > embedded baselines (`kleos-lib/lexicon/{en,fr}.toml`). Cache TTL 5s + Arc<ParsedLexicon> + OnceLock + RwLock (pattern miroir de `kleos_lib::llm::prompts.rs` Patch 15).

2. **Livrable 2.A** -- 12/12 sites Layer A pur consomment `lexicon::word_class` au lieu de constantes Rust : STATE_VERBS, PROHIBITIONS, SCRUB_PATTERNS, articles, INTENSIFIERS, causal+negation, stopwords, EMOTION_KEYWORDS, FILLER_PREFIXES, META_STOPLIST, infer_domain, SENTIMENT_LEXICON.

3. **Livrable 2.A.normalize** -- Couche de normalisation au matching pour absorber accents et morphologie :
   - Crates ajoutees : `unicode-normalization` 0.1, `rust-stemmers` 1.2 (workspace, pures Rust, ~100 KiB total).
   - API publique `lexicon::fold_for_matching(s, lang, with_stem)` : lowercase + NFD strip + Snowball stemming optionnel.
   - API publique `lexicon::fold_word_for_class(word, lang, class)` : consulte le metadata `stem` de la classe.
   - TOMLs FR re-orthographies avec les accents corrects (~70 mots accentues sur 29 classes).
   - TOMLs EN re-orthographies avec apostrophes vraies dans contractions (`didn't`, `I'm just`, etc.).
   - 9 classes mots-grammaire marquees `stem = false` pour eviter over-stemming sur les tokens courts ou techniques (state_verbs, articles, stopwords, first_person_pronoun, negation_marker, 5 intensifier_* tiers, credential_keywords).
   - 8 / 11 sites L2.A patches utilisent le folding ; 3 sites position-based (clean_subject, hopfield causal, decomposition strip_filler) preservent leur comparaison surface car le folding casserait les indices de position (strip_prefix / starts_with + word indices).

4. **Livrable 2.B partial 3/4** -- Refactor des regex multi-langues en templates parametriques par langue :
   - extraction.rs 5/12 regex i18n-portables : like_regex_for, dislike_regex_for, favorite_regex_for, location_regex_for, role_regex_for. Les 7 autres (buy, spent, have, exercise, made, earned) sont differables car elles encodent une syntaxe unit/currency EN-only.
   - personality.rs 7/7 patterns : LIKE_PATTERN, DISLIKE_PATTERN, FAV_PATTERN, DECISION_PATTERN, IDENTITY_PATTERN, VALUE_PATTERN, MOTIVATION_PATTERN.
   - handoffs/atoms.rs 4/4 patterns : RE_DECISION, RE_CONSTRAINT, RE_TASK, RE_QUESTION (RE_ENTITY_PATH et RE_ENTITY_LABEL restent inchanges, ils encodent du structurel non linguistique).
   - valence.rs 0/22 EMOTION_PATTERNS differable (chaque pattern porte valence + arousal metadata, decoupage TOML en 22 classes lourd).

**Submodule** -- Nouveau `lexicon-overrides/` mappe vers `VOCSAP/Kleos.lexicon` (private). Pre-loaded avec les TOMLs EN/FR au moment de la creation. README du repo + README principal du repo Kleos documentent le pattern overlay (Patches 15, 19b, 38) sans nommer les repos prives.

**Classes lexicon livrees** (en + fr.toml chacun) :
- Grammar (stem=false) : verb_like, verb_dislike, verb_buy, state_verbs, articles, stopwords, first_person_pronoun, negation_marker, 5x intensifier_*, credential_keywords, atom_decision_markers, atom_constraint_markers, atom_task_markers, atom_question_markers, decision_verbs, identity_markers, value_markers, motivation_markers, favorite_marker, is_or_are, favorite_category, location_verbs, role_verbs.
- Semantic (stem=true default) : 17 emotion_* (avec valence + intensity metadata), 5 causal_*, filler_prefixes, meta_stoplist, 7 domain_*, prohibition_marker, 10 sentiment_* (par bucket de score).

Total : 47 classes EN + 47 classes FR.

**Fichiers touches (additifs)** :
- Cargo.toml workspace deps (+2)
- kleos-lib/Cargo.toml (+2 deps)
- kleos-lib/src/lib.rs (+1 pub mod)
- kleos-lib/src/lexicon/{mod,loader,cache}.rs (nouveau module, ~700 lignes)
- kleos-lib/lexicon/{en,fr}.toml (embedded baselines, ~250 lignes chaque)
- docs/dev-notes/{i18n-audit,patch-38-regex-overrides-design,patch-38-remaining-work}.md
- README.md (section Runtime overlays)
- lexicon-overrides/ (nouveau submodule)
- .gitmodules

**Fichiers touches (refactor)** :
- kleos-lib/src/intelligence/{extraction,temporal,decomposition,sentiment}.rs
- kleos-lib/src/{personality,prompts}.rs
- kleos-lib/src/brain/hopfield/recall.rs
- kleos-lib/src/services/brain.rs
- kleos-lib/src/handoffs/atoms.rs
- kleos-server/src/routes/gate/mod.rs

**Commits sur branche `local/patch-38-i18n-core`** :
- `cb72ed09` feat(i18n): Livrable 1 lexicon core module
- `9e721a5b` docs(i18n): Kleos.lexicon submodule + overlay pattern
- `e5932477` docs(i18n): describe overlay intent without naming private repos
- `22f09d4f` a `5e874aa3` : Livrable 2.A sites 1-11 (12 commits)
- `6369d5af` Livrable 2.A site 12 sentiment AFINN
- `039c0083` + `1c3b7587` : normalize fold_for_matching helper + accents FR
- `0819c066` Livrable 2.B 1/4 extraction
- `ddb4ec45` Livrable 2.B 2/4 personality
- `f0b556a5` Livrable 2.B 3/4 atoms

**Verification** : `cargo check -p kleos-lib --features bundled-sqlite` et `cargo check -p kleos-server --features kleos-lib/bundled-sqlite` passent avec 0 erreur a chaque commit (10 warnings pre-existants dans cred/bootstrap.rs, non lies). Tests integration kleos-lib non executes cote Windows MSVC pour bug ICE pre-existant (memoire Kleos #4545, independant). Validation E2E LXC 121 + smoke FR cross-space contradiction reportes apres build WSL + redeploy.

**Conditions de retrait** -- ce patch est intrinsequement local (zero changement de behavior par defaut quand `KLEOS_LEXICON_REPOSITORY` est unset et que `${KLEOS_DATA_DIR}/lexicon/` est absent : le cascade tombe sur les embedded baselines). Candidat PR upstream une fois Livrables 2.B complet (valence + 7 extraction unit-specific) et Livrable 3 (migrations + admin endpoints) livres et valides E2E.

agent-forge spec_ids : `spec_9737ef82` (L1), `spec_477f37f9` (L2.A), `spec_afa9794c` (normalize), `spec_118f0313` (L2.B partial 3/4).

### Patch 38 -- L2.B wildcard-after-stem (2026-05-27)

**Symptome / motivation** -- Smoke E2E 2026-05-27 16:53 BRT a montre que les 37 patterns L2.B (5 extraction + 7 personality + 4 atoms + 21 valence) ne matchent pas les conjugues FR : `j'aime` rate parce que le TOML liste `aimer` et que `word_class_alternation` injecte les words bruts sans passer par `fold_for_matching`. Asymetrie avec L2.A (qui folde les deux cotes de la comparaison). Memoire issue Kleos #5608 trace le diagnostic. `j'adore` matche par coincidence lexicale FR/EN (le TOML EN liste `adore`).

**Approche** -- Niveau delta `additif + chirurgical local` (touche uniquement du code Patch 38 deja local). Solution **1-phase wildcard-after-stem** preferee a la two-phase match parce que la phase 1 stem-source + regex-stem ne distingue pas mieux les false-positifs (`aimable` se stem aussi en `aim`) tout en doublant le cout d'execution. Le risque over-match est explicitement tolere au MVP et mesurable post-deploy.

**Mecanique** :

- Nouveau helper `kleos_lib::lexicon::word_class_alternation_stemmed(lang, class) -> String` qui retourne la concatenation pipe-joined des words stemmes via `fold_for_matching` (respect `stem = false` sur classes grammaticales, multi-word entries gerees par split-stem-rejoin existant).
- Helper `class_stem_enabled(lang, class)` rendu `pub` pour atoms.rs et valence.rs qui doivent stemmer leurs markers multi-mots tout en preservant la collapse `\s+` (atoms) ou la concatenation pipe-joined (valence).
- Sites refactores : chaque template regex remplace `(?:{alternation})` par `(?:{alternation_stemmed})\w*` -- le wildcard absorbe les conjugues et accords. Captures `(.+?)` restent sur source raw -> objects extraits preservent accents/casse.
- valence.rs inclus dans le refactor (cout marginal, bonus FR reel sur accords feminin/pluriel comme `fatiguee`/`fatigues`/`joyeuses`, pas de capture donc 0 risque regression).

**Fichiers touches (refactor local)** :
- kleos-lib/src/lexicon/mod.rs (+helper + 5 tests + 1 fn `pub`)
- kleos-lib/src/intelligence/extraction.rs (5 patterns)
- kleos-lib/src/personality.rs (7 patterns)
- kleos-lib/src/handoffs/atoms.rs (4 patterns via `build_atom_regex` refactore en `(lang, class)`)
- kleos-lib/src/intelligence/valence.rs (21 patterns via LazyLock loop)
- docs/dev-notes/i18n-audit.md (section 7 mise a jour)
- docs/dev-notes/patch-38-remaining-work.md (section 1 cloturee)

**Verification** : `cargo check -p kleos-lib --features bundled-sqlite --lib` passe a 0 erreur (10 warnings pre-existants cred/bootstrap.rs, non lies). Tests cfg(test) non executables cote Windows MSVC pour dette pre-existante (`StoreRequest space` field manquant dans plusieurs fichiers de test) heritage de Patch 33+, hors scope. Validation tests lexicon prevus via WSL build operateur + smoke E2E FR post-deploy LXC 121.

**Conditions de retrait** -- Comportement par defaut preserve quand TOMLs FR sont vides (pas de match). Quand TOMLs FR sont peuples (aimer, adorer, ...), le `\w*` permet de matcher les conjugues sans dupliquer le TOML. Au prochain rebase upstream, candidat PR upstream unique avec le reste du Patch 38.

agent-forge spec : `spec_01e9984f`.

---

## Binaires compilés pour chaque plateforme

| Binaire | Windows (MSVC) | Linux musl (WSL) |
|---|---|---|
| kleos-server | non (Linux uniquement) | oui |
| kleos-cli | non (Linux uniquement) | oui |
| kleos-mcp | non (Linux uniquement) | oui |
| kleos-sh | oui | non |
| agent-forge | oui | non |
| kleos-sidecar | oui | non |
| kleos-cred (cred + derive-db-key) | oui | non |
