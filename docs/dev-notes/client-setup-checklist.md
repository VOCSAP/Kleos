# Checklist -- Setup Client Windows post-déploiement v1.0.0

**Créé :** 2026-05-11
**Statut global :** En cours

Légende : [ ] à faire | [x] terminé | [~] partiel / bloqué | [!] bug ouvert

---

## 1. Hooks Claude Code + CLAUDE.md

### 1.a. Vérifier que CLAUDE.md repo est chargé par Claude Code

- [ ] Confirmer que `Kleos/CLAUDE.md` est bien lu au démarrage de session
      (vérifier dans les premières lignes de contexte Claude Code)

### 1.b. Hooks existants -- audit

- [ ] `session-start-kleos.sh` : démarre le sidecar, charge contexte Eidolon/Kleos
- [ ] `session-end-kleos.sh` : enregistre fin de session
- [ ] `user-prompt-lean.sh` : injecte règles obligatoires
- [ ] `rtk-rewrite.sh` : rewrite commandes Bash
- [ ] `enforce-kleos-search.sh` : force recherche Kleos avant actions
- [ ] `git-commit-guard.py` : guard commits
- [ ] `file-write-scanner.py` : scan fichiers écrits
- [ ] `track-agent-forge.sh` : suivi agent-forge
- [ ] `post-tool-kleos-prompt.sh` : prompt Kleos post-erreur Bash

### 1.c. Câbler kleos-sh comme hook PreToolUse Bash

- [ ] Ajouter dans `settings.json` > `PreToolUse` > matcher `Bash` :
      ```json
      {
        "type": "command",
        "command": "kleos-sh.exe --claude-hook",
        "timeout": 6
      }
      ```
- [ ] Tester : lancer une commande Bash dans Claude Code et vérifier que
      kleos-sh contacte le serveur (logs kleos-server ou RUST_LOG=debug)

### 1.d. eidolon-supervisor -- démarrage automatique

- [ ] **Option A (hook SessionStart -- recommandée)** : ajouter dans
      `session-start-kleos.sh` le même pattern que pour kleos-sidecar :
      ```bash
      if ! pgrep -x eidolon-supervisor.exe > /dev/null 2>&1; then
        nohup eidolon-supervisor.exe \
          --watch-dir "$USERPROFILE/.claude/projects" \
          >> "$LOG_DIR/eidolon-supervisor.log" 2>&1 &
        disown
      fi
      ```
- [ ] **Option B (Task Scheduler Windows)** : tâche planifiée au login utilisateur
- [ ] Choisir entre A et B, implémenter
- [ ] Vérifier que eidolon-supervisor est bien lancé et surveille les sessions

---

## 2. Vérification des binaires côté client

### 2.a. kr / ke / kw (kleos-fs)

- [ ] `kr <fichier>` : lit un fichier -- vérifier sortie normale sur petit fichier
- [ ] `kr <gros-fichier.rs>` : vérifier délégation à agent-forge (log "agent-forge fallback" attendu si agent-forge absent du PATH)
- [ ] `kw <fichier>` (stdin) : écrire un fichier de test dans CWD
- [ ] `kw` hors de `KLEOS_FS_ALLOWED_ROOTS` : doit refuser
- [ ] `ke <fichier>` : vérifier comportement (gate edit)
- [ ] Configurer `KLEOS_FS_ALLOWED_ROOTS` si nécessaire

### 2.b. kleos-sidecar

- [ ] Vérifier que le hook session-start lance bien le sidecar sur port 7711
      ```bash
      curl -sf http://127.0.0.1:7711/health
      ```
- [ ] Vérifier que `KLEOS_SIDECAR_URL=http://127.0.0.1:7711` est bien dans l'env
- [ ] Vérifier que kleos-sh envoie les observations post-tool au sidecar
      (chercher dans les logs sidecar `~/.claude/logs/kleos-sidecar.log`)
- [ ] Tester la compression contexte : lancer une session, vérifier les logs
      pour `compress` ou `ollama_probe`

### 2.c. kleos-credd + kleos-cred (cred)

- [ ] Décider du mode d'auth credd : `password` ou `keyfile`
      (pas de YubiKey sur ce poste Windows)
- [ ] Si `keyfile` : générer une clé master
      ```bash
      openssl rand -hex 32 > %APPDATA%\cred\master.key
      ```
- [ ] Démarrer kleos-credd avec `CREDD_AUTH_MODE=keyfile CREDD_KEYFILE=...`
- [ ] Tester `cred get kleos api-key-claude --raw`
- [ ] Vérifier que kleos-sh peut récupérer le token via credd
      (retirer KLEOS_API_KEY de l'env et tester que la gate fonctionne encore)
- [ ] Note : **optionnel si KLEOS_API_KEY reste dans l'env systeme**

### 2.d. eidolon-supervisor

- [ ] Lancer manuellement et vérifier démarrage propre :
      ```bash
      eidolon-supervisor.exe --watch-dir %USERPROFILE%\.claude\projects
      ```
- [ ] Vérifier dans les logs que les règles sont chargées
- [ ] Déclencher une règle test (ex : commande git push --force dans une session)
      et vérifier qu'une alerte part vers kleos-server `/activity`
- [ ] Implémenter le démarrage automatique (voir 1.d)

### 2.e. agent-forge

- [ ] Vérifier `agent-forge stats` -- doit créer/ouvrir la DB locale
      (`~/.local/share/agent-forge/forge.db` ou chemin équivalent Windows)
- [ ] Vérifier que `track-agent-forge.sh` détecte bien l'utilisation
- [ ] Tester `agent-forge spec-task` sur une tâche factice

---

## 3. MCP

### 3.a. Configurer kleos-mcp dans claude_desktop_config.json / .mcp.json

- [ ] Vérifier quel fichier de config MCP est actif
      (`~/.claude.json`, `%APPDATA%\Claude\claude_desktop_config.json`, ou `.mcp.json` projet)
- [ ] Ajouter la config kleos-mcp (transport SSH stdio vers LXC 121) :
      ```json
      {
        "mcpServers": {
          "kleos": {
            "command": "ssh",
            "args": ["root@192.168.10.21", "kleos-mcp"],
            "env": {
              "KLEOS_API_KEY": "<token>"
            }
          }
        }
      }
      ```
- [ ] Redémarrer Claude Code après modification

### 3.b. Vérifier fonctionnement MCP

- [ ] Vérifier que Claude Code liste bien le serveur MCP kleos au démarrage
      (aucune erreur de connexion SSH dans les logs Claude Code)
- [ ] Tester un tool MCP : `mcp__kleos__search` ou équivalent
- [ ] Vérifier que les outils kleos-mcp apparaissent bien dans la liste des tools

---

## Bugs ouverts

### [!] GUI Kleos -- "Invalid API Key"

**Statut :** Patch 6 appliqué (normalize_key accepte kleos_) mais insuffisant.

**Hypothèse principale :** le hash en base a été calculé depuis la forme
canonique `kleos_<hex>` (si la clé a été générée par une version patchée du
serveur), mais normalize_key renvoie `engram_<hex>` pour le recalcul du hash
-- mismatch.

**Debug à faire :**
```sql
-- Sur LXC 121 (sqlite3 /var/lib/kleos/kleos.db) :
SELECT key_prefix, hash_version, name, scopes FROM api_keys;
```
Comparer `key_prefix` (8 premiers chars hex) avec les 8 premiers chars du
hex dans la clé `kleos_ca...` côté client. Si les préfixes correspondent mais
le hash échoue, la forme canonique de hachage est différente.

**Solution probable :** soit corriger la forme canonique retournée par
normalize_key pour les clés `kleos_`, soit régénérer une nouvelle clé via
`kleos-cli admin api-key create` et mettre à jour les env.

---

## Notes d'environnement

| Variable | Valeur | Où |
|---|---|---|
| `KLEOS_URL` | `http://192.168.10.21:4200` | Env Windows système |
| `KLEOS_API_KEY` | `kleos_ca...` | Env Windows système |
| `KLEOS_SIDECAR_URL` | `http://127.0.0.1:7711` | Env Windows système -- fait |
| `OLLAMA_URL` | `http://192.168.10.16:11434/v1/chat/completions` | Env Windows |
| `OLLAMA_MODEL` | `qwen3.5:4b` | Env Windows |
| `KLEOS_SIDECAR_TOKEN` | `<token>` | Env Windows |
| `KLEOS_FS_ALLOWED_ROOTS` | à définir | Non configuré |
| `CREDD_AUTH_MODE` | à définir | Non configuré |
