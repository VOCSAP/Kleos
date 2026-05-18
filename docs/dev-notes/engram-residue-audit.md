# Audit "engram" résidus -- Kleos VOCSAP fork

**Date :** 2026-05-18 (post-rebase v1.1.5)
**Déclencheur :** Bug Broca `ask` retourne vide -- découverte que
`kleos-lib/src/activity.rs:261` hardcode `service="engram"` alors que
`kleos-lib/src/services/broca.rs:1209` filtre par `known_services` qui ne
contient pas `engram`. Audit étendu pour anticiper d'autres résidus similaires.

**Méthode :** `git grep -i -w '\bengram\b'` sur `*.rs`. 145 occurrences dans 44
fichiers. Classification manuelle ci-dessous.

---

## Catégorie 1 -- Hardcode comportemental (BUG, fix prioritaire)

Résidus qui affectent le comportement runtime, pas juste la doc/tests.

| Fichier:ligne | Code | Diagnostic | Action |
|---|---|---|---|
| `kleos-lib/src/activity.rs:261` | `service: Some("engram".to_string())` | Hardcode dans `fanout_broca`. Tout event POST /activity atterrit dans `broca_actions` avec `service="engram"` -> Broca `ask` ne match jamais. | **FIX appliqué local 2026-05-18** : `"engram"` -> `"kleos"`. |
| `kleos-lib/src/services/broca.rs:1209` | `known_services = [...]` ne contient pas `"engram"` | Ne match pas les rows existantes (avant le fix activity.rs). | **FIX appliqué local 2026-05-18** : ajout de `"engram"` comme alias rétro-compat ceinture+bretelles. |
| `kleos-lib/src/intelligence/growth.rs:424` | `service: "engram".to_string()` | Même pattern que activity.rs, dans le contexte `growth_observations`. Aucun endpoint ne filtre dessus -- mais c'est d'autant plus vicieux : invisible côté API, le résidu legacy persiste en interne sans symptôme. | **FIX appliqué local 2026-05-18** : `"engram"` -> `"kleos"`. |
| `kleos-lib/src/intelligence/growth.rs:91` | `"engram" => "You are Kleos's..."` | Match arm sur service name pour choisir le prompt système (self-reflection dreamer). Couplé à `growth.rs:424`. | **FIX appliqué local 2026-05-18** : arm devient `"engram" \| "kleos" => ...` (rétro-compat ceinture+bretelles, comme broca.rs). |
| `kleos-lib/src/intelligence/growth.rs:407` | `WHERE source = 'engram-growth'` (hardcode) | Query SQL filtre par source legacy `engram-growth`. **Important** : ligne 288 montre que le `source` est généré dynamiquement via `format!("{}-growth", req.service)` au write -- donc peut être `engram-growth`, `kleos-growth`, `claude-code-growth`, etc. Le hardcode rate tout sauf `engram-growth`. | **FIX appliqué local 2026-05-18** : `source LIKE '%-growth'` couvre tous les `<service>-growth` actuels et futurs. `category = 'growth'` borne déjà le scope -> pas de faux positif réaliste. |

---

## Catégorie 2 -- Path filesystem legacy (fallback compat, intentionnel)

Chemins config/data avec `engram` pour migrer depuis l'ancien upstream sans casser
les installs existantes. Comportement défensif : tentent `kleos.db` d'abord puis
`engram.db` en fallback.

| Fichier:ligne | Code | Diagnostic |
|---|---|---|
| `kleos-lib/src/config.rs:42, 46` | Fallback `engram.db` si `kleos.db` absent | Intentionnel. Doc `kleos.db not found -- falling back to legacy engram.db`. |
| `kleos-lib/src/config.rs:66` | `~/.config/engram/dbkey` | Path legacy lecture clé DB. |
| `kleos-lib/src/config.rs:635-656, 1111-1136` | Multiples joins `engram/config.toml`, `engram/models/...` | Fallback config dir. |
| `kleos-lib/src/encryption.rs:60, 95, 100` | `$XDG_CONFIG_HOME/engram/dbkey` | Lecture keyfile legacy. |
| `kleos-cred/src/yubikey.rs:266-343` | `~/.config/engram/challenge` | YubiKey challenge dir. |
| `kleos-lib/src/tenant/mod.rs:4` (doc) | `tenants/<id>/engram.db` | Doc legacy, le code peut écrire kleos.db ou engram.db selon les conditions. |
| `kleos-lib/src/db/pitr.rs` (21 occurrences) | `engram-backup-YYYYMMDD-HHMMSS.db` prefix | Backup files naming. Tout le code PITR (parse/list/find/restore) utilise ce prefix. **Si rebrand : casser la rétro-compat des backups existants.** |
| `kleos-lib/src/db/backup.rs:144-226` | `engram-src-*.db`, `engram-restore-*.db` etc. | Tests + naming snapshots. |

**Décision** : ne pas toucher cette catégorie. Risque > bénéfice (casser les
installs existantes). Si rebrand un jour, il faudrait une migration dédiée.

---

## Catégorie 3 -- Identifiants OpenTelemetry / tracing (cosmétique)

Noms de service pour distributed tracing. Pas d'impact fonctionnel, juste les
spans/metrics tagués sous l'ancien nom.

| Fichier:ligne | Code |
|---|---|
| `kleos-cli/src/main.rs:853` | `init_tracing("engram-cli", ...)` |
| `kleos-credd/src/main.rs:48` | `init_tracing("engram-credd", ...)` |
| `kleos-lib/src/observability.rs:1, 41` | Doc commentaires `engram-server` |

**Décision** : pas critique. Renommage propre nécessite coordination avec les
dashboards Grafana qui peuvent filtrer sur `service.name="engram-*"`.

---

## Catégorie 4 -- Test fixtures (no-op)

Tests qui utilisent `"engram"` comme valeur de fixture (project, service, agent).
Pas d'impact prod, n'affecte que les tests unitaires.

Fichiers concernés (non-exhaustif) :
- `kleos-lib/src/db/tenant_migrations.rs:1714, 1734, 1789, 1822, 2326, 2353`
- `kleos-lib/src/db/pool.rs:418, 448`
- `kleos-lib/src/db/backup.rs:144-226`
- `kleos-lib/src/auth.rs:697`
- `kleos-lib/src/encryption.rs:241, 254`
- `kleos-lib/src/intelligence/scheduler.rs:307`
- `kleos-cred/src/agent_keys_file.rs:359-364`
- `kleos-cred/src/bin/cred.rs:2877, 2901, 3002`
- `kleos-server/tests/api_parity.rs:3-5`
- `kleos-credd/tests/integration.rs:1`
- `kleos-credd/src/bootstrap.rs:159`
- `kleos-lib/benches/pitr_collect.rs:3, 22`
- `kleos-lib/examples/db_benchmark.rs:1-459`
- `kleos-lib/examples/pagerank_benchmark.rs:44`

**Décision** : ignore. Aucun fix nécessaire.

---

## Catégorie 5 -- Alias / rétro-compat intentionnels

Endroits où `engram` est explicitement préservé comme alias.

| Fichier:ligne | Code | Diagnostic |
|---|---|---|
| `kleos-lib/src/services/broca.rs:1213` (post-fix VOCSAP) | known_services inclut `"engram"` | Rétro-compat pour data déjà stockée. |
| `kleos-ingest/src/extractor.rs:148` | `services = ["kleos", "engram", ...]` | Liste services connus, alias intentionnel. |
| `kleos-lib/src/memory/auto_tag.rs:7` | `("engram", "engram")` mapping | Tag auto-attribution, rétro-compat. |
| `kleos-cli/src/main.rs:2730, 2739, 2740` | `engram-rust` category alias | Permet `kleos-cli cred exec engram-rust <slot>`. Légitime. |
| `kleos-credd/src/handlers/bootstrap_bearer.rs:15, 131, 170, 172, 298, 300` | `[CRED:v3] engram-rust/<agent>` legacy_prefix | Lit les secrets stockés sous l'ancien prefix. Intentionnel pour migration douce. |
| `kleos-cred/src/agent_keys_file.rs` (tests) | `engram-rust/foo` scopes | Tests rétro-compat. |

**Décision** : ne pas toucher. Tous documentés inline avec mention "legacy" ou
"rename".

---

## Catégorie 6 -- Noms de binaires / crates / produits

Noms exportés qui font partie de l'API publique.

| Item | Localisation |
|---|---|
| `engram-approval-tui` (binaire) | `kleos-approval-tui/Cargo.toml [[bin]]` |
| `engram-credd` (binaire) | `kleos-credd/Cargo.toml [[bin]]` |
| `engram-cred` (références doc) | `kleos-cred/src/lib.rs:1`, `migrate-cred.rs:1,17`, `cred-gui.rs:1-14` |
| `engram-cred-kdf-v1` (KDF domain) | `kleos-cred/src/crypto.rs:37` -- **CRITIQUE** : changer casse la dérivation de clé existante. |
| `engram-server`, `engram-cli`, etc. dans commentaires/doc | Multiples |
| `engram-rust` -- préfixe de catégorie pour secrets | Voir Catégorie 5 |
| `engram-backup-*.db` -- nommage des backup files | `kleos-lib/src/db/pitr.rs`, voir Catégorie 2 |

**Décision** : ne pas toucher. Renaming = breaking change majeur (KDF domain,
binaire path, configs externes).

---

## Catégorie 7 -- Commentaires / docs historiques

Mentions "Engram" dans les comments et docstrings (pas dans le code exécuté). Ex :

- `kleos-lib/src/brain/instincts/mod.rs:1` -- comment "fresh Engram brains"
- `kleos-lib/src/services/loom.rs:1662` -- "engram-style {system, prompt} shape"
- `kleos-lib/src/services/structural/mod.rs:3` -- "EN (Engram Notation)..."
- `kleos-lib/src/services/soma.rs:184` -- "mirrors the legacy engram-ts..."
- `kleos-server/src/routes/structural/mod.rs:2` -- "Mirrors the legacy Engram MCP..."
- `kleos-lib/src/services/chiasm/tasks.rs:615` -- "engram-style endpoint"
- `kleos-lib/src/graph/types.rs:37` -- "expected by engram-gui graph visualization"
- `kleos-lib/src/db/schema_sql.rs:1197, 1200` -- doc schema
- `kleos-lib/src/services/brain.rs:1651, 1674-1684` -- test prompts mentionnent "Engram"
- `kleos-migrate/src/source.rs:56` -- doc "older Engram databases"
- `kleos-approval-tui/src/main.rs:205` -- titre UI "ENGRAM APPROVAL CONSOLE"

**Décision** : ne pas toucher individuellement. Si rebrand visuel, batch via PR
upstream. Le titre UI `engram-approval-tui` mérite peut-être un patch local
"KLEOS APPROVAL CONSOLE" (cosmétique).

---

## Plan d'action

### Maintenant (Patch 13 local potentiel + push upstream)

1. **`activity.rs:261`** : `"engram"` -> `"kleos"` (FAIT 2026-05-18)
2. **`broca.rs:1209`** : ajout `"engram"` à `known_services` pour rétro-compat (FAIT 2026-05-18)
3. **`growth.rs:424`** : `"engram"` -> `"kleos"` (FAIT 2026-05-18). Pattern interne dreamer, invisible côté API mais cohérence est important : un résidu invisible est d'autant plus vicieux qu'il survit aux audits superficiels.
4. **`growth.rs:91`** : match arm `"engram" | "kleos" => ...` (FAIT 2026-05-18). Ceinture+bretelles : nouveau caller `"kleos"` matche, ancien `"engram"` reste valide.
5. **`growth.rs:407`** : `source LIKE '%-growth'` (FAIT 2026-05-18). Le hardcode `'engram-growth'` ratait tous les autres `<service>-growth` que le code peut produire (ligne 288 : `format!("{}-growth", req.service)`). Couvre legacy + post-rebrand + futurs services sans patch successif.

Ces 5 changements forment un commit semantique unique
`fix(broca,activity,growth): tag fanout/reflection events as kleos + accept engram as legacy alias`.
Candidat PR upstream (note explicative : résidus pre-rebrand "engram" qui restent
visibles dans les rows broca_actions + growth_observations, et bloquent Broca ask
sur installs neuves).

### Non touché

6. **Catégories 2-7** : paths legacy intentionnels, OTel cosmétique, tests, alias KDF critique, comments. Ne pas toucher sans coordination.

---

## Pour un rebrand complet futur (hors scope actuel)

Si jamais l'upstream décide un rebrand complet "engram -> kleos" :
- Couper les paths filesystem legacy (Catégorie 2) après une période de
  transition documentée.
- Renommer `engram-credd` -> `kleos-credd` (binaire) coordonné avec un
  systemd service rename + symlink pour rétro-compat.
- Changer KDF domain `engram-cred-kdf-v1` nécessite re-derivation forcée de
  toutes les clés -> NE PAS faire sans plan de migration.
- Catégorie 7 (comments) en batch via sed + review humaine.

C'est un travail d'environ 1-2 jours, pas dans le scope d'un patch local VOCSAP.
