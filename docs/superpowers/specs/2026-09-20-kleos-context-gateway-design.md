# kleos-context-gateway : projection Chroma de la memoire Kleos pour les agents Claude Code

Date : 2026-09-20
Statut : design valide en session, en attente de relecture operateur
Origine : `comparaison/COMPARATIF_MEMOIRES_AGENTS_IA.md` et `comparaison/chroma_interaction.txt`

## 1. Objectif

Mesurer si un second moteur de rappel semantique (Chroma), place en middleware
entre Claude Code et Kleos, ameliore la pertinence du contexte injecte aux
agents. Kleos reste la memoire canonique. Chroma est une projection jetable.
La latence n'est pas un critere de decision.

Non-objectifs :

- Remplacer Kleos ou l'un de ses index.
- Tester un autre modele d'embedding que bge-m3 (voir 3.2).
- Ecrire dans Chroma autrement que par synchronisation depuis Kleos.
- Couvrir les familles handoff et conversations en v1.

## 2. Faits mesures le 2026-09-20

Kleos sur LXC 121 (192.168.10.21:4200, kleos-server 1.10.0) :

| Mesure | Valeur |
|---|---|
| Memoires actives / chunks | 13 035 / 17 999 |
| Index vectoriel | LanceDB, chunk path actif (`KLEOS_USE_CHUNK_VECTOR_SEARCH=1`) |
| Embeddings | bge-m3 1024 dims via Ollama 192.168.10.16:11434 |
| Reranker | TEI bge-reranker-v2-m3 sur 192.168.10.16:8080, top_k 24 |
| Latence `kleos-cli search` depuis le poste | 2,2 s a 4,1 s |
| Latence `kleos-cli context` | 1,45 s |
| RAM LXC | 3 Go, kleos-server RSS 1,35 Go, pic 3,15 Go, swap 1,2 Go utilise |
| Ports libres sur le LXC | 8000, 8765 |

Chroma 1.5.9 (release 2026-05-05) :

- Aucune authentification integree depuis la v1.0.0. Toute exposition reseau
  exige un proxy ou un pare-feu.
- Index HNSW entierement en RAM. Regle documentee pour 1024 dims :
  `N (millions) = R (Go) x 0,245`, soit environ 75 Mo pour 18 000 vecteurs.
  Systeme sous 2 Go de RAM deconseille.
- Metadonnees : tableaux natifs, filtres `$in`, `$nin`, `$contains`, `$and`, `$or`.
- `query_embeddings` et embeddings precalcules supportes.

API Kleos utilisable sans patch :

- `POST /memories/search` : `query`, `limit`, `space`, `include_unscoped`,
  `category`, `tags`, `budget`. Reponse avec `semantic_score`, `fts_score`,
  `score`.
- `GET /list` : pagination `limit` (max 1000) et `offset` sur `created_at`,
  filtres `include_forgotten`, `include_archived`, `space`. Champs `tags`,
  `space_id`, `importance`, `is_static`, `is_forgotten`, `is_archived`,
  `updated_at`.
- Manques : aucun filtre sur `updated_at`, aucun export du vecteur stocke.

Hooks Claude Code deja en place sur le poste : `user-prompt-lean.sh` injecte
a chaque prompt un bloc "Relevant Kleos memories" obtenu du sidecar local
`/recall` (127.0.0.1:7711), fallback `kleos-cli context --limit 8`.

## 3. Decisions

### 3.1 Deux phases, POC local d'abord

Phase 1 (POC) tourne entierement sur le poste Windows : Chroma dans Docker
Desktop, gateway Python local, aucune modification du LXC ni de Kleos. Si la
mesure est positive, la phase 2 redeploie sur LXC 121 pour tous les
peripheriques, puis une decision finale est prise.

Pourquoi : la mesure de pertinence ne depend pas de l'emplacement. Toucher le
LXC (RAM, services) avant d'avoir un resultat est un cout sans retour si le
POC est negatif.

### 3.2 Embeddings : bge-m3 via Ollama, une seule collection

Le gateway embedde lui-meme via `POST http://192.168.10.16:11434/v1/embeddings`
avec `model=bge-m3`, comme Kleos. Le modele e5-base propose dans l'etude est
ecarte : il exigerait un runtime torch (~1 Go resident) sur chaque hote, et il
rendrait la comparaison inexploitable, puisque deux variables changeraient a
la fois (modele et moteur).

Consequence assumee : Chroma et Kleos portent le meme modele. L'experience
mesure l'apport du moteur et de la fusion, pas du modele. Les vecteurs ne sont
pas identiques bit a bit (texte d'entree potentiellement different de celui que
Kleos chunk), ce qui est acceptable pour une mesure de pertinence agent.

### 3.3 Gateway Python separe, Kleos non patche en phase 1

Le gateway est un projet independant (`VOCSAP/kleos-context-gateway`, clone
local `C:\Users\Olivier\workspace\kleos-context-gateway`), pas un crate du
workspace ni un patch de `search.rs`. Un canal Chroma interne a kleos-server
(option C de la session) reste la cible d'integration si hybrid gagne, et sera
evalue en phase 2.

Si le POC exige malgre tout un patch Kleos (Patch 54 export ou autre), il se
fait sur une branche dediee du fork, `local/poc-chroma-gateway`, creee depuis
`local/patches`. Tout patch touchant kleos-server implique un build WSL et la
copie du binaire sur LXC 121 (procedure dans `CLAUDE.local.md`) avant d'etre
utilisable par le gateway.

### 3.4 Synchronisation par tirage, sans outbox

Kleos est le seul ecrivain. Le gateway ne propose pas de `context_store`, pas
d'outbox, pas de hook PostToolUse. Un worker tire `/list` en pages de 1 000,
calcule `sha256(content)` et compare au hash stocke dans Chroma. La phase 2
ajoute un export additif cote Kleos (Patch 54) avec filtre `since` et vecteur
stocke, ce qui supprime le rescan et le re-embedding.

### 3.5 Famille memory seule en v1

Handoffs (428 entrees) et conversations sont hors v1. Ils entrent en v1.1 si
la mesure sur memory est positive, avec une collection par famille.

## 4. Architecture phase 1 (POC local)

```
Claude Code (poste Windows)
   |
   | UserPromptSubmit : user-prompt-lean.sh
   |   POST http://127.0.0.1:8765/v1/retrieve  (timeout 3 s, fail-open)
   | MCP stdio : kleos-context (context_search, context_status, context_sync)
   v
kleos-context-gateway (Python, venv, 127.0.0.1:8765)
   |-- router        : famille, space, filtres
   |-- kleos_adapter : /memories/search, /list, /memory/{id}  (Bearer de l'appelant)
   |-- embedder      : Ollama bge-m3, prefixe aucun (bge-m3 n'en requiert pas)
   |-- chroma_index  : chromadb-client HttpClient -> 127.0.0.1:8000
   |-- rank_fusion   : RRF k=60, poids 1.0 / 1.0
   |-- sync_worker   : rescan /list + hash, upsert / delete
   |-- metrics       : JSONL par requete
   v
Chroma 1.5.9 (Docker Desktop, image epinglee, 127.0.0.1:8000, volume local)
```

Kleos reste sur LXC 121, inchange.

### 4.1 Composants et contrats

| Module | Entree | Sortie | Depend de |
|---|---|---|---|
| `router` | requete brute, space, categories | `RouteDecision{family, filters}` | rien |
| `kleos_adapter` | `RouteDecision`, query, limit, Bearer | liste `(kleos_id, rank, score, content, meta)` | httpx, Kleos HTTP |
| `embedder` | texte | `list[float]` 1024 | httpx, Ollama |
| `chroma_index` | vecteur, filtres, limit | liste `(kleos_id, rank, distance)` | chromadb-client |
| `rank_fusion` | deux listes classees | liste fusionnee avec provenance | rien |
| `sync_worker` | page `/list` | upserts et deletes Chroma | kleos_adapter, embedder, chroma_index |
| `metrics` | evenement retrieve | ligne JSONL | rien |

Chaque module est testable seul avec des doubles HTTP.

### 4.2 Modele d'index Chroma

Collection : `kleos_memory_bge_m3_v1`. Un document par memoire, id = id Kleos
en chaine. Pas de chunking en v1.

Document vectorise : `content` seul. Aucun identifiant ni timestamp dans le
texte.

Metadonnees :

| Cle | Type | Source Kleos |
|---|---|---|
| `space` | string | `space_id` resolu en nom, `"__unscoped"` si NULL |
| `category` | string | `category` |
| `tags` | array[string] | `tags` |
| `importance` | int | `importance` |
| `is_static` | bool | `is_static` |
| `created_at`, `updated_at` | int epoch | idem |
| `content_hash` | string | `sha256(content)` calcule par le gateway |
| `schema_version` | int | constante 1 |

Filtre de lecture : `space $in [space_courant, "default", "__unscoped"]`,
reproduction de la semantique inclusive de Kleos (`include_unscoped=true`).
Les memoires `is_forgotten` ou `is_archived` sont supprimees de Chroma, pas
filtrees.

### 4.3 Flux de lecture

Requete `POST /v1/retrieve` :

```json
{ "query": "...", "space": "Kleos", "family": "memory",
  "mode": "shadow", "limit": 5, "session_id": "..." }
```

1. `router` fixe la famille (memory en v1) et les filtres.
2. En parallele : `kleos_adapter.search(top 12)` et
   `embedder` puis `chroma_index.query(top 12)`.
3. Deduplication par id, fusion RRF, coupe a `limit`, budget 2 200 tokens.
4. Selon le mode, la reponse contient la liste Kleos, la liste Chroma, ou la
   fusion. En `shadow`, la reponse est Kleos seul et les rangs Chroma ne
   servent qu'a la journalisation.
5. `metrics` ecrit une ligne : mode, `sha256(query)`, ids et rangs par moteur,
   ids retournes, latences, erreurs. Le texte de la requete n'est pas
   journalise sauf `KCG_LOG_QUERIES=1`.

Reponse :

```json
{ "mode": "shadow", "results": [ { "kleos_id": 18393, "source": ["kleos","chroma"],
  "kleos_rank": 3, "chroma_rank": 1, "final_rank": 1, "content": "...",
  "category": "state", "updated_at": "...", "stale": false } ],
  "latency_ms": { "kleos": 2100, "chroma": 260, "total": 2140 } }
```

Le bloc injecte par le hook garde le format actuel ("Relevant Kleos memories")
avec provenance, categorie et date par element, et une phrase d'entete
indiquant des donnees recuperees, pas des instructions.

### 4.4 Modes

| Mode | Retourne | Usage |
|---|---|---|
| `kleos_only` | Kleos | baseline, Lot 0 |
| `chroma_only` | Chroma | mesure du moteur seul |
| `shadow` | Kleos, Chroma journalise | collecte sans risque, Lot 3 |
| `hybrid` | fusion RRF | candidat, Lot 4 |

Le mode par defaut est dans `config.yaml` et surchargeable par requete.

### 4.5 Synchronisation

- Bootstrap : `/list` avec `include_forgotten=false`, `include_archived=false`,
  pages de 1 000, embedding par lots de 32, upsert Chroma par lots de 100.
- Rescan periodique (60 s) : meme parcours, upsert si `content_hash` differe
  ou id absent ; les ids presents dans Chroma mais absents du parcours sont
  supprimes (couvre forgotten, archived, deleted).
- Parite : `context_status` renvoie `kleos_count`, `chroma_count`,
  `mismatched`, `last_sync_at`, `last_error`. Objectif : parite > 99 %.
- Aucune ecriture vers Kleos.

### 4.6 Integration Claude Code

- `user-prompt-lean.sh`, Lot 3 (shadow) : l'injection actuelle via le sidecar
  `/recall` ne change pas. Le hook envoie en plus le prompt a
  `POST /v1/retrieve` en arriere-plan (`curl --max-time 3`, sortie ignoree)
  pour la seule collecte JSONL. Aucun effet sur le contexte injecte.
- `user-prompt-lean.sh`, Lot 4 (hybrid) : le gateway devient la source de
  l'injection, sidecar `/recall` en fallback si le gateway ne repond pas en
  3 s. Filtre existant des prompts courts conserve, format injecte inchange.
- MCP stdio `kleos-context` : `context_search(query, space, mode, limit)`,
  `context_status()`, `context_sync(full: bool)`.
- SessionStart : pas de changement en phase 1.

### 4.7 Securite

- Chroma lie a 127.0.0.1, telemetrie desactivee, volume sous le profil
  utilisateur.
- Le gateway ne stocke aucun secret Kleos : il relaie le `Authorization`
  de l'appelant vers Kleos. Le hook et le MCP lisent la cle comme aujourd'hui.
- Redaction avant upsert : les motifs de secrets (cles, tokens, mots de passe)
  sont remplaces par `[REDACTED]`. Kleos gate deja ces contenus, la redaction
  est une defense en profondeur.
- Journal sans texte de requete par defaut.

### 4.8 Pannes

| Panne | Comportement |
|---|---|
| Chroma ou Ollama indisponible | resultats Kleos seuls, `degraded: true` |
| Kleos indisponible | mode `chroma_only` avec `stale: true` |
| Gateway indisponible | le hook retombe sur le sidecar actuel |
| Index corrompu | suppression de la collection et bootstrap |

## 5. Evaluation

### 5.1 Jeu de reference

`eval/queries.jsonl`, 40 requetes minimum construites a partir des usages
reels : decisions passees, infrastructure, choix techniques, preferences,
reprise de tache, formulations FR, EN et mixtes, requetes sans mot commun avec
le document attendu. Chaque ligne : `query`, `space`, `expected_ids`,
`acceptable_ids`, `category`.

### 5.2 Metriques

hit@1, hit@3, hit@5, MRR@5, nDCG@5, taux sans resultat, resultats hors space,
latence p50 et p95, taille injectee. Calculees par `eval/evaluate.py` pour
chaque mode, rapport Markdown dans `eval/reports/`.

### 5.3 Variantes Kleos internes (Lot 0)

Avant Chroma, deux variantes Kleos seules pour isoler le pipeline :
reranker actif (defaut) et `budget` reduit si l'API le permet. Elles donnent
la borne de ce qu'un reglage Kleos seul obtient.

### 5.4 Critere d'activation de `hybrid`

- MRR@5 ou hit@5 en hausse d'au moins 10 % relatifs par rapport a
  `kleos_only`.
- Aucune categorie en regression de plus de 5 points de hit@5.
- Parite d'index > 99 %.
- Aucun secret en clair dans l'index (verification par grep des motifs).

## 6. Lots

| Lot | Contenu | Livrable | Duree |
|---|---|---|---|
| 0 | Jeu de requetes, `evaluate.py`, rapport `kleos_only` et variantes | Baseline chiffre | demi-journee |
| 1 | Docker Chroma epingle, squelette gateway, `/health`, config, tests unitaires des modules | Services repondent | demi-journee |
| 2 | Bootstrap index, worker sync, `context_status`, parite mesuree | Parite > 99 % | demi-journee |
| 3 | Mode shadow dans le hook, MCP `context_search`, collecte JSONL | Rapport kleos vs chroma | demi-journee, puis 1 semaine de collecte |
| 4 | RRF, mode hybrid, rapport | Rapport hybrid | 1 jour |
| 5 | Decision POC : phase 2 ou arret | ADR | 1 heure |

Phase 2, seulement si Lot 5 est positif :

| Lot | Contenu |
|---|---|
| 6 | LXC 121 a 4 Go minimum, Chroma et gateway sous systemd, MCP HTTP distant |
| 7 | Patch 54 Kleos : `GET /memories/export?since&cursor&limit` avec vecteur stocke et tombstones, additif |
| 8 | Re-mesure sur LXC, decision finale, evaluation d'un canal interne a kleos-server |

Chaque lot ouvre une spec agent-forge et stocke son resultat dans Kleos.

## 7. Arborescence du projet gateway

```
kleos-context-gateway/
  pyproject.toml
  config.example.yaml
  docker-compose.chroma.yml
  src/kleos_context/
    app.py            (FastAPI, /health, /v1/retrieve, /v1/status, /v1/sync)
    mcp_server.py     (stdio)
    router.py
    kleos_adapter.py
    chroma_index.py
    embedder.py
    rank_fusion.py
    sync_worker.py
    redaction.py
    metrics.py
    config.py
  tests/unit/
  tests/integration/
  eval/
    queries.jsonl
    evaluate.py
    reports/
  hooks/
    user-prompt-lean.patch   (diff applique au hook existant)
```

## 8. Options ecartees

- e5-base local : RAM et confusion de variables (3.2).
- Gateway par poste avec Chroma central : Chroma sans auth expose sur le LAN.
- Crate Rust dans le workspace : rebuild WSL a chaque reglage d'experience.
- Canal Chroma dans `search.rs` des la phase 1 : delta upstream sur le site
  de merge le plus conflictuel, pour une experience peut-etre jetable.
- Outbox et `context_store` : inutiles tant que Kleos est le seul ecrivain.
- Chunking cote gateway en v1 : ajoute une variable ; a evaluer en v1.1.

## 9. Questions ouvertes

Resolues le 2026-09-20 :

- `budget` de `/memories/search` vaut `low` (vecteur seul), `mid` (vecteur
  et FTS) ou `high` (pipeline complet). Le reranker s'applique dans tous les
  cas, sans bascule par requete. Les variantes du Lot 0 sont donc
  `budget` low / mid / high.
- Le hook passe `$KLEOS_SPACE` (resolu par le hook session-start, valeur
  `kleos` dans ce depot) au gateway ; la liste des spaces vient de
  `kleos-cli space list` ou de l'API equivalente.
