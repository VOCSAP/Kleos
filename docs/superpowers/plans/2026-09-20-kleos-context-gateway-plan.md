# Plan d'implementation : kleos-context-gateway, phase 1 (POC local)

Spec : `docs/superpowers/specs/2026-09-20-kleos-context-gateway-design.md`
Date : 2026-09-20

## Conventions communes a tous les lots

- Depot : `C:\Users\Olivier\workspace\kleos-context-gateway`, remote
  `VOCSAP/kleos-context-gateway`, branche `main`. Python 3.10, venv `.venv`,
  `pyproject.toml` avec `[project.optional-dependencies] dev = [pytest, pytest-asyncio, respx, ruff]`.
- Dependances runtime : `fastapi`, `uvicorn`, `httpx`, `chromadb-client`,
  `pydantic`, `pyyaml`, `mcp`.
- Config : `config.yaml` charge par `config.py` (pydantic), surcharge par env
  `KCG_*`. Valeurs de depart :

  ```yaml
  mode: kleos_only
  kleos:
    url: http://192.168.10.21:4200
    timeout_ms: 6000
  ollama:
    url: http://192.168.10.16:11434/v1/embeddings
    model: bge-m3
    timeout_ms: 3000
  chroma:
    url: http://127.0.0.1:8000
    collection: kleos_memory_bge_m3_v1
    timeout_ms: 1500
  retrieval:
    kleos_top_k: 12
    chroma_top_k: 12
    final_top_k: 5
    context_token_budget: 2200
    rrf_k: 60
    weight_kleos: 1.0
    weight_chroma: 1.0
  sync:
    interval_s: 60
    page_size: 1000
    embed_batch: 32
    upsert_batch: 100
  metrics:
    path: ./data/retrieve.jsonl
    log_queries: false
  ```

- Cle Kleos : jamais dans la config. Le gateway lit `Authorization` de la
  requete entrante et le relaie. Le worker de sync et l'outil `evaluate.py`
  lisent `KLEOS_API_KEY` dans l'environnement.
- Chaque lot : `agent-forge spec-task` avant le code, tests unitaires avec
  doubles HTTP (`respx`), `kleos-cli store` du resultat en fin de lot avec
  tag `chroma-gateway`, categorie `state`.
- Aucune ecriture vers Kleos depuis le gateway, verifie par un test qui
  echoue si `kleos_adapter` emet autre chose que GET ou POST `/memories/search`.

## Lot 0 : baseline Kleos (demi-journee)

Objectif : un rapport chiffre de Kleos seul, avant tout Chroma.

1. Creer le depot, `pyproject.toml`, `src/kleos_context/__init__.py`,
   `config.py`, `kleos_adapter.py` (fonctions `search(query, space, limit,
   budget, bearer)` et `list_page(offset, limit, bearer)`), `metrics.py`.
2. `eval/queries.jsonl` : 40 lignes minimum. Sources : `kleos-cli list`,
   handoffs recents, `retrieve.jsonl` inexistant encore, donc construction
   manuelle assistee. Schema par ligne :
   `{"id": "q001", "query": "...", "space": "kleos", "category": "decision",
   "expected_ids": [18393], "acceptable_ids": [], "lang": "fr"}`.
   Repartition cible : 10 decisions, 8 infrastructure, 8 choix techniques,
   6 preferences ou regles, 4 reprise de tache, 4 sans mot commun avec la
   cible. Au moins 8 en anglais et 4 mixtes.
3. `eval/evaluate.py` : lit `queries.jsonl`, appelle une fonction
   `retrieve(query, space, mode, variant)` fournie en parametre, calcule
   hit@1/3/5, MRR@5, nDCG@5, sans-resultat, hors-space, p50 et p95, ecrit
   `eval/reports/<date>-<mode>-<variant>.md` et `.json`.
4. Variantes Lot 0 : `kleos_only` avec `budget` = `high` (defaut), `mid`,
   `low`. La variante `low` est la borne "vecteur Lance seul + reranker".
5. Tests : `tests/unit/test_evaluate.py` (metriques sur un jeu synthetique
   de 5 requetes avec valeurs attendues calculees a la main),
   `tests/unit/test_kleos_adapter.py` (mapping de la reponse, propagation du
   Bearer, refus de toute methode d'ecriture).

Commandes :

```bash
python -m venv C:/Users/Olivier/workspace/kleos-context-gateway/.venv
C:/Users/Olivier/workspace/kleos-context-gateway/.venv/Scripts/pip install -e "C:/Users/Olivier/workspace/kleos-context-gateway[dev]"
C:/Users/Olivier/workspace/kleos-context-gateway/.venv/Scripts/python -m pytest C:/Users/Olivier/workspace/kleos-context-gateway/tests/unit -q
C:/Users/Olivier/workspace/kleos-context-gateway/.venv/Scripts/python C:/Users/Olivier/workspace/kleos-context-gateway/eval/evaluate.py --mode kleos_only --variant high
```

Fait quand : trois rapports `kleos_only-{high,mid,low}` existent, et le
tableau des metriques est stocke dans Kleos.

## Lot 1 : Chroma local et squelette gateway (demi-journee)

1. `docker-compose.chroma.yml` :

   ```yaml
   services:
     chroma:
       image: chromadb/chroma:1.5.9
       ports: ["127.0.0.1:8000:8000"]
       volumes: ["${USERPROFILE}/.local/share/kleos-chroma:/data"]
       environment:
         CHROMA_PERSIST_PATH: /data
         ANONYMIZED_TELEMETRY: "FALSE"
   ```

   Verifier apres `docker compose up -d` : `curl -sf http://127.0.0.1:8000/api/v2/heartbeat`.
2. `embedder.py` : `embed(texts: list[str]) -> list[list[float]]`, lots de 32,
   verifie `len == 1024`, erreur typee `EmbedderUnavailable`.
3. `chroma_index.py` : `ensure_collection()`, `upsert(items)`, `delete(ids)`,
   `query(vector, space, top_k)` avec `where = {"space": {"$in": [...]}}`,
   `count()`, `all_ids_with_hash()` (pagination `get` par 1000, champs
   `metadatas` seulement).
4. `redaction.py` : motifs `sk-[A-Za-z0-9]{20,}`, `ghp_[A-Za-z0-9]{30,}`,
   `AKIA[0-9A-Z]{16}`, `Bearer [A-Za-z0-9._-]{20,}`, `-----BEGIN [A-Z ]*PRIVATE KEY-----`,
   `password\s*[:=]\s*\S+`. Remplacement `[REDACTED]`.
5. `app.py` : FastAPI, `GET /health` (etat Kleos, Chroma, Ollama en
   parallele, timeouts courts), `POST /v1/retrieve` en mode `kleos_only`
   seulement a ce lot, `GET /v1/status` (stub).
6. Tests : `test_embedder.py` (dimension, lots, indisponibilite),
   `test_chroma_index.py` (contre un Chroma reel lance par le compose, marque
   `integration`), `test_redaction.py` (chaque motif, faux positifs sur une
   URL et un hash git), `test_app_health.py`.

Commandes :

```bash
docker compose -f C:/Users/Olivier/workspace/kleos-context-gateway/docker-compose.chroma.yml up -d
C:/Users/Olivier/workspace/kleos-context-gateway/.venv/Scripts/uvicorn kleos_context.app:app --host 127.0.0.1 --port 8765
curl -sf http://127.0.0.1:8765/health
```

Fait quand : `/health` renvoie `ok` pour les trois dependances et les tests
unitaires passent.

## Lot 2 : bootstrap et synchronisation (demi-journee)

1. `sync_worker.py` :
   - `run_full()` : parcourt `/list` (`include_forgotten=false`,
     `include_archived=false`, pages de 1 000, tous spaces), construit
     `{id: (content_hash, meta)}`, compare a `all_ids_with_hash()`, embed +
     upsert les nouveaux et modifies, delete les absents. Journal : compteurs
     `seen`, `upserted`, `deleted`, `errors`, duree.
   - Boucle periodique `interval_s`, un seul passage a la fois (verrou).
   - Mapping metadonnees selon spec 4.2. Le nom de space se resout via
     `GET /spaces` ou `kleos-cli space list` mis en cache au demarrage ;
     `space_id` NULL devient `"__unscoped"`.
2. `GET /v1/status` : `kleos_count`, `chroma_count`, `mismatched`,
   `last_sync_at`, `last_duration_ms`, `last_error`, `parity` en pourcentage.
3. `POST /v1/sync?full=true` declenche un passage a la demande.
4. Tests : `test_sync_worker.py` avec un faux `/list` de 3 pages contenant
   un ajout, une modification, une suppression, un `is_archived` ; verifie
   les appels upsert et delete attendus, l'idempotence d'un second passage
   (zero upsert, zero delete), et qu'aucune requete d'ecriture ne part vers
   Kleos.
5. Bootstrap reel : 13 000 memoires, 407 lots d'embedding. A 218 ms par
   petit appel, prevoir 10 a 20 minutes selon la taille des lots ; lancer en
   arriere-plan et suivre `/v1/status`.

Fait quand : `parity >= 99` sur `/v1/status`, deux passages consecutifs sans
upsert ni delete, resultat stocke dans Kleos.

## Lot 3 : mode shadow et collecte (demi-journee, puis une semaine)

1. `router.py` : `decide(query, family, space) -> RouteDecision` ; v1 renvoie
   toujours `memory` avec le filtre space inclusif.
2. `rank_fusion.py` : `rrf(lists: dict[str, list[id]], k, weights) ->
   list[(id, score, ranks_by_source)]`, deterministe, tie-break par id.
3. `/v1/retrieve` : execution parallele Kleos et Chroma (`asyncio.gather`,
   timeouts par source), modes `kleos_only`, `chroma_only`, `shadow`,
   `hybrid` ; en `shadow` la reponse est Kleos seul, la ligne JSONL porte les
   deux classements.
4. `metrics.py` : ligne JSONL par requete, `sha256(query)` sauf
   `log_queries: true`, latences par source, erreurs, `degraded`.
5. Hook : `hooks/user-prompt-lean.patch` ajoute, apres l'appel sidecar
   existant, un envoi en arriere-plan :

   ```bash
   curl -s --max-time 3 -o /dev/null -X POST http://127.0.0.1:8765/v1/retrieve \
     -H "Authorization: Bearer $KLEOS_API_KEY" -H "Content-Type: application/json" \
     --data-binary "@$PAYLOAD_FILE" &
   ```

   Le payload est ecrit dans un fichier temporaire pour eviter le probleme
   d'encodage cp1252 des `-d` inline sous Git Bash. `space` vient de
   `$KLEOS_SPACE`, `mode` = `shadow`.
6. `mcp_server.py` : outils `context_search`, `context_status`,
   `context_sync`. Declaration dans `~/.claude.json` ou `.mcp.json` du
   projet, commande `.venv/Scripts/python -m kleos_context.mcp_server`.
7. `evaluate.py` gagne `--mode chroma_only` et `--mode shadow` via
   `/v1/retrieve`.
8. Tests : `test_rank_fusion.py` (cas a la main : deux listes disjointes,
   deux identiques, un id present d'un seul cote, egalite de score),
   `test_retrieve_modes.py` (chaque mode avec doubles Kleos et Chroma,
   Chroma en panne donne `degraded` sans erreur, Kleos en panne en
   `chroma_only` donne `stale`), `test_router.py`.

Fait quand : rapport `chroma_only` vs `kleos_only` sur le jeu de reference,
et au moins 200 lignes JSONL collectees en usage reel sur une semaine.

## Lot 4 : mode hybrid (1 jour)

1. Budget de contexte : coupe a `final_top_k` puis a
   `context_token_budget` (estimation 4 caracteres par token), provenance et
   date conservees sur chaque element.
2. Formatage du bloc injecte, identique au bloc actuel "Relevant Kleos
   memories" avec en tete la phrase de mise en garde et par element
   `#id [categorie] [date] [source kleos|chroma|both]`.
3. Hook : variante Lot 4 du patch, le gateway devient la source, sidecar en
   fallback si le gateway ne repond pas en 3 s. Le mode par defaut passe a
   `hybrid` dans `config.yaml` uniquement quand le rapport le justifie.
4. `evaluate.py --mode hybrid`, plus un balayage `weight_chroma` dans
   `{0.5, 1.0, 1.5}` reporte dans le rapport.
5. Verification secrets : script `eval/scan_index.py` qui parcourt tous les
   documents Chroma avec les motifs de `redaction.py` et doit renvoyer zero.
6. Tests : `test_context_budget.py`, `test_format_block.py`.

Fait quand : rapport `hybrid` avec les trois poids, scan secrets a zero,
comparaison sur les 200 lignes de shadow (part des ids Chroma absents de
Kleos qui figurent dans `expected_ids` du jeu).

## Lot 5 : decision (1 heure)

1. Tableau final `kleos_only-high` / `kleos_only-low` / `chroma_only` /
   `hybrid` sur toutes les metriques.
2. Application du critere de la spec 5.4.
3. ADR `docs/superpowers/specs/2026-XX-XX-chroma-gateway-decision.md` dans
   le fork Kleos : verdict, chiffres, suite (phase 2 ou arret et suppression
   du conteneur).
4. Stockage Kleos categorie `decision`, importance 8, tags
   `chroma-gateway,decision,delta-upstream` si phase 2 retenue.

## Phase 2, si Lot 5 positif

### Lot 6 : deploiement LXC 121

1. Operateur : RAM du LXC a 4 Go minimum via Proxmox, verifier `free -m`.
2. Chroma : `python3 -m venv /opt/kleos-chroma/.venv && pip install chromadb==1.5.9`,
   unit systemd `kleos-chroma.service` : `chroma run --path /var/lib/kleos-chroma --host 127.0.0.1 --port 8000`.
3. Gateway : `/opt/kleos-gateway`, venv, unit `kleos-gateway.service` sur
   `0.0.0.0:8765`, `EnvironmentFile=/etc/kleos/gateway.env` sans cle Kleos
   (le worker de sync a besoin d'une cle de lecture : creer une cle Bearer
   `read` dediee via `kleos-cli api-key create --scopes read`).
4. MCP en transport HTTP streamable sur `/mcp` du gateway ; chaque poste
   declare l'URL, aucun venv local.
5. Hook des postes : URL du gateway configurable par `KCG_URL`, defaut
   `http://192.168.10.21:8765`.

### Lot 7 : Patch 54 Kleos, export additif

Branche `local/poc-chroma-gateway` depuis `local/patches`.

1. Nouveau fichier `kleos-server/src/routes/memory/export.rs` : handler
   `GET /memories/export?since=<rfc3339>&cursor=<id>&limit=<n<=1000>`
   renvoyant `items[]` avec les champs de `memory_to_json` plus
   `content_hash`, `embedding` (vecteur stocke ou `null`) et `deleted`
   (vrai si `is_forgotten` ou `is_archived`), et `next_cursor`.
   Ordre `(updated_at, id)`, filtre `updated_at >= since`.
2. Une ligne dans le routeur memory pour enregistrer la route.
3. Tests dans `export.rs` : pagination stable sur 2 500 lignes, `since`
   exclut les anciennes, tombstones presents, vecteur `null` quand absent.
4. Section Patch 54 dans `docs/dev-notes/local-patches.md`, stockage Kleos
   tag `delta-upstream`.
5. Build WSL, copie du binaire, redemarrage, `curl /health`, puis le
   `sync_worker` passe en mode `since` et n'embedde plus.

### Lot 8 : re-mesure et decision finale

1. Rejouer `evaluate.py` contre le gateway LXC, toutes variantes.
2. Verifier l'empreinte : `systemctl show kleos-chroma -p MemoryCurrent`,
   `free -m`, `du -sh /var/lib/kleos-chroma`.
3. ADR final : conserver le gateway, ou porter le canal dans kleos-server
   (patch dans `search.rs`, a chiffrer separement), ou arreter.

## Risques suivis

| Risque | Signal | Parade |
|---|---|---|
| Jeu de reference biaise vers Kleos | `expected_ids` choisis via `kleos-cli search` | construire au moins la moitie des requetes depuis les handoffs et la memoire de session, pas depuis une recherche Kleos |
| Bootstrap long ou rate-limit Kleos | 429 sur `/list` | pages de 1 000, pause 200 ms entre pages, reprise par offset |
| Ollama sature par le bootstrap | latence Kleos qui monte pendant la sync | lots de 32, un seul passage a la fois, bootstrap hors heures de travail |
| Derive de schema Kleos (`/list`) apres un merge upstream | test d'integration `test_kleos_list_shape` en echec | test execute contre le vrai serveur au debut de chaque lot |
