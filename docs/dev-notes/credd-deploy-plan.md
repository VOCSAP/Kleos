# kleos-credd Deploy Plan -- Windows Client + Linux Daemon

**Date :** 2026-05-12
**Objectif :** Déployer kleos-credd sur LXC 121 et configurer kleos-sh Windows pour
résoudre son bearer via credd (eliminer KLEOS_API_KEY en clair sur le poste).

**Infrastructure cible :**
- LXC 121 -- 192.168.10.21 -- Debian, déjà kleos-server sur port 4200
- Poste Windows -- Claude Code + kleos-sh

---

## Statut global

| Phase | Statut | Notes |
|-------|--------|-------|
| Phase 0 -- Clarifications préalables | [x] RESOLU | |
| Phase 1 -- Build Linux | [x] FAIT | glibc, libpcsclite1 requis sur LXC |
| Phase 2 -- Déploiement LXC | [x] FAIT | binaires dans /usr/local/bin/ |
| Phase 3 -- Bootstrap vault | [x] FAIT | vault init, bootstrap.enc créé, agent key générée |
| Phase 4 -- Systemd + config serveur | [x] FAIT | kleos-credd.service actif |
| Phase 5 -- Sécurité réseau | [ ] EN ATTENTE | iptables LXC pas encore fait |
| Phase 6 -- Configuration Windows | [x] FAIT | CREDD_BIND + CREDD_AGENT_KEY posés |
| Phase 7 -- Validation end-to-end | [~] PARTIEL | 7.1/7.2/7.3 OK -- 7.4 kleos-sh gate à valider |

---

## Phase 0 -- Clarifications (RESOLUES)

### [x] C0.1 -- Commandes bootstrap.enc

Séquence exacte (source : `kleos-cred/src/bin/cred.rs`) :

```bash
# Etape 1 : stocker le bearer Kleos dans le vault (interactif -- saisir la valeur à l'invite)
CRED_AUTH_MODE=keyfile \
CRED_KEYFILE=/etc/kleos/cred-master.key \
cred store kleos bearer -t api-key
# Prompt : entrer $KLEOS_API_KEY

# Etape 2 : chiffrer le bearer dans bootstrap.enc
CRED_AUTH_MODE=keyfile \
CRED_KEYFILE=/etc/kleos/cred-master.key \
cred bootstrap wrap kleos bearer
# Produit : ~/.config/cred/bootstrap.enc (mode 0600)
# credd charge ce fichier automatiquement au démarrage
```

Emplacement fixe : `~/.config/cred/bootstrap.enc` (hardcodé, pas de var d'env).

### [x] C0.2 -- Build glibc (décision)

Build **glibc** pour cette itération (pragmatique, évite le problème libpcsclite musl).
Debian 12 sur LXC = glibc 2.36 -- compatible si buildé sur WSL Debian 12.

---

## Phase 1 -- Build Linux (depuis WSL Debian 12)

### [ ] 1.1 -- Préparer l'environnement de build WSL

```bash
# Dans WSL
sudo apt-get install -y libpcsclite-dev pkg-config libssl-dev
```

### [ ] 1.2 -- Compiler les binaires

```bash
cd /mnt/c/Users/Olivier/workspace/claude-experiment/Kleos

cargo build --release \
  -p kleos-credd \
  -p kleos-cred

# Binaires produits :
#   target/release/kleos-credd
#   target/release/cred
#   target/release/derive-db-key
```

### [ ] 1.3 -- Vérifier les binaires

```bash
file target/release/kleos-credd    # ELF 64-bit x86-64
ldd target/release/kleos-credd     # lister les dépendances dynamiques
target/release/kleos-credd --help
target/release/cred --help
```

---

## Phase 2 -- Déploiement sur LXC 121

### [ ] 2.1 -- Copier les binaires

```bash
# Depuis Windows (Git Bash)
scp target/release/kleos-credd   root@192.168.10.21:/usr/local/bin/
scp target/release/cred          root@192.168.10.21:/usr/local/bin/
scp target/release/derive-db-key root@192.168.10.21:/usr/local/bin/

ssh root@192.168.10.21 \
  "chmod +x /usr/local/bin/kleos-credd /usr/local/bin/cred /usr/local/bin/derive-db-key"
```

### [ ] 2.2 -- Créer les répertoires (si absents)

```bash
ssh root@192.168.10.21 "mkdir -p /etc/kleos /var/lib/kleos"
```

### [ ] 2.3 -- Vérifier les dépendances dynamiques sur le LXC

```bash
ssh root@192.168.10.21 "/usr/local/bin/kleos-credd --help"
# Si erreur 'libpcsclite.so not found' :
ssh root@192.168.10.21 "apt-get install -y libpcsclite1"
```

---

## Phase 3 -- Bootstrap du vault

### [ ] 3.1 -- Générer la master keyfile

```bash
ssh root@192.168.10.21 "
  derive-db-key > /etc/kleos/cred-master.key
  chmod 600 /etc/kleos/cred-master.key
  cat /etc/kleos/cred-master.key   # vérifier : 64 chars hex
"
```

### [ ] 3.2 -- Initialiser le vault

```bash
ssh root@192.168.10.21 "
  CRED_AUTH_MODE=keyfile \
  CRED_KEYFILE=/etc/kleos/cred-master.key \
  cred init
"
```

### [ ] 3.3 -- Stocker le bearer Kleos + créer bootstrap.enc

```bash
# Se connecter en SSH interactif pour la saisie du bearer
ssh root@192.168.10.21

# Sur le LXC :
CRED_AUTH_MODE=keyfile \
CRED_KEYFILE=/etc/kleos/cred-master.key \
cred store kleos bearer -t api-key
# Saisir à l'invite : $KLEOS_API_KEY

CRED_AUTH_MODE=keyfile \
CRED_KEYFILE=/etc/kleos/cred-master.key \
cred bootstrap wrap kleos bearer
# Vérifier : ls -la ~/.config/cred/bootstrap.enc  -> doit exister, mode 600
```

### [ ] 3.4 -- Créer l'agent key pour kleos-sh Windows

```bash
ssh root@192.168.10.21 "
  CRED_AUTH_MODE=keyfile \
  CRED_KEYFILE=/etc/kleos/cred-master.key \
  cred agent-key generate kleos-sh-windows --scope bootstrap/kleos-sh
"
# NOTER le token retourné -> CREDD_AGENT_KEY sur Windows (Phase 6)
```

### [ ] 3.4bis -- Pousser l'entree CRED:v3 dans kleos-server (ETAPE OBLIGATOIRE)

> **Pourquoi cette étape existe :** Le endpoint `/bootstrap/kleos-bearer?agent=<slot>` de
> credd ne lit PAS depuis sa DB locale. Il appelle `GET {KLEOS_URL}/list?category=credential`
> sur kleos-server et cherche `[CRED:v3] engram-rust/<slot> = <hex>`. Sans cette entree,
> la resolution retourne 404.
>
> **Pourquoi le bypass Python et pas `POST /secret` via credd :** `POST /secret` via credd
> (port 4400) provoque un timeout 408 (30s) sur LXC sans YubiKey -- credd tente
> `yubikey::YubiKey::open()` via PCSC avant chaque appel HTTP vers kleos-server.
> Voir `docs/dev-notes/credd-todo.md` TODO-1 pour le fix.

```bash
# Sur le LXC -- installer python3-cryptography si absent
apt-get install -y python3-cryptography

# Lire la master key
MASTER_KEY=$(cat /etc/kleos/cred-master.key)

# Chiffrer le bearer et creer l'entree dans kleos-server
# Remplacer BEARER par le bearer kleos de l'agent (ici meme bearer que bootstrap)
BEARER="$KLEOS_API_KEY"
AGENT_SLOT="kleos-sh"   # doit correspondre au ?agent= utilise par le client

HEX_DATA=$(python3 -c "
import json,os
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
mk=bytes.fromhex('$MASTER_KEY')
pt=json.dumps({'type':'api_key','key':'$BEARER'},separators=(',',':')).encode()
n=os.urandom(12)
print((n+AESGCM(mk).encrypt(n,pt,None)).hex())
")

curl -s -X POST http://192.168.10.21:4200/store \
  -H "Authorization: Bearer $BEARER" \
  -H 'Content-Type: application/json' \
  -d "{\"content\":\"[CRED:v3] engram-rust/$AGENT_SLOT = $HEX_DATA\",\"category\":\"credential\",\"source\":\"credd\",\"importance\":10,\"is_static\":true}"

# Attendu : {"created_at":"...","id":NNN,"stored":true,...}
```

> **Verification :** depuis Windows apres Phase 6 :
> ```powershell
> curl.exe -H "Authorization: Bearer $env:CREDD_AGENT_KEY" `
>   "http://192.168.10.21:4400/bootstrap/kleos-bearer?agent=kleos-sh"
> # Attendu : {"key":"kleos_ca...","expires_at":"...","ttl_secs":3600}
> ```

> **Repetition :** a refaire pour chaque nouvel agent (`AGENT_SLOT` different).
> L'entree est persistante dans kleos-server -- pas besoin de la recreer sauf
> si kleos-server est reinitialise.

---

## Phase 4 -- Configuration serveur et systemd

### [x] 4.1 -- Fichier d'environnement credd (/etc/kleos/credd.env)

> Fichier créé manuellement. Utiliser `KLEOS_*` (pas `ENGRAM_*`) -- `migrate_env_prefix()`
> traduit automatiquement au démarrage. Les deux fonctionnent, KLEOS_ est cohérent.

```
CREDD_LISTEN=192.168.10.21:4400
CREDD_DB_PATH=/var/lib/kleos/cred.db
CREDD_AUTH_MODE=keyfile
CREDD_KEYFILE=/etc/kleos/cred-master.key
KLEOS_ENCRYPTION_MODE=env
KLEOS_DB_KEY=<générer avec : openssl rand -hex 32>
```

`CREDD_LISTEN` peut aussi être mis dans `kleos.env` si un seul fichier est préféré --
kleos-server ignore les vars `CREDD_*`. Les deux approches sont valides.

### [x] 4.2 -- Unité systemd (/etc/systemd/system/kleos-credd.service)

> Fichier créé manuellement.

```ini
[Unit]
Description=Kleos Credential Daemon
After=network.target

[Service]
EnvironmentFile=/etc/kleos/credd.env
ExecStart=/usr/local/bin/kleos-credd
Restart=on-failure
RestartSec=5
User=root

[Install]
WantedBy=multi-user.target
```

### [ ] 4.3 -- Activer et démarrer

```bash
ssh root@192.168.10.21 "
  systemctl daemon-reload
  systemctl enable kleos-credd
  systemctl start kleos-credd
  systemctl status kleos-credd
"
```

### [ ] 4.4 -- Vérifier les logs de démarrage

```bash
ssh root@192.168.10.21 "journalctl -u kleos-credd -n 50"
# Chercher :
#   "credd listening on tcp:192.168.10.21:4400"
#   "credd: master key loaded from keyfile"
#   Pas d'erreur sur bootstrap.enc (warning normal si absent avant Phase 3.3)
```

---

## Phase 5 -- Sécurité réseau

### Note topologie OPNSense

Si Windows (192.168.10.X) et LXC 121 sont sur le **même segment LAN** (même bridge
Proxmox), le trafic entre eux ne passe pas par OPNSense -- il reste sur le switch
virtuel. Dans ce cas, OPNSense ne peut pas filtrer ce trafic.

**Défense primaire : iptables sur le LXC lui-même** (toujours efficace).
**Défense secondaire : règle OPNSense** (utile seulement si trafic inter-VLAN).

### [ ] 5.1 -- Firewall sur le LXC (défense primaire)

```bash
ssh root@192.168.10.21 "
  # Remplacer 192.168.10.X par l'IP du poste Windows
  iptables -A INPUT -p tcp --dport 4400 -s 192.168.10.X -j ACCEPT
  iptables -A INPUT -p tcp --dport 4400 -j DROP

  # Persister les règles
  apt-get install -y iptables-persistent
  iptables-save > /etc/iptables/rules.v4
"
```

### [ ] 5.2 -- Règle OPNSense (défense secondaire, si trafic inter-VLAN)

Dans Firewall > Rules > interface LAN concernée, ajouter dans cet ordre :

| Priorité | Action | Proto | Source | Destination | Port | Description |
|----------|--------|-------|--------|-------------|------|-------------|
| 1 | Pass | TCP | `192.168.10.X` (Windows) | `192.168.10.21` | `4400` | Allow Windows to credd |
| 2 | Block | TCP | any | `192.168.10.21` | `4400` | Block all others to credd |

Les règles OPNSense s'évaluent de haut en bas -- Pass doit être avant Block.

### [ ] 5.3 -- Vérifier les permissions des fichiers secrets

```bash
ssh root@192.168.10.21 "
  ls -la /etc/kleos/cred-master.key    # 600 root:root
  ls -la /etc/kleos/credd.env          # 600 root:root
  ls -la ~/.config/cred/bootstrap.enc  # 600 root:root
  ls -la /var/lib/kleos/cred.db        # 600 root:root (après init)
"
```

---

## Phase 6 -- Configuration Windows

### [ ] 6.1 -- Ajouter les variables d'environnement système

System Properties > Advanced > Environment Variables > System variables :

```
CREDD_BIND=192.168.10.21:4400
CREDD_AGENT_KEY=<token de l'étape 3.4>
```

`KLEOS_API_KEY` reste en place comme fallback pendant la validation.
kleos-sh essaie credd en premier, tombe sur `KLEOS_API_KEY` si credd est inaccessible.

### [ ] 6.2 -- Redémarrer Claude Code

Pour que les nouvelles variables d'environnement soient prises en compte.

---

## Phase 7 -- Validation end-to-end

### [ ] 7.1 -- Connectivité réseau

```powershell
# Depuis Windows (PowerShell)
Test-NetConnection -ComputerName 192.168.10.21 -Port 4400
# Attendu : TcpTestSucceeded : True
```

### [ ] 7.2 -- Health check credd

```bash
curl http://192.168.10.21:4400/health
# Attendu : 200 OK
```

### [x] 7.3 -- Résolution bearer via credd (VALIDE 2026-05-12)

Résultat obtenu :
```json
{"key":"$KLEOS_API_KEY","expires_at":"2026-05-12T16:59:24.401052257+00:00","ttl_secs":3600}
```

**Blocages rencontrés et résolus :**
1. `CREDD_AGENT_KEY` vide dans la session PowerShell -> ouvrir une nouvelle session après avoir posé la var système
2. `KLEOS_URL` manquant dans credd.env -> ajouté `KLEOS_URL=http://192.168.10.21:4200`
3. Entree `[CRED:v3] engram-rust/kleos-sh` absente de kleos-server -> etape manquante dans le plan (Phase 3.4bis ci-dessous)

**Phase 3.4bis manquante -- A ajouter au plan pour tout nouvel agent :**

```bash
# Creer l'entree chiffree dans kleos-server (bypass credd -- timeout YubiKey, voir credd-todo.md)
HEX_DATA=$(python3 -c "import json,os; from cryptography.hazmat.primitives.ciphers.aead import AESGCM; mk=bytes.fromhex('MASTER_KEY_64HEX'); pt=json.dumps({'type':'api_key','key':'BEARER'},separators=(',',':')).encode(); n=os.urandom(12); print((n+AESGCM(mk).encrypt(n,pt,None)).hex())")

curl -s -X POST http://192.168.10.21:4200/store \
  -H "Authorization: Bearer <kleos_bearer>" \
  -H 'Content-Type: application/json' \
  -d "{\"content\":\"[CRED:v3] engram-rust/<agent_name> = $HEX_DATA\",\"category\":\"credential\",\"source\":\"credd\",\"importance\":10,\"is_static\":true}"
```

> Note : `POST /secret` via credd (port 4400) time out a 30s a cause de `yubikey::YubiKey::open()`
> sur LXC sans YubiKey. Fix pending dans `docs/dev-notes/credd-todo.md` TODO-1.

### [ ] 7.4 -- Test kleos-sh gate check complet

```powershell
$env:RUST_LOG="kleos_sh=debug"
kleos-sh.exe -c "echo test"
# Chercher dans stderr : "resolved bearer via credd"
```

### [ ] 7.5 -- Retirer KLEOS_API_KEY (après validation complète)

Une fois le flow credd confirmé, supprimer `KLEOS_API_KEY` des env vars Windows.

---

## Référence -- Variables d'environnement complètes

### LXC 121 (/etc/kleos/credd.env)

| Variable | Valeur | Notes |
|----------|--------|-------|
| `CREDD_LISTEN` | `192.168.10.21:4400` | Bind LAN uniquement (pas 0.0.0.0) |
| `CREDD_DB_PATH` | `/var/lib/kleos/cred.db` | Séparé de kleos.db |
| `CREDD_AUTH_MODE` | `keyfile` | Pas de YubiKey sur le LXC |
| `CREDD_KEYFILE` | `/etc/kleos/cred-master.key` | 600 root:root |
| `KLEOS_ENCRYPTION_MODE` | `env` | Chiffrement at-rest DB credd |
| `KLEOS_DB_KEY` | `<openssl rand -hex 32>` | Clé séparée de kleos-server |

### Poste Windows (env vars système)

| Variable | Valeur | Notes |
|----------|--------|-------|
| `CREDD_BIND` | `192.168.10.21:4400` | Adresse TCP credd |
| `CREDD_AGENT_KEY` | `<token Phase 3.4>` | Auth kleos-sh -> credd |
| `KLEOS_API_KEY` | `kleos_ca...` | Fallback -- retirer après Phase 7.5 |
| `KLEOS_URL` | `http://192.168.10.21:4200` | Inchangé |

---

## Rollback

credd tombant n'impacte pas kleos-server. kleos-sh retombe sur `KLEOS_API_KEY`.

```bash
ssh root@192.168.10.21 "systemctl stop kleos-credd"
```
