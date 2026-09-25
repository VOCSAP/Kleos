# Plan d'implementation : kleos-cache (magasin vectoriel local, Rust)

ADR : `docs/superpowers/specs/2026-09-21-kleos-cache-decision.md`
Date : 2026-09-21
Precede par : `docs/superpowers/plans/2026-09-20-kleos-context-gateway-plan.md` (POC Python, Lots 0 a 4 executes)

## Conventions communes a tous les lots

- Depot : `C:\Users\Olivier\workspace\kleos-cache`, remote `VOCSAP/kleos-cache`, branche `main`.
  Aucun fichier du fork Kleos n'est touche par ce plan, sauf le hook `hooks/full/user-prompt-lean.sh`
  au Lot 4 (fichier deja en delta VOCSAP permanent, cf. `local-patches.md` "Hooks VOCSAP").
- Rust : `rust-toolchain.toml` epingle `1.94.0` (meme version que Kleos, deja active sur le poste :
  MESURE `rustup toolchain list` -> `1.94.0-x86_64-pc-windows-msvc (active)`). Edition 2021.
  Build natif MSVC, aucun passage par WSL.
- Crate unique `kleos-cache` : `src/lib.rs` (modules `config`, `store`, `index`, `embedder`, `redaction`,
  `kleos`, `sync`, `fusion`, `http`, `metrics`) et `src/main.rs` (sous-commandes `serve`, `sync`, `scan`,
  `export`, `import`, `token`). Dependances : `axum`, `tokio`, `reqwest` (json), `rusqlite`
  (`bundled`), `serde`/`serde_json`, `toml`, `sha2`, `subtle`, `rand`, `regex`, `tracing`, `clap`, `dirs`.
  Dev : `tempfile`, `wiremock` (doubles HTTP), `criterion` (bancs). Rien de `kleos-lib`.
- Config : `config.toml` dans le dossier de donnees (`%LOCALAPPDATA%\kleos-cache\`), surcharge par env
  `KLEOS_CACHE_*` (meme cascade que le POC `KCG_*`). Valeurs de depart :

  ```toml
  mode = "local"
  listen = "127.0.0.1:8765"

  [kleos]
  url = "http://192.168.10.21:4200"
  timeout_ms = 6000

  [ollama]
  url = "http://192.168.10.16:11434/v1/embeddings"
  model = "bge-m3"
  dim = 1024
  timeout_ms = 30000

  [retrieval]
  kleos_top_k = 12
  local_top_k = 12
  final_top_k = 5
  rrf_k = 10
  weight_kleos = 1.0
  weight_local = 1.0

  [sync]
  interval_s = 60
  page_size = 1000
  embed_batch = 32
  upsert_batch = 100
  deletion_guard_rows = 200
  deletion_guard_ratio = 0.10
  missing_passes_before_delete = 2

  [metrics]
  path = "retrieve.jsonl"
  log_queries = false
  ```

- Credentials : le jeton local vit dans `%LOCALAPPDATA%\kleos-cache\token` (0600, 64 hex, cree par
  `kleos-cache token`), jamais dans `config.toml`. La cle Kleos de lecture est lue dans `KLEOS_CACHE_SYNC_KEY`
  par le seul processus de sync ; l'operateur la cree avec `kleos-cli api-key create --scopes read`.
- Chaque lot : `agent-forge spec-task` avant le code (payload via `~/.agent-forge/scratch/`), tests
  unitaires avec doubles HTTP, `kleos-cli store` du resultat en fin de lot, tag `kleos-cache`, categorie
  `state`, space `Kleos`.
- Test de forme au debut de chaque lot a partir du Lot 2 : `cargo test --test kleos_contract -- --ignored`
  rejoue `/list`, `/spaces`, `/me` contre le vrai serveur et compare les cles JSON aux fixtures.
- Aucune ecriture vers Kleos, atteste par un test qui enregistre toutes les requetes emises par le client
  pendant un passage complet et echoue si l'une n'est pas `GET` ou `POST /memories/search`.
- Commande de test ciblee par lot : `cargo test -p kleos-cache --lib <module>::` ; la suite complete est
  lancee une fois par le sous-agent `test-runner` en fin de lot, jamais dans le fil principal.

## Portage : ce qui bouge, ce qui reste, ce qui disparait

| Source POC (`kleos-context-gateway/`) | Sort | Cible |
|---|---|---|
| `eval/evaluate.py`, `eval/queries.jsonl`, `eval/queries-v1-2026-09-20.jsonl`, `eval/reports/*` | **reste en Python**, deplace | `kleos-cache/eval/` (Lot 0) |
| `tests/unit/test_evaluate.py` | reste en Python | `kleos-cache/eval/tests/` |
| `tests/unit/test_rank_fusion.py`, `test_redaction.py` | cas exportes en fixtures JSON, puis le fichier Python est archive | `kleos-cache/tests/fixtures/{fusion,redaction}.json` |
| `src/kleos_context/{sync_worker,embedder,redaction,rank_fusion,kleos_adapter,app,config,metrics}.py` | **portes en Rust** | Lots 1 a 3 |
| `src/kleos_context/chroma_index.py`, `docker-compose.chroma.yml`, `tests/integration/test_chroma_index.py` | **jetes** | -- |
| Le reste du prototype | archive tel quel dans `kleos-cache/attic/kleos-context-gateway/` au premier commit (il n'est versionne nulle part : MESURE `git log` -> `fatal: not a git repository`) | Lot 0 |

## Lot 0 : depot, actifs d'evaluation, fixtures de contrat (demi-journee)

Objectif : rien de ce qui a ete mesure ne peut plus etre perdu, et le contrat HTTP avec Kleos est fige
dans des fichiers.

1. `git init`, `Cargo.toml`, `rust-toolchain.toml`, `src/lib.rs` vide, `src/main.rs` qui affiche la version.
   `.gitignore` : `target/`, `eval/.venv/`, `*.db*`, `token`.
2. Copier `eval/` du prototype, creer `eval/pyproject.toml` (deps `pytest`, `httpx`), venv `eval/.venv`.
   `pytest eval/tests -q` doit passer tel quel.
3. `evaluate.py` : renommer le mode `chroma_only` en `local_only` ; le constructeur appelle
   `POST {KLEOS_CACHE_URL}/v1/retrieve` avec `mode=local` et le jeton lu dans `KLEOS_CACHE_TOKEN` ; le mode
   `hybrid` idem avec `mode=hybrid` et, en plus, le bearer Kleos dans un en-tete `X-Kleos-Authorization`
   (relaye par kleos-cache, jamais utilise comme credential de route). Le mode `kleos_only` ne change pas.
4. `evaluate.py` : bootstrap apparie. Pour deux rapports JSON, re-echantillonner 10 000 fois les 40 requetes
   avec remise, calculer la difference de hit@5 et de MRR@5 par tirage, sortir l'intervalle a 95 %.
   Sous-commande `python eval/evaluate.py --compare <a.json> <b.json>`. Test sur un jeu synthetique de 5
   requetes a valeurs attendues calculees a la main.
5. Fixtures de contrat : enregistrer avec la cle de lecture une page `GET /list?offset=0&limit=3`,
   `GET /spaces`, `GET /me`, `POST /memories/search` (3 resultats). Passer chaque fichier par la redaction du
   POC avant de le committer, et verifier a la main qu'aucun contenu sensible n'y reste.
   `tests/fixtures/kleos/{list_page,spaces,me,search}.json`.
6. Exporter les cas de `test_rank_fusion.py` (listes disjointes, identiques, id d'un seul cote, egalite de
   score) et de `test_redaction.py` (chaque motif, faux positifs URL et hash git) en
   `tests/fixtures/fusion.json` et `tests/fixtures/redaction.json`, avec `input` et `expected` explicites.
7. Archiver le prototype dans `attic/`.

Commandes :

```bash
cargo build --manifest-path C:/Users/Olivier/workspace/kleos-cache/Cargo.toml
C:/Users/Olivier/workspace/kleos-cache/eval/.venv/Scripts/python -m pytest C:/Users/Olivier/workspace/kleos-cache/eval/tests -q
C:/Users/Olivier/workspace/kleos-cache/eval/.venv/Scripts/python C:/Users/Olivier/workspace/kleos-cache/eval/evaluate.py --compare <chroma_only.json> <kleos_only-mid.json>
```

Fait quand : `cargo build` vert sur le poste, `pytest eval/tests` vert, les quatre fixtures Kleos et les
deux fixtures de cas existent, `--compare` imprime un intervalle a 95 % pour vectoriel vs Kleos et pour
fusion vs Kleos sur les rapports du 2026-09-20, et le premier commit contient `eval/queries.jsonl`.
L'intervalle mesure remplace le "±0,15" de l'ADR section 2.3 (mise a jour de l'ADR incluse dans le lot).

## Lot 1 : store SQLite, index en memoire, client d'embeddings (1 jour)

1. `store.rs` : ouverture (`journal_mode=WAL`, `foreign_keys=ON`), schema de l'ADR 3.3 (`docs`,
   `vectors`, `sync_state`), `schema_version = 1`. Fonctions : `open(path)`, `upsert_batch(&[Doc], &[Vec<f32>])`
   (une transaction), `delete_batch(&[i64], reference_count)` (une transaction, ecrit `reference_count`),
   `all_hashes() -> HashMap<i64, String>`, `load_matrix() -> (Vec<i64>, Vec<f32>, Vec<DocMeta>)`,
   `state_get/state_set`, `integrity_check()` (docs sans vecteur, vecteurs sans doc, dimension non conforme).
2. `index.rs` : `Index { ids, matrix, meta }` construit depuis `load_matrix` ; `search(query: &[f32],
   spaces: Option<&[String]>, top_k) -> Vec<Hit{id, score}>` par produit scalaire, filtre de space
   inclusif applique AVANT le tri (jamais un top-k global filtre apres coup, qui rendrait moins de
   resultats que demande). `apply(upserts, deletes)` pour mettre la matrice a jour apres commit.
3. `embedder.rs` : trait `Embedder { fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> }` et
   `OllamaEmbedder` : lots de 32, body `{"model","input"}`, items tries par `index`, erreur si
   `len != dim`, normalisation L2 defensive (no-op a 1e-7 pres sur bge-m3, MESURE).
4. Verification d'identite : `state.embedding_identity = "<url_host>/<model>/<dim>"`. `open` refuse
   (`Err(IdentityMismatch)`) si la valeur stockee differe de la configuration ; `kleos-cache serve` s'arrete
   avec un message qui nomme les deux valeurs et propose `--rebuild`.
5. Bancs `benches/index.rs` (criterion) : chargement de 5 227 vecteurs synthetiques depuis SQLite ;
   recherche top-12 sur 5 227 x 1 024.
6. Tests : `store::` (upsert puis relecture, transaction interrompue par une erreur injectee a mi-lot ->
   compte inchange, `integrity_check` detecte un vecteur orphelin insere a la main, identite refusee),
   `index::` (filtre inclusif : un doc `default` et un `__unscoped` sortent pour n'importe quel space,
   un doc `autre` ne sort pas ; 5 vecteurs a la main avec classement attendu ; `top_k` superieur au
   corpus), `embedder::` (dimension, lots, reponse desordonnee par `index`, indisponibilite).

Commandes :

```bash
cargo test --manifest-path C:/Users/Olivier/workspace/kleos-cache/Cargo.toml --lib store::
cargo test --manifest-path C:/Users/Olivier/workspace/kleos-cache/Cargo.toml --lib index::
cargo bench --manifest-path C:/Users/Olivier/workspace/kleos-cache/Cargo.toml --bench index
```

Fait quand : tests verts ; banc mesure et consigne dans `docs/bench.md` : chargement < 200 ms et recherche
p50 < 5 ms sur 5 227 x 1 024 (reference numpy 1,58 ms ; au-dela de 5 ms, ajouter un chemin SIMD avant de
passer au Lot 2) ; le test de transaction interrompue est rouge quand on retire le `BEGIN`.
Mesures #1 et #2 de l'ADR section 7 renseignees.

## Lot 2 : client Kleos lecture seule, redaction, worker de sync (1 jour)

1. `kleos.rs` : `KleosClient { base_url, timeout }` avec exactement quatre methodes : `list_page(offset,
   limit, key)`, `list_spaces(key)`, `whoami(key)`, `search(query, space, limit, budget, bearer)`.
   Le transport est injectable (trait `Transport`) ; en test, un transport enregistreur capture
   `(method, path)` de chaque requete.
2. `redaction.rs` : les dix motifs du POC (`redaction.py:14-28`), plus :
   - `bearer` insensible a la casse ;
   - detecteur d'entropie : token de 32 caracteres ou plus dans `[A-Za-z0-9_\-+/=.]`, au moins trois
     classes de caracteres parmi {minuscule, majuscule, chiffre, symbole}, entropie de Shannon >= seuil ;
     le seuil de depart est 4,5 bits/caractere, **calibre au point 6** ;
   - application a `text`, `tags`, `category` ;
   - `REDACTION_VERSION: u32` constante ; le worker refuse de tourner si `state.redaction_version` differe
     et exige `--rebuild`.
3. `sync.rs` : un passage = `list_spaces` -> `list_page` jusqu'a page vide ou page sans id nouveau
   (`sync_worker.py:181-209`) -> pour chaque record projetable : `content_hash` (champs du POC, sans
   `updated_at`), redaction, comparaison au hash stocke -> embed + `upsert_batch` par lots de 100 ->
   candidats a suppression = ids stockes absents du passage ou devenus non projetables ; `missing_passes`
   incremente, suppression quand il atteint `missing_passes_before_delete` ; garde anti-suppression (ADR
   3.4) sur le compte de `desired` contre `state.reference_count` ; `delete_batch` avec la nouvelle
   reference dans la meme transaction ; en cas de refus, aucune reference ecrite. Verrou : un seul passage
   a la fois, le second retourne `skipped`.
   Au demarrage : `whoami(key)` ; `state.owner_user_id` fixe au premier passage, refus si different ensuite.
4. `metrics`/status : `Status { local_count, kleos_count, mismatched, parity, last_pass_at,
   last_duration_ms, last_error, degraded }` avec la parite du POC (`sync_worker.py:119-132` : division par
   le plus grand des deux cotes, `None` sur deux cotes vides).
5. Sous-commandes : `kleos-cache sync --once` (un passage, sortie JSON des compteurs), `kleos-cache scan`
   (rejoue tous les motifs de redaction sur `docs.text`, `tags`, `category` et imprime le nombre de hits par
   motif ; code de sortie 1 si > 0).
6. Calibrage de l'entropie : `kleos-cache scan --dry-run-entropy` sur les 5 227 documents avec les seuils
   4,0 / 4,5 / 5,0 ; revue manuelle des hits ; le seuil retenu et le tableau des comptes vont dans
   `docs/redaction.md`.
7. Tests, portage un pour un des 25 tests de `test_sync_worker.py` (MESURE `pytest tests/unit/test_sync_worker.py -q`
   -> `25 passed`), dans cet ordre : d'abord `an_empty_listing_deletes_nothing_and_degrades_the_pass`
   (rouge avant le garde, vert apres), `the_guard_survives_a_restart`, `a_deletion_under_the_guard_thresholds_
   still_happens`, puis les autres. Ajouts : `a_document_is_deleted_only_after_two_consecutive_absences`,
   `deletions_and_reference_count_land_in_one_transaction` (erreur injectee apres les DELETE -> reference
   inchangee), `only_get_and_search_leave_the_client` (transport enregistreur sur un passage complet),
   `tags_and_category_are_redacted`, `a_lowercase_bearer_is_redacted`, `a_bare_high_entropy_token_is_redacted`,
   `a_git_sha_and_a_url_are_not_redacted`, `a_redaction_version_change_refuses_to_run`,
   `an_owner_change_refuses_to_run`. Fixtures `tests/fixtures/redaction.json` rejouees.
8. Bootstrap reel contre LXC 121 avec la cle de lecture, en arriere-plan, `kleos-cache sync --once` deux
   fois de suite apres le bootstrap. Pendant le premier passage post-bootstrap, relever sur le LXC
   `systemctl show kleos-server -p MemoryCurrent` et `top -b -n 3 -p $(pidof kleos-server)` (mesure #3).

Commandes :

```bash
cargo test --manifest-path C:/Users/Olivier/workspace/kleos-cache/Cargo.toml --lib sync::
cargo test --manifest-path C:/Users/Olivier/workspace/kleos-cache/Cargo.toml --lib redaction::
cargo test --manifest-path C:/Users/Olivier/workspace/kleos-cache/Cargo.toml --test kleos_contract -- --ignored
C:/Users/Olivier/workspace/kleos-cache/target/release/kleos-cache sync --once
C:/Users/Olivier/workspace/kleos-cache/target/release/kleos-cache scan
```

Fait quand : les tests listes sont verts et le premier est atteste rouge sans le garde ; bootstrap reel en
15 min ou moins avec parite 100 ; deux passages consecutifs a zero upsert et zero suppression ; `scan` sort 0 ;
seuil d'entropie retenu et consigne ; mesures #3, #4 et #5 de l'ADR section 7 renseignees et stockees dans
Kleos. Si la mesure #3 montre un cout visible sur LXC 121, `interval_s` passe a 300 dans `config.toml`
avant le Lot 3.

## Lot 3 : surface HTTP, authentification, modes de recherche, fusion (1 jour)

1. `fusion.rs` : `rrf(lists, k, weights)` du POC (`rank_fusion.py:23-51` : arrondi a 12 decimales avant
   tri, tie-break par id, un id vu par un seul moteur garde sa seule contribution) et `fuse_results`.
   Fixtures `tests/fixtures/fusion.json` rejouees.
2. `http.rs` (axum) :
   - `GET /health` : etat de l'index (compte, identite), Ollama, Kleos (sondes en parallele, timeouts courts) ;
     seule route hors authentification ;
   - middleware `require_token` : `Authorization: Bearer <jeton local>`, comparaison en temps constant
     (`subtle::ConstantTimeEq`), 401 sinon, sur toute autre route ;
   - middleware `require_local_host` : `Host` doit etre `127.0.0.1[:port]` ou `localhost[:port]`, 421 sinon ;
     bind refuse si `listen` n'est pas loopback ;
   - `POST /v1/retrieve` `{query, space?, top_k?, mode?}` : `local` (embed + index), `kleos` (relais de
     `X-Kleos-Authorization` vers `POST /memories/search`, 401 si absent), `hybrid` (les deux en parallele
     avec budgets par source, `degraded[]`, 503 si les deux echouent) ; reponse `{mode, results[{id, score,
     final_rank, ranks, source, category, space, text}], degraded, latency_ms{local, kleos, embed}}` ;
   - `GET /v1/status`, `POST /v1/sync` (un passage a la demande, `skipped` si un passage court) ;
   - limite de corps 64 Ko sur `/v1/retrieve`.
3. `metrics.rs` : une ligne JSONL par requete (`sha256(query)` sauf `log_queries`), latences par source,
   erreurs, `degraded`.
4. `kleos-cache serve` : charge l'index, demarre la boucle de sync si `KLEOS_CACHE_SYNC_KEY` est
   presente (sinon sert l'index existant et le dit dans `/health`), arret propre sur Ctrl-C.
5. Tests : `http::` avec doubles (sans jeton -> 401, jeton faux de meme longueur -> 401, bon jeton -> 200,
   `Host: evil.example` -> 421, `Host: localhost:8765` -> 200, `/health` sans jeton -> 200) ; portage des
   7 tests de `test_retrieve_modes.py` (fusion classe le hit commun en premier, reponse locale seule quand
   Kleos est en panne, reponse Kleos seule quand Ollama est en panne, 503 quand tout echoue, source hors budget
   marquee `degraded`, modes simples sans degradation, mode inconnu refuse) ; `fusion::` sur les fixtures.
6. Mesure de parite avec le rapport Chroma : `evaluate.py --mode local_only --variant mid` puis
   `--mode hybrid --variant mid` contre kleos-cache, puis `--compare` avec les rapports du 2026-09-20.

Commandes :

```bash
cargo test --manifest-path C:/Users/Olivier/workspace/kleos-cache/Cargo.toml --lib http::
cargo test --manifest-path C:/Users/Olivier/workspace/kleos-cache/Cargo.toml --lib fusion::
C:/Users/Olivier/workspace/kleos-cache/target/release/kleos-cache serve
C:/Users/Olivier/workspace/kleos-cache/eval/.venv/Scripts/python C:/Users/Olivier/workspace/kleos-cache/eval/evaluate.py --mode local_only --variant mid
C:/Users/Olivier/workspace/kleos-cache/eval/.venv/Scripts/python C:/Users/Olivier/workspace/kleos-cache/eval/evaluate.py --mode hybrid --variant mid
```

Fait quand : tests verts ; rapport `local_only` a hit@5 0,575 et MRR@5 0,456 a une requete pres du rapport
Chroma (memes vecteurs, meme modele : un ecart superieur a une requete est un bug a expliquer, pas une
variance) ; rapport `hybrid` a hit@5 0,575 ; p50 `local_only` <= 250 ms et `latency_ms.embed` >= 90 % de
`latency_ms.local` (ce qui confirme que le levier suivant est l'embedding, pas le magasin) ; mesure #8
renseignee.

## Lot 4 : integration poste de travail (demi-journee)

1. `kleos-cache token` : genere le jeton, ecrit `token` en 0600, l'affiche une fois ; `kleos-cache export
   <fichier>` (copie coherente via `VACUUM INTO`) et `kleos-cache import <fichier>` (refuse si
   `embedding_identity`, `redaction_version` ou `owner_user_id` different de la configuration).
2. Demarrage : script `scripts/start-kleos-cache.ps1` enregistre dans le Planificateur de taches Windows a
   l'ouverture de session (`serve`, journal dans le dossier de donnees), et `scripts/stop-kleos-cache.ps1`.
   Le hook ne demarre rien : s'il ne trouve pas kleos-cache, il retombe.
3. Hook `hooks/full/user-prompt-lean.sh` (fork Kleos, delta VOCSAP existant) : avant l'appel sidecar
   existant, `curl -sf --max-time 1 -H "Authorization: Bearer $(cat "$LOCALAPPDATA/kleos-cache/token")"
   --data-binary "@$PAYLOAD_FILE" http://127.0.0.1:8765/v1/retrieve` avec `mode=local`,
   `space=$KLEOS_SPACE` ; payload via fichier temporaire (piege cp1252 de Git Bash) ; si vide ou en erreur,
   chaine existante inchangee (sidecar `/recall` puis `kleos-cli context`). Bloc injecte au format actuel
   "Relevant Kleos memories", chaque element `#id [categorie] [date] [local]`, en-tete de mise en garde
   conserve.
4. Verification : trois prompts consecutifs dans une session Claude Code, `retrieve.jsonl` gagne trois lignes ;
   arret de kleos-cache, un prompt de plus, la ligne de log du hook montre le fallback sidecar.

Fait quand : le jeton existe en 0600 ; `export` puis `import` dans un dossier temporaire donne le meme
`local_count` et un `/health` identique ; le hook sert depuis kleos-cache (trois lignes JSONL) et retombe
proprement quand il est arrete ; le temps ajoute au prompt, mesure dans le log du hook, est sous 400 ms.

## Lot 5 : decision finale et decommission (1 heure)

1. Mettre a jour l'ADR : tableau final `kleos_only-mid` / `local_only` / fusion POC, intervalles du bootstrap
   apparie, mesures #1 a #8 renseignees ; #3, #4, #5 et #7 sont non bloquantes ; reponses finales de
   l'operateur en section 9.
2. Decommission annulee : Chroma est conserve, son conteneur reste arrete et son bind mount est garde. Toute
   reprise future renvoie a l'ADR, section 10, et exige une nouvelle decision operateur explicite.
3. Stockage Kleos categorie `decision`, importance 8, tags `kleos-cache,decision`, space `Kleos`.
4. Mise a jour de `CLAUDE.local.md` : port, dossier de donnees, procedure de rebuild, commande de scan.

## Lots optionnels, hors decision

- **MCP** : `kleos-cache mcp` (stdio) exposant `context_search(query, space, mode, limit)`, `context_status()`,
  `context_sync()`. A ouvrir seulement si un usage hors hook apparait ; `kleos-mcp` et son allowlist
  (Patch 18) restent le serveur MCP de Kleos.
- **Embarqueur local** : impl `Embedder` sur ONNX bge-m3 sur le poste, pour attaquer les 180 ms ; change
  l'identite de l'index, donc rebuild, et exige une re-mesure sur le jeu de reference avant d'etre le defaut.
- **Enrichissement du jeu** : `log_queries = true` pendant une semaine, puis annotation manuelle de 20
  requetes reelles supplementaires avec leur `lexical_overlap`.

## Risques suivis

| Risque | Signal | Parade |
|---|---|---|
| Cout du rescan sur LXC 121 sature | RSS ou CPU de kleos-server qui monte pendant un passage (mesure #3) | `interval_s` 300, pages de 500 ; Patch 54 export seulement si cela ne suffit pas |
| Seuil d'entropie qui ampute le corpus | `scan --dry-run-entropy` avec des centaines de hits sur des identifiants legitimes | calibrage Lot 2, classes de caracteres >= 3, exclusion des hex purs |
| Rapport `local_only` different du rapport Chroma de plus d'une requete | `--compare` | verifier normalisation, filtre de space, tri stable ; ne pas "ajuster" le jeu |
| Derive du JSON de `/list` ou `/spaces` apres un merge upstream | `kleos_contract` en echec en debut de lot | mettre a jour les fixtures, puis le mapping ; ne jamais assouplir le test |
| Jeton local lu par un autre utilisateur du poste | ACL du fichier `token` | 0600 verifie a chaque demarrage, refus de servir sinon |
| Deux instances sur le meme dossier de donnees | `SQLITE_BUSY` ou index divergent | verrou fichier `serve.lock` a l'ouverture |
| `updated_at` bumpe par les lectures du sidecar et de `kleos-cli context` | rien : le hash l'exclut | conserver l'exclusion, la tester (`content_hash_ignores_fields_that_move_on_every_read`) |
