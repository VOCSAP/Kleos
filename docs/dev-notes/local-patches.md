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

## Procédure d'intégration des releases upstream (depuis v1.1.0)

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
