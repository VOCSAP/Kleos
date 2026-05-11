# Local Patches -- Kleos VOCSAP Fork

**Date de création :** 2026-05-11
**Contexte :** Ce fichier répertorie tous les changements locaux (non upstream) appliqués
sur la branche `main` VOCSAP. À consulter impérativement avant tout `git merge` ou
`git pull` depuis Ghost-Frame/Kleos pour identifier les conflits prévisibles et les
re-appliquer si perdus.

---

## Patch 1 -- Windows port : SQLite bundled pour agent-forge

**Fichier :** `agent-forge/Cargo.toml`
**Statut upstream :** Absent. Jamais soumis en PR.
**Symptôme si absent :** `LINK : fatal error LNK1181: cannot open input file 'sqlite3.lib'`

```toml
# Ajouter à la fin du fichier :
[target.'cfg(windows)'.dependencies]
rusqlite = { version = "0.31", features = ["bundled"] }
```

**Pourquoi :** Pas de sqlite3.lib système sur Windows. `bundled` compile SQLite depuis les sources.

---

## Patch 2 -- Windows port : sqlcipher feature pour kleos-sidecar

**Fichier :** `kleos-sidecar/Cargo.toml`
**Statut upstream :** Absent. Seul crate kleos-lib-dépendant sans override Windows.
**Symptôme si absent :** `LINK : fatal error LNK1181: cannot open input file 'sqlite3.lib'`

```toml
# Ajouter à la fin du fichier :
[target.'cfg(windows)'.dependencies]
kleos-lib = { path = "../kleos-lib", version = "1.0.0", features = ["sqlcipher"] }
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

## Patch 4 -- Windows port : OpenOptionsExt gated dans derive-db-key

**Fichier :** `kleos-cred/src/bin/derive-db-key.rs`
**Statut upstream :** Absent. Seul fichier kleos-cred avec import Unix non-gatté.
**Symptôme si absent :** `error[E0433]: use of undeclared type 'OpenOptionsExt'`

```rust
// AVANT (compile error sur Windows)
use std::os::unix::fs::OpenOptionsExt;
// ...
.mode(0o600)

// APRES
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

## Patch 6 -- Auth : accepter le préfixe `kleos_` dans normalize_key

**Fichier :** `kleos-lib/src/auth.rs`
**Statut upstream :** Absent.
**Symptôme si absent :** GUI et kleos-cli retournent "Invalid API Key" quand la clé commence par `kleos_` (ex: `kleos_ca...`). Les clés `engram_` et `eg_` fonctionnent.

**Pourquoi :** `normalize_key()` n'accepte que `engram_` et `eg_` comme préfixes valides. Les clés générées pendant la période de rebrand ou saisies avec le préfixe `kleos_` sont rejetées avant même la vérification en base. Le hex sous-jacent est identique -- seul le préfixe textuel change.

```rust
// AVANT
fn normalize_key(raw_key: &str) -> Option<String> {
    let hex_portion = if let Some(rest) = raw_key.strip_prefix("engram_") {
        rest
    } else {
        raw_key.strip_prefix("eg_")?
    };

// APRES
fn normalize_key(raw_key: &str) -> Option<String> {
    let hex_portion = if let Some(rest) = raw_key.strip_prefix("engram_") {
        rest
    } else if let Some(rest) = raw_key.strip_prefix("kleos_") {
        rest
    } else {
        raw_key.strip_prefix("eg_")?
    };
```

**Note :** Le lookup DB utilise `key_prefix = hex[0..8]` -- pas le préfixe textuel. Un même secret peut donc être présenté indifféremment comme `engram_<hex>` ou `kleos_<hex>`.

---

## Procédure de re-application après un merge upstream

1. Vérifier si upstream a intégré le patch (souvent : non) :
   ```bash
   git show origin/main:agent-forge/Cargo.toml | grep "cfg(windows)"
   git show origin/main:kleos-server/src/main.rs | grep "EMBEDDING_BACKEND"
   ```

2. Si absent : le patch est à re-appliquer. Référencer ce fichier pour le contenu exact.

3. Après re-application : `cargo check -p kleos-server -p kleos-sh -p agent-forge -p kleos-cred -p kleos-sidecar`

4. Mettre à jour la date et le statut de chaque patch dans ce fichier.

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
