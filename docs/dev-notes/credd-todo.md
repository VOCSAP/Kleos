# kleos-credd -- TODOs et bugs connus

**Date :** 2026-05-12

---

## TODO-1 -- Timeout 408 sur POST /secret quand pas de YubiKey (PRIORITE HAUTE)

### Symptome

`POST http://192.168.10.21:4400/secret/engram-rust/kleos-sh` retourne **408 Request Timeout**
apres exactement 30 secondes (= CREDD_REQUEST_TIMEOUT_SECS dans `kleos-credd/src/lib.rs`).

### Cause racine identifiee

Dans `kleos-credd/src/state.rs`, `init_kleos_signer()` appelle
`RequestSigner::from_env_or_file(...)` qui est defini dans `kleos-lib/src/auth_piv.rs`.

Extrait pertinent (`auth_piv.rs`, fonction `from_env_or_file`) :

```rust
// T1: Try PIV YubiKey first (highest auth tier)
#[cfg(feature = "piv")]
{
    match Self::from_yubikey(host, agent, model) {
        Ok(signer) => return Ok(Some(signer)),
        Err(e) => {
            tracing::debug!("PIV YubiKey not available, falling back: {e}");
        }
    }
}

// T2: Software Ed25519 key from env var or file
if let Ok(hex_key) = std::env::var("KLEOS_IDENTITY_KEY") { ... }
```

`from_yubikey` appelle `yubikey::YubiKey::open()` qui tente d'initialiser le daemon
PCSC. Sur un LXC sans YubiKey branchee, `pcscd` peut soit :
- Retourner une erreur immediate (si pas installe) -- pas de probleme
- Attendre le timeout PCSC par defaut (~20-30s) -- cause le 408

Ensuite, si `kleos_signer` est `Some(Ed25519 ou Piv)`, chaque appel HTTP vers
kleos-server dans `kleos_sync::apply_auth` passe par `signer.sign_request(...)`.
Si le signer est PIV, `sign_request` appelle `piv_verify_and_sign` qui re-tente
le YubiKey -- nouveau timeout.

### Impact

Le `store_handler` (`POST /secret/{category}/{name}`) appelle `store_to_kleos`
en fin de handler. Comme `store_to_kleos` est `.await`ed et que `apply_auth`
bloque sur le timeout PCSC, le handler entier depasse 30s => 408.

Le `/bootstrap/kleos-bearer` n'est pas affecte car il n'appelle pas `apply_auth`
directement pour la partie GET (il utilise `bootstrap_master` directement via
`reqwest::Client::new()` + `header("Authorization", ...)`).

### Fix recommande

**Option A (simple) :** Generer une cle Ed25519 software et la mettre dans credd.env.
Cela court-circuite le T1 (YubiKey) car T2 est tente avant de faire appel au PIV.

```bash
# Sur le LXC -- generer une cle Ed25519 (32 bytes hex)
python3 -c "import os; print(os.urandom(32).hex())"
# Ou avec OpenSSL :
openssl rand -hex 32
```

Ajouter dans `/etc/kleos/credd.env` :
```
KLEOS_IDENTITY_KEY=<hex_32_bytes>
```

Et enregistrer la cle publique correspondante dans kleos-server (voir doc upstream
sur la procedure `KLEOS_IDENTITY_KEY`). Attention : cette cle donne acces signe
a kleos-server -- proteger comme un secret.

**Option B (propre) :** Modifier `from_env_or_file` pour tester PCSC availability
avant `from_yubikey`, ou ajouter un flag `KLEOS_NO_PIV=1` pour sauter le T1.
Necessite une PR upstream ou un patch local.

**Option C (workaround immediat) :** Si seule la route `bootstrap_master` est
necessaire (pas de `POST /secret` depuis credd), aucun fix urgent -- le
`/bootstrap/kleos-bearer` fonctionne deja sans passer par `sign_request`.

### Etat actuel

L'entree `[CRED:v3] engram-rust/kleos-sh` a ete creee directement dans
kleos-server via `POST /store` (bypass credd) -- voir TODO-2.
`POST /secret` via credd reste en timeout mais n'est pas necessaire pour
le flow nominal `kleos-sh -> credd -> kleos-server`.

---

## TODO-2 -- Etape manquante dans le plan deploy : stocker l'entree CRED:v3

### Symptome

`GET /bootstrap/kleos-bearer?agent=kleos-sh` retourne 404 :
`{"error":"agent bearer not found: kleos-sh"}`.

### Cause racine

Le code dans `bootstrap_bearer.rs` (fonction `get_bootstrap_kleos_bearer`) :
1. Utilise le `bootstrap_master` (bearer charge depuis `bootstrap.enc`) pour
   s'authentifier sur kleos-server
2. Appelle `GET {KLEOS_URL}/list?category=credential&limit=500`
3. Recherche une entree avec le prefixe `[CRED:v3] engram-rust/kleos-sh = `

Cette entree n'est PAS creee automatiquement. Elle doit etre stockee dans
kleos-server manuellement lors de la configuration initiale.

Le plan deploy original (Phase 3.4) ne couvrait que la generation de l'agent key,
pas la creation de l'entree kleos-server correspondante.

### Format de l'entree

```
content  : "[CRED:v3] engram-rust/kleos-sh = <hex(nonce||ciphertext+tag)>"
category : "credential"
```

Le hex est : AES-256-GCM de `{"type":"api_key","key":"<bearer>"}` avec la
master key (format `nonce(12B) || ciphertext || tag(16B)` -- defini dans
`kleos-cred/src/crypto.rs`, fonction `encrypt`).

### Creation directe (bypass credd)

Puisque `POST /secret` via credd est en timeout (TODO-1), l'entree est creee
directement via `POST /store` sur kleos-server :

```bash
HEX_DATA=$(python3 -c "import json,os; from cryptography.hazmat.primitives.ciphers.aead import AESGCM; mk=bytes.fromhex('MASTER_KEY_HEX'); pt=json.dumps({'type':'api_key','key':'BEARER'},separators=(',',':')).encode(); n=os.urandom(12); print((n+AESGCM(mk).encrypt(n,pt,None)).hex())")

curl -X POST http://192.168.10.21:4200/store \
  -H "Authorization: Bearer $KLEOS_API_KEY" \
  -H 'Content-Type: application/json' \
  -d "{\"content\":\"[CRED:v3] engram-rust/kleos-sh = $HEX_DATA\",\"category\":\"credential\",\"source\":\"credd\",\"importance\":10,\"is_static\":true}"
```

### A ajouter dans le plan deploy (Phase 3.4bis)

Voir `credd-deploy-plan.md` -- section Phase 3 a completer.

---

## TODO-3 -- cred bootstrap unwrap echoue sans KLEOS_DB_KEY

### Symptome

```
Error: failed to open database
Caused by: file is not a database
```

### Cause

La DB cred (`/root/.config/cred/cred.db`) est chiffree (SQLCipher,
`KLEOS_ENCRYPTION_MODE=env`). Lancer `cred` sans `KLEOS_DB_KEY` dans
l'environnement fait echouer l'ouverture.

### Fix

Toujours prefixer les commandes `cred` avec les variables de chiffrement :

```bash
KLEOS_ENCRYPTION_MODE=env KLEOS_DB_KEY=<valeur_dans_credd.env> \
  CRED_AUTH_MODE=keyfile CRED_KEYFILE=/etc/kleos/cred-master.key \
  cred <sous-commande>
```

---

## Contexte infrastructure

- LXC 121 : 192.168.10.21
- credd port : 4400
- kleos-server port : 4200
- cred DB : /root/.config/cred/cred.db (SQLCipher, KLEOS_DB_KEY dans credd.env)
- master key : /etc/kleos/cred-master.key (64 hex chars)
- bootstrap.enc : /root/.config/cred/bootstrap.enc
- agent-keys.json : /root/.config/cred/agent-keys.json
- credd.env : /etc/kleos/credd.env
