# llm-tap -- plan d'implementation (proxy de capture des appels LLM)

Statut : plan, rien d'implemente. Redige le 2026-10-12.
Option retenue : **A** -- depot separe, **zero delta upstream** dans Kleos (seule
la configuration par variables d'environnement change).

## 0. Objectif et perimetre

Capturer chaque echange LLM de Kleos (prompt systeme, prompt user, reponse,
parametres, latence) pour constituer :

1. le dataset de distillation (professeur = le Qwen3-8B de production, sorties
   filtrees) ;
2. le banc d'evaluation qui comparera les petits modeles candidats.

Hors perimetre de cette phase : validateurs par famille, juge local,
entrainement. Ils consommeront la base produite ici (cf. section 12).

Contrainte : **aucune sortie d'un modele Claude** ne doit entrer dans le
dataset ni servir a le filtrer (CGU Anthropic, section 3). Claude ecrit le code
et analyse les metriques agregees, rien de plus.

## 1. Inventaire des chemins LLM de Kleos (verifie dans le code)

| Module | Variable(s) d'URL | Forme attendue | Detection OpenAI-compat | `stream` | Statut |
|---|---|---|---|---|---|
| `kleos-lib/src/llm/local.rs` (`LocalModelClient`) | `KLEOS_LLM_URL` > `OLLAMA_URL` | URL complete `.../v1/chat/completions` | aucune (POST direct) | `false` | actif, chemin principal |
| `kleos-lib/src/intelligence/llm.rs` | `KLEOS_LLM_URL` > `ENGRAM_LLM_URL` > `OLLAMA_URL` | URL complete | -- | `false` | actif |
| `kleos-lib/src/services/broca.rs` | `BROCA_LLM_URL` > `LLM_URL` | base ou complete | contient `11434`, `ollama`, `/v1/chat` ou `/chat/completions` | `false` | actif |
| `kleos-lib/src/services/chiasm/tasks.rs` | `CHIASM_LLM_URL` > `LLM_URL` | base ou complete | idem broca | `false` | actif |
| `kleos-ingest/src/summarizer.rs` | `KLEOS_INGEST_OLLAMA_URL` > `OLLAMA_URL` | URL complete | -- | `false` | actif si ingest utilise |
| `kleos-lib/src/services/loom.rs` | champ `url` de la config du step LLM du workflow | complete | contient `/v1/chat` ou `/chat/completions` | a verifier | actif seulement si des workflows ont des steps LLM |
| `kleos-lib/src/handoffs/atoms.rs` | `sidecar_url` passe par l'appelant | base | -- | -- | **inactif** : l'unique appelant (`kleos-server/src/routes/handoffs/mod.rs:71`) passe `None` |

Pieges releves :

- **Detection par port** (broca, chiasm) : un proxy sur un autre port que
  `11434` sans `/v1/chat/completions` dans l'URL bascule ces services sur le
  format generique `{prompt, system}`. -> Toujours configurer l'URL
  **complete** `http://<hote>:<port>/v1/chat/completions`.
- **Garde SSRF** (`kleos-lib/src/net.rs::validate_outbound_url`, appelee par
  `local.rs`) : rejette loopback et RFC1918 sauf si `KLEOS_NET_ALLOW_PRIVATE=1`.
  Si Ollama est deja sur une IP LAN, la variable est probablement deja posee :
  **a verifier** sur le serveur avant bascule.
- **Probe** : `local.rs` interroge `/api/tags` (derive de l'URL) pour savoir si
  le modele est joignable. Le proxy doit relayer `GET /api/tags`.

## 2. Architecture

```
kleos-server / kleos-ingest
        |  POST /v1/chat/completions   (URL pointee vers le proxy)
        v
   llm-tap  (:11435)  ---- relais transparent ---->  Ollama (:11434, PC 2080 Ti)
        |
        +--> canal mpsc borne --> tache writer --> SQLite (llm-tap.db)
```

Principes :

1. **Transparence** : la reponse renvoyee a Kleos est byte-identique a celle
   d'Ollama (status, headers utiles, body). Kleos ne doit voir aucune
   difference, circuit breaker compris.
2. **Fail-open** : une erreur de capture (DB verrouillee, disque plein, canal
   plein) ne fait jamais echouer la requete. Elle incremente un compteur et
   logge un warning.
3. **Hors chemin critique** : l'ecriture SQLite est asynchrone via un canal
   borne ; si le canal est plein, l'enregistrement est abandonne (compteur
   `dropped`).
4. **Relais generique** : toute route autre que `POST /v1/chat/completions`
   (dont `/api/tags`, `/api/chat`, `/api/generate`) est relayee sans capture.

Emplacement : sur la machine de `kleos-server`, ou sur le PC d'inference.
A trancher (question ouverte 1).

## 3. Choix techniques

- **Rust** : meme toolchain que Kleos, binaire unique, deploiement identique.
- Crates : `axum` (serveur), `reqwest` (client upstream, `rustls`),
  `rusqlite` (`bundled`, pour Windows), `tokio`, `serde_json`, `tracing`,
  `sha2`, `clap` (sous-commandes CLI).
- Mode WAL SQLite ; un seul writer.

## 4. Structure du depot (`VOCSAP/llm-tap`, a creer)

```
llm-tap/
  Cargo.toml
  src/
    main.rs        # clap: serve | stats | export | catalog
    config.rs      # env vars
    proxy.rs       # handlers axum, relais upstream
    capture.rs     # struct CallRecord, canal, writer task
    db.rs          # schema, migrations idempotentes, requetes
    catalog.rs     # chargement des prompts Kleos, inference prompt_id
    derive.rs      # json_valid, strip <think>, extraction usage tokens
    export.rs      # export JSONL format chat
  tests/
    proxy_passthrough.rs
    catalog_match.rs
  deploy/
    llm-tap.service   # unite systemd
  README.md
```

## 5. Configuration (variables d'environnement)

| Variable | Defaut | Role |
|---|---|---|
| `LLM_TAP_LISTEN` | `127.0.0.1:11435` | adresse d'ecoute (jamais `0.0.0.0` sans besoin) |
| `LLM_TAP_UPSTREAM` | `http://127.0.0.1:11434` | base Ollama |
| `LLM_TAP_DB` | `./llm-tap.db` | fichier SQLite |
| `LLM_TAP_PROMPT_DIRS` | -- | liste `;`-separee : `<kleos>/kleos-lib/prompts;<prompts-overrides>` |
| `LLM_TAP_UPSTREAM_TIMEOUT_MS` | `600000` | doit depasser le plus long timeout cote Kleos |
| `LLM_TAP_MAX_CAPTURE_BYTES` | `1048576` | au-dela, body tronque et `truncated=1` |
| `LLM_TAP_CAPTURE` | `1` | `0` = relais pur sans capture (coupe-circuit) |
| `LLM_TAP_QUEUE` | `1000` | taille du canal vers le writer |

## 6. Schema SQLite

```sql
CREATE TABLE IF NOT EXISTS calls (
  id              INTEGER PRIMARY KEY,
  ts              TEXT    NOT NULL,          -- RFC3339 UTC, debut de requete
  prompt_id       TEXT    NOT NULL,          -- ex. extraction/facts, ou unknown:<hash8>
  match_kind      TEXT    NOT NULL,          -- exact | prefix | none
  model           TEXT,
  temperature     REAL,
  max_tokens      INTEGER,
  think           TEXT,                      -- valeur think / reasoning_effort si presente
  system_prompt   TEXT,
  user_prompt     TEXT,
  request_json    TEXT    NOT NULL,          -- body brut (pour rejouer a l'identique)
  response_raw    TEXT,                      -- content brut, <think> compris
  response_clean  TEXT,                      -- content sans bloc <think>
  http_status     INTEGER,
  error           TEXT,                      -- erreur reseau/upstream
  latency_ms      INTEGER,
  tokens_in       INTEGER,                   -- usage.prompt_tokens si fourni
  tokens_out      INTEGER,                   -- usage.completion_tokens si fourni
  json_valid      INTEGER,                   -- 1/0, NULL si non applicable
  truncated       INTEGER NOT NULL DEFAULT 0,
  system_sha256   TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS calls_prompt_ts ON calls(prompt_id, ts);

-- Rempli par les phases suivantes (validateurs, juge local, relecture humaine).
CREATE TABLE IF NOT EXISTS judgments (
  id        INTEGER PRIMARY KEY,
  call_id   INTEGER NOT NULL REFERENCES calls(id),
  judge     TEXT    NOT NULL,   -- validator:<nom> | llm:<modele> | human
  verdict   TEXT    NOT NULL,   -- pass | fail | partial
  score     REAL,
  reason    TEXT,
  ts        TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS judgments_call ON judgments(call_id);
```

Migrations : DDL idempotent (`IF NOT EXISTS`) + `PRAGMA user_version`.

## 7. Inference du `prompt_id`

Kleos n'envoie pas l'id du prompt ; on le deduit du prompt systeme.

1. Au demarrage (puis toutes les 60 s), parcourir `LLM_TAP_PROMPT_DIRS`. Pour
   chaque `<id>/system.txt` (les overrides priment sur l'embedded, comme la
   cascade Kleos), memoriser `id`, `sha256(normalise(texte))` et le texte
   normalise. Normalisation : `trim_end` par ligne, fins de ligne `\n`.
2. Match **exact** : `sha256(normalise(system_prompt))` dans la table.
3. Sinon match **prefixe** : le system recu commence par le texte d'un
   `system.txt` connu (cas `system.txt` + `system_suffix.txt`, cf. Patch 16).
   Retenir le prefixe le plus long.
4. Sinon (system compose ou modifie par l'appelant), tenter le prefixe statique
   du `user.txt` (texte avant le premier `{{`), longueur minimale 40 caracteres.
5. Sinon `unknown:<8 premiers hex du sha256>`. La commande `stats` liste les
   `unknown` les plus frequents pour completer le catalogue.

## 8. Champs derives

- `response_clean` : retirer `<think>...</think>` en tete (Qwen3).
- `json_valid` : appliquer sur `response_clean` la meme strategie que
  `kleos-lib/src/llm/mod.rs::repair_and_parse_json` (retrait des fences
  ```` ```json ````, extraction du premier objet/tableau equilibre), a porter
  a l'identique pour que "valide" signifie la meme chose que pour Kleos.
  `NULL` si la famille de prompt n'attend pas de JSON (table statique dans
  `derive.rs`, a remplir depuis le catalogue des prompts).
- `tokens_in` / `tokens_out` : champ `usage` de la reponse OpenAI-compat
  d'Ollama, s'il est present.

## 9. Bascule cote Kleos (configuration seulement)

Sur la machine de `kleos-server`, avec le proxy sur `<tap>` :

```bash
KLEOS_LLM_URL=http://<tap>:11435/v1/chat/completions
BROCA_LLM_URL=http://<tap>:11435/v1/chat/completions    # ou LLM_URL si partage
CHIASM_LLM_URL=http://<tap>:11435/v1/chat/completions
KLEOS_INGEST_OLLAMA_URL=http://<tap>:11435/v1/chat/completions
# verifier : KLEOS_NET_ALLOW_PRIVATE=1 deja present ?
```

- Workflows Loom avec steps LLM : modifier le champ `url` du step (a inventorier).
- Noter les valeurs actuelles avant modification : le **rollback** consiste a
  les restaurer et redemarrer `kleos-server`.

## 10. Securite et confidentialite

- Les captures contiennent les memoires personnelles stockees dans Kleos.
  La base ne quitte pas le reseau local ; aucun envoi vers un service externe.
- Ecoute sur `127.0.0.1` ou l'interface LAN/WireGuard uniquement.
- Fichier DB en `0600`, utilisateur systemd dedie.
- Ne jamais journaliser les headers `Authorization` (ni en DB ni en logs).
- Retention : purge configurable (`llm-tap purge --older-than 90d`), duree a
  fixer (question ouverte 4).

## 11. Tests

Unitaires :

- `catalog` : match exact, prefixe (system + suffix), prefixe user, `unknown`,
  priorite override > embedded.
- `derive` : strip `<think>`, `json_valid` sur fences, JSON tronque, texte libre.

Integration (upstream simule par un serveur axum de test) :

- reponse byte-identique (status, body) pour 200, 4xx, 5xx ;
- upstream injoignable -> erreur relayee, une ligne `calls` avec `error` ;
- DB en lecture seule -> la requete reussit quand meme (fail-open) ;
- `GET /api/tags` relaye sans capture ;
- body > `LLM_TAP_MAX_CAPTURE_BYTES` -> relais complet, capture tronquee.

Bout en bout sur le PC :

1. Lancer `llm-tap` devant l'Ollama reel.
2. Basculer les variables Kleos (section 9).
3. Declencher chaque chemin : store memoire (extraction), `/ask` (broca),
   generation de plan (chiasm), reflexion growth, ingest si utilise.
4. `llm-tap stats` : au moins une ligne par `prompt_id` attendu, zero
   `unknown` non explique, latence ajoutee negligeable devant celle du LLM.

## 12. CLI d'exploitation et interface avec les phases suivantes

- `llm-tap stats [--since 7d]` : volume, taux `json_valid`, taux d'erreur,
  latence p50/p95, par `prompt_id` ; top des `unknown`.
- `llm-tap export --prompt-id <id> [--only-valid] [--judged pass] --out f.jsonl` :
  format chat (`{"messages":[{"role":"system"...},{"role":"user"...},{"role":"assistant","content":response_clean}]}`),
  directement consommable par TRL / Unsloth.
- `llm-tap catalog` : liste des prompts charges et de leur empreinte.

Phases suivantes (hors de ce plan) :

1. Validateurs automatiques par famille -> table `judgments` (`validator:*`).
2. Juge LLM local (Qwen3-8B en mode reflexion, ou plus gros) avec grilles ecrites.
3. Relecture humaine d'un echantillon pour calibrer le juge.
4. Baseline : Qwen3-4B / 1.7B non entraines rejoues sur les `request_json` captures.
5. Fine-tuning LoRA sur la RTX 5080, export GGUF, bascule via `KLEOS_LLM_MODEL`.

## 13. Ordre d'implementation

1. Squelette `serve` : relais pur sans capture + test byte-identique.
2. Schema SQLite + writer asynchrone + fail-open.
3. Champs derives (`derive.rs`).
4. Catalogue et inference `prompt_id`.
5. CLI `stats`, `export`, `catalog`, `purge`.
6. Unite systemd, README.
7. Bascule Kleos, test bout en bout, une a deux semaines de collecte.

## 14. Questions ouvertes

1. Ou tourne `llm-tap` : machine `kleos-server` ou PC d'inference (2080 Ti) ?
2. Des workflows Loom ont-ils des steps LLM ?
3. `KLEOS_NET_ALLOW_PRIVATE=1` est-il deja pose sur le serveur ?
4. Duree de retention des captures ?
5. `kleos-ingest` est-il utilise (sinon son chemin peut etre ignore) ?
