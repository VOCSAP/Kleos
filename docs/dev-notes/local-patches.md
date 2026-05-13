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
kleos-lib = { path = "../kleos-lib", version = "1.0.0", features = ["sqlcipher"] }
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

## Procédure de re-application après un merge upstream

1. Vérifier si upstream a intégré le patch (souvent : non) :
   ```bash
   # Patches Windows (1-4)
   git show origin/main:agent-forge/Cargo.toml | grep "cfg(windows)"
   git show origin/main:kleos-sidecar/Cargo.toml | grep "cfg(windows)"
   git show origin/main:kleos-approval-tui/Cargo.toml | grep "cfg(windows)"
   git show origin/main:kleos-sh/src/main.rs | grep "cfg(not(unix))"
   git show origin/main:kleos-cred/src/bin/derive-db-key.rs | grep "cfg(unix)"
   # Patch 5 -- embedding backend
   git show origin/main:kleos-server/src/main.rs | grep "EMBEDDING_BACKEND"
   # Patch 7 -- auth kleos_ prefix
   git show origin/main:kleos-lib/src/auth.rs | grep "kleos_\|split_once"
   # Patch 8A -- SPA prefix routing
   git show origin/main:kleos-server/src/routes/gui/mod.rs | grep "starts_with.*spa"
   # Patch 9A -- connect_timeout configurable
   git show origin/main:kleos-sh/src/main.rs | grep "KLEOS_SH_CONNECT_TIMEOUT_SECS"
   # Patch 9B -- exec.rs Windows shell
   git show origin/main:kleos-sh/src/exec.rs | grep "cfg(not(unix))"
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
