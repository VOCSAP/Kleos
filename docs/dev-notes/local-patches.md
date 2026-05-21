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
| **18 -- kleos-mcp allowlist (`KLEOS_MCP_TOOL_ALLOWLIST`)** (NOUVEAU 2026-05-21) | Filtre additif sur `kleos-mcp/src/tools.rs::registry()` pour restreindre la registry MCP (474 routes -> 15 a 140 selon profil Minimal/Standard/Advanced). Matcher manuel exact + suffix `.*`. Var unset/vide = comportement upstream. Tests: 7 unitaires inline. Cf. section dediee "Patch 18" plus bas. | a commiter |
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

**Hooks deployes mais retires en attente d'audit (2026-05-21)** :

| Hook | Event prevu | Raison du retrait |
|---|---|---|
| `enforce-kleos-search.sh` | PreToolUse Bash | A auditer : gate bloquant sur kleos-cli search, coherence avec code serveur courant a verifier |
| `user-prompt-lean.sh` | UserPromptSubmit `""` | A auditer : context injection lean, alignement avec routes kleos-server v1.1.5 a verifier |
| `post-tool-kleos-prompt.sh` | PostToolUse Bash | A auditer : prompt injection post-bash, alignement avec routes a verifier |
| `track-agent-forge.sh` | PostToolUse `.*` | A auditer : state machine pour enforce-agent-forge, marker files a verifier |
| `mnemonic-observe.sh` | PostToolUse `.*` | A auditer : fire-and-forget vers kleos-sidecar `/observe`, route et payload a verifier (kleos-sidecar est le Rust binary VOCSAP, pas le Node legacy) |

Chacun fera l'objet d'une analyse dediee (audit ligne par ligne contre le code kleos-server + kleos-sidecar Rust courant, comme fait pour `session-start-kleos.sh` en cette session) avant remise en service.

### Conditions de retrait

Aucune. Tant qu'upstream ne re-introduit pas le bundle `hooks/*` avec une logique equivalente, ces fichiers vivent en permanence dans `local/patches`. Une partie peut etre proposee en PR upstream (notamment le drain hook qui complete la route `/supervisor/pending` deja presente upstream sans drainer).

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
