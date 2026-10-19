# Plan d'etude et d'implementation : supersession des memoires (intention muninn)

Statut : **plan, rien d'implemente**. Redige le 2026-10-17 depuis une session cloud
sans acces a la base ni aux transcripts. Concu pour etre deroule dans une session
dediee sur le PC, avec `kleos-server`, `kleos-ingest` et les hooks en place.

Comment utiliser ce fichier : derouler les phases dans l'ordre, cocher les cases,
remplir le journal (section 14) a chaque mesure. Une phase de code ne demarre pas
tant que la porte de decision de la phase precedente (section 11) n'est pas passee.

Source etudiee : https://github.com/ilien-dev/muninn (clone lu au commit `23b83613`,
version 1.0.5). Fichiers cles : `docs/technical-overview.md`, `design/ENGINE.md`
(sections 5, 6.1, 12), `crates/muninn-core/src/schema.sql`,
`crates/muninn-capture/src/ingest.rs`, `crates/muninn-cli/src/maintain.rs`,
`research/00-evidence-log.md` (entrees C1, K6, K7/N4, K10, X1, Z5, Z11).

---

## 1. Intention

Quand l'utilisateur change d'avis, l'ancienne decision ne doit plus atteindre le
modele, la nouvelle doit arriver en premier, et les deux doivent rester en base
avec le lien "qui a remplace quoi". L'exemple canonique :

- lundi : "on compresse en gzip" ;
- jeudi : "changement de plan, on passe a zstd" ;
- deux semaines plus tard, nouvelle session : "documente la compression".

Aujourd'hui Kleos sert les deux memoires avec des scores voisins et laisse le
modele deviner. L'objectif est triple :

1. **Garantie structurelle** : une memoire marquee remplacee n'est jamais servie,
   quel que soit le canal (FTS, vecteur, graphe, faits, contexte statique,
   evolution). Pas "rarement", jamais, et un test le prouve.
2. **Detection** : poser cette marque automatiquement dans les cas qu'une regle
   deterministe atteint sans faux positif. Un faux retrait est pire qu'un retrait
   manque : il fait disparaitre un fait vivant en silence.
3. **Livraison** : presenter au modele la liste des decisions actives, la plus
   recente d'abord, avec `replaces #n` et `conflict` explicites. Chez muninn,
   c'est la livraison, pas la detection, qui a fait passer le taux de bonne
   reponse de 4 a 9 sur 27 a 39 puis 54 sur 54, a mecanisme de retrait identique.

Ce que l'etude muninn apporte de plus precieux n'est pas son code (licence
incompatible, voir section 2) mais ses mesures negatives : ce qu'il ne faut pas
construire (section 12).

---

## 2. Cadre

### Licence

muninn est sous **AGPL-3.0-only** avec clause NOTICE ; Kleos est sous
**Elastic-2.0**. Aucune ligne de code, aucune liste de regex, aucun fichier de
fixture ne doit etre copie. On reprend les principes, les parametres publies
(seuils, gardes) et la methode de mesure. Les listes de marqueurs de changement
sont a rederiver en francais et en anglais depuis nos propres transcripts.

### Politique d'ecart upstream (CLAUDE.md)

Niveau le plus bas qui resout le probleme, dans l'ordre :

| Niveau | Ce qui s'y range dans ce plan |
|---|---|
| Zero delta (overlay) | classes lexicon TOML (phase 3), colonne `origin` via overlay Patch 41 (phase 6), prompts via `prompts-overrides` |
| Additif pur | regle de supersession sur `structured_facts` (phase 2), module d'ingestion git (phase 5), catalogue (phase 4) |
| Patch chirurgical | predicats `is_superseded = 0` au recall, `correct_memory`, poids du lien `contradicts`, phase Evolution (phase 1) |

Chaque phase de code = une section numerotee dans `docs/dev-notes/local-patches.md`
(prochain numero libre : 60 au 2026-10-17) + un fait stocke dans Kleos avec le tag
`delta-upstream`. Les corrections de la phase 1 sont des bugs upstream purs :
candidats a une PR vers `Ghost-Frame/Kleos` pour effacer le delta.

### Confidentialite

Le depot est public. Les fixtures ne contiennent que des decisions synthetiques.
Aucun extrait de transcript reel, aucun hote, aucune IP dans ce fichier ni dans les
fixtures committees. Les mesures brutes vont dans `docs/dev-notes/` (gitignore).

---

## 3. Etat des lieux verifie dans le code (branche `ccr-bfdfcf62-1j7z1k`, base `2d14846`)

Legende : **V** = verifie par lecture directe dans cette session ;
**R** = rapporte par le sous-agent d'exploration, a reverifier sur le PC avant de
s'appuyer dessus.

### 3.1 La chaine de supersession existe mais n'est pas raccordee

| # | Constat | Ou | Statut |
|---|---|---|---|
| 1 | `memories.is_superseded` est ecrit par `correction.rs:52` et `duplicates.rs:91`, mais **aucun chemin de recall ne le filtre** (FTS `fts.rs:206-209`, vecteur `vector.rs:74-76`, hydratation `search.rs:584-587`, contexte statique `context/deps.rs:116`, `/recall` `memory/mod.rs:1271`). | kleos-lib | V |
| 2 | `correct_memory` laisse l'original en `is_latest = 1` : l'original **et** sa correction sont servis. | `intelligence/correction.rs:30-70` | V |
| 3 | `structured_facts.invalid_at` / `invalidated_by` existent, sont filtres en lecture (`facts_channel.rs:66`, `context/deps.rs:347`), mais **rien ne les ecrit**. | kleos-lib | V |
| 4 | Le header de `intelligence/temporal.rs:1-6` promet l'auto-invalidation du predecesseur sur meme subject+verb. Le code n'existe pas. | kleos-lib | V |
| 5 | `detect_contradictions` n'est appele que depuis les routes `/intelligence/contradictions` et le scheduler, **pas a l'ingestion**, contrairement au README ligne 39 et au Patch 37.1 de `local-patches.md`. | kleos-server `routes/intelligence/mod.rs:10,65-73` | V |
| 6 | Son seul effet est un lien `contradicts` (`contradiction.rs:146-149`). L'expansion de graphe suit tous les types de liens (`search.rs:529-535,637-642`) et `link_type_weight` donne **1.3** a `contradicts` (`scoring.rs:686`) : une contradiction detectee **augmente** la co-recuperation de l'ancien fait. | kleos-lib | V |
| 7 | La phase Evolution du contexte injecte toute la chaine de versions, anciennes incluses, en clair (`context/mod.rs:885`). `get_version_chain` (`deps.rs:163`) ne filtre ni `is_forgotten` ni `is_archived`. | kleos-lib | V (ligne) / R (filtres) |
| 8 | `contradiction_penalty` (`scoring.rs:627`) ne s'applique qu'a `!is_latest`, deja exclu par SQL : jamais actif en pratique. | kleos-lib | R |
| 9 | Dedup a l'ingestion = SimHash, distance de Hamming < 3 : renvoie l'id existant, ne supersede jamais. Deux phrases "gzip" / "zstd" passent comme deux memoires sans lien. | `routes/memory/mod.rs:578-605` | R |
| 10 | `duplicates.rs:56-124` marque `is_superseded = 1` la memoire de **moindre importance** d'une paire a similarite >= seuil. Si la memoire la plus recente a la moindre importance, c'est la **nouvelle** decision qui disparaitra une fois le filtre de la phase 1 pose. A mesurer avant la phase 1 (section 4.3). | kleos-lib | R |
| 11 | `reconsolidation.rs:85-96` compte les liens entrants `corrects / updates / contradicts` et baisse `confidence` et `importance`, mais `confidence` et `importance` n'entrent pas dans le score de recherche (`search.rs:1186-1198`). | kleos-lib | R |
| 12 | Les atomes de handoff ont un vrai cycle `active / superseded / contested`, respecte par `get_packed_context` (`handoffs/mod.rs:880-883`), mais pose a la main. | kleos-lib | R |
| 13 | La seule defense en prompt : "If a memory contradicts the current conversation, prefer the conversation" (`prompts.rs:128`). Ne couvre pas une session neuve. | kleos-lib | V |
| 13b | La section "Resolved Contradictions (brain settled these conflicts)" du prompt vivant (`prompts.rs:332-345`, rendue par `build_living_prompt`) est **vide par construction**. Trois morceaux existent et chacun attend l'autre : (a) `POST /prompt/generate` (`kleos-server/src/routes/prompts/mod.rs:489-518`) lit `task_result.contradictions` et construit des paires gagnant / perdant ; (b) `services/brain.rs:939` renvoie toujours `contradictions: Vec::new()` avec le commentaire "done by the intelligence layer" ; (c) la couche intelligence ne produit que des liens `contradicts` dans `memory_links`, que rien ne reconvertit en paires. | kleos-lib, kleos-server | V |
| 13c | **Chiasm et Broca ne sont pas impliques.** Chiasm = routage de taches multi-agents (`services/chiasm/` : `chiasm_tasks`, claims, file, heartbeats, cles). Broca = journal d'actions et narration (`services/broca.rs` : `broca_actions`). Aucun des deux ne lit ni n'ecrit `memories`, `structured_facts` ou `memory_links`, et aucune reference a contradiction / supersession dans leurs modules. Ne pas chercher la detection de ce cote. | kleos-lib | V |
| 13d | Le seul code qui tranche reellement entre deux memoires contradictoires est `brain/hopfield/interference.rs:25-40` (feature `brain_hopfield`) : gagnant x1.2, perdant x0.1 sur des aretes `brain_edges` de type `contradiction`. Aucun appelant ne cree ces aretes a partir du contenu (`store_edge` n'ecrit que Association / Causal). Verifier sur le PC si la feature est compilee dans le binaire deploye. | kleos-lib | R |
| 14 | Aucune provenance "dit par l'utilisateur" / "infere" sur `memories` (seulement `source`, `model`, `status`). | schema | R |
| 15 | Aucune ingestion git. `kleos-ingest` suit `~/.claude/projects/**/*.jsonl` (`config.rs:16-22`) et classe par classes lexicon `ingest_decision_markers` / `ingest_preference_markers` (`extractor.rs:68-75`, `kleos-lib/lexicon/{fr,en,de}.toml:485-489`). | kleos-ingest | V |

### 3.2 Chemins d'injection reels sur le PC (a confirmer en phase 0)

- Par prompt (**corrige le 2026-10-18 apres mesure**) : le hook actif n'est pas
  `hooks/full/user-prompt-lean.sh` mais le port Rust `claude-hooks user-prompt-lean`
  (configuration utilisateur, hors depot). Cascade : kleos-cache local
  `POST /v1/retrieve` (`top_k: 3`) > sidecar `POST /recall` (`limit: 8`) >
  `kleos-cli context --limit 8`. kleos-cache sert l'immense majorite des prompts
  (206 sur 219 appels logges). kleos-cache est un depot separe qui replique les
  memoires via `GET /list` et garde une ligne si
  `!is_archived && !is_forgotten && is_latest` (`kleos-cache/src/sync_worker.rs:468`) :
  il **ignore `is_superseded`**. Un filtre pose seulement dans kleos-lib ne ferme
  donc pas la fuite sur ce canal (voir 1g).
- Debut de session : `~/.claude/hooks/session-start-kleos.sh` (script maison ; le
  passage par Eidolon est mesure sur 30 demarrages sur 30) appelle Eidolon
  `/prompt/generate`, puis `kleos-cli list` / `kleos-cli context ... --limit 8` en
  repli (lignes 319-430). `/prompt/generate` est servi par
  `kleos-server/src/routes/prompts/mod.rs:155` et rend `build_living_prompt`
  (`prompts.rs:353`), donc la section "Resolved Contradictions" du constat 13b
  **atteindrait le modele en debut de session si elle etait alimentee**.
- Hors hooks : `POST /search`, `POST /context`, `POST /recall` cote serveur.

---

## 4. Phase 0 : mesurer l'existant sur le PC (aucun code)

Objectif : prouver la fuite sur des donnees reelles, fixer la ligne de base, et
construire le banc qui jugera les phases suivantes. Tout se fait sur une **copie**
de la base.

### 4.1 Prerequis

- [ ] `kleos-server` a jour sur la branche de travail, `kleos-ingest` et les hooks
      actifs, un espace de test dedie (ex. `KLEOS_SPACE=supersession-bench`).
      **Corrige le 2026-10-18** : `kleos-ingest` ne tourne nulle part, et un store
      dans un espace de test du serveur vif ecrit dans la base vive. Le banc tourne
      donc sur une **seconde instance `kleos-server`** (autre port, `KLEOS_DB_PATH`
      vers la copie), avec un `kleos-ingest` et un kleos-cache de test pointes
      dessus. Le serveur vif n'est jamais ecrit.
- [x] Copie de la base : les bases sont chiffrees (sqlcipher) et SQLCipher refuse
      `.backup` ("backup is not supported with encrypted databases"). Copie faite
      par `sqlcipher_export` depuis la source attachee en lecture seule, dans un
      dossier hors depot du LXC ; `integrity_check` = ok. Les memoires vivent dans le
      shard tenant (28 987 lignes, schema tenant v84), la base monolith n'en
      contient aucune (schema v102).
- [ ] Reverifier les constats marques **R** de la section 3.1, en notant les lignes
      exactes dans le journal.
- [x] Verifier si la feature `brain_hopfield` est compilee dans le binaire deploye
      (`cargo build` flags dans `CLAUDE.local.md` / `deploy/`) et si le hook de debut
      de session passe reellement par Eidolon `/prompt/generate` ou par le repli
      `kleos-cli` : cela decide ou la phase 4b doit brancher les conflits.
- [ ] Verifier s'il existe deja un jeu d'evaluation du recall dans le depot ou les
      dev-notes. S'il n'y en a pas, enregistrer 20 requetes de reference et leurs
      top-5 actuels avant tout patch (regression guard pour la phase 1).

### 4.2 Inventaire de la base (lecture seule)

```sql
-- served-but-superseded: the leak, in rows
SELECT COUNT(*) FROM memories WHERE is_superseded = 1 AND is_latest = 1
  AND is_forgotten = 0 AND is_archived = 0;

-- latent contradictions: same key, different object, both active
SELECT user_id, subject, predicate, COUNT(DISTINCT object) AS n
FROM structured_facts WHERE invalid_at IS NULL
GROUP BY user_id, subject, predicate HAVING n > 1 ORDER BY n DESC LIMIT 50;

-- invalid_at ever written?
SELECT COUNT(*) FROM structured_facts WHERE invalid_at IS NOT NULL;

-- link inventory
SELECT type, COUNT(*) FROM memory_links GROUP BY type;

-- dedup risk (constat 10): superseded row newer than its survivor
SELECT ml.target_id AS old_id, ml.source_id AS new_id
FROM memory_links ml JOIN memories o ON o.id = ml.target_id
JOIN memories s ON s.id = ml.source_id
WHERE ml.type = 'supersedes' AND o.is_superseded = 1 AND o.created_at > s.created_at;

-- decision memories by space and age
SELECT space_id, COUNT(*), MIN(created_at), MAX(created_at)
FROM memories WHERE category = 'decision' AND is_latest = 1 GROUP BY space_id;
```

Noter chaque resultat dans le journal. Si la requete sur `invalid_at` renvoie 0 et
la premiere renvoie > 0, le constat central est confirme.

### 4.3 Reproduction sur donnees reelles

**Corrige le 2026-10-18 (decision operateur)** : la requete "latent contradictions"
ne renvoie que du bruit d'extraction et la base ne contient aucune paire
"ancienne decision / nouvelle decision" marquee. Les seules supersessions reelles
sont les 81 paires de doublons de `duplicates.rs` encore servables (section 14).
La 4.3 mesure donc sur ces paires si la memoire **retiree** est servie ; la fuite
d'une decision changee se mesure uniquement sur la fixture 4.4.

- [ ] Prendre les 81 paires gardee / retiree servables (Q1 de la 4.2), sur
      l'instance de banc.
- [ ] Pour chacune, formuler une question **hors du vocabulaire exact** des deux
      memoires (muninn : la question en vocabulaire identique mesure la
      recherche, pas la memoire).
- [ ] Interroger les trois canaux reels : kleos-cache `POST /v1/retrieve`
      (`top_k: 3`), sidecar / serveur `POST /recall` (`limit: 8`), `POST /search` ;
      noter par id : retiree servie ? gardee servie ? rang de chacune ?
- [ ] Meme chose en debut de session via `/prompt/generate`.

Resultat 2026-10-18 (instance de banc, copie figee, 81 questions hors vocabulaire,
comparaison exacte d'id, `limit: 8`, espace de la paire) :

| Canal | retiree servie | gardee servie | les deux | retiree avant gardee | aucune |
|---|---|---|---|---|---|
| `POST /recall` (= `kleos-cli context`) | 45 / 81 | 46 / 81 | 31 | 20 | 21 |
| `POST /search` | 64 / 81 | 62 / 81 | 55 | 31 | 10 |

Sur les 13 paires dont la gardee n'est plus servable, la retiree est la seule
version servie dans 7 cas (`/recall`) et 7 cas (`/search`). kleos-cache et
`/prompt/generate` : pas encore mesures (kleos-cache de test a monter).

### 4.4 Banc synthetique (la fixture qui jugera les phases 2 a 5)

Construire dans `docs/dev-notes/supersession-bench/` (gitignore) :

- **30 paires** decision / remplacement, 15 en francais, 15 en anglais, reparties
  sur **5 formulations** (methode muninn v43, 6 paires par formulation) :
  1. valeur nue (`gzip` puis `zstd`) ;
  2. phrase complete ("on compresse en gzip" / "on passe a zstd") ;
  3. comparative ("zstd est plus rapide, on le prend") ;
  4. verbe de changement explicite ("changement de plan : zstd") ;
  5. mixte, avec le remplacement **sans aucun mot commun** avec l'original
     ("Vault pour les secrets" / "on migre vers AWS Secrets Manager"). muninn :
     23 paires sur 30 sont dans ce cas en conditions reelles.
- **30 paires de controle** : deux decisions sur des sujets differents, ou une
  decision puis une phrase qui en parle sans la changer ("on a teste gzip et
  zstd"). Elles ne doivent **jamais** etre superseedees.
- **10 scenarios "bruit"** : la paire separee par 10 autres decisions.
- Pour chaque paire : une question en formulation retenue (held-out), ecrite
  avant de lancer quoi que ce soit, et la **valeur attendue** + la **valeur
  interdite**.

Deux voies d'injection, a mesurer separement :

Les deux voies visent l'instance de banc (section 4.1), jamais le serveur vif.

- **Voie A, API** : `kleos-cli store` ou `POST /memory` dans l'espace de test, avec
  horodatage force (lundi / jeudi) si le serveur le permet, sinon ingestion
  espacee.
- **Voie B, transcript** : un `.jsonl` synthetique au format Claude Code depose
  dans le dossier surveille par `kleos-ingest`. C'est la voie reelle : elle passe
  par `extractor.rs` et ses classes lexicon. Verifier d'abord le format attendu par
  `tailer.rs` / `extractor.rs`.

### 4.5 Metriques (fixees avant la premiere mesure, pas apres)

| Metrique | Definition | Cible finale |
|---|---|---|
| `delivered_new` | la valeur attendue apparait dans le contexte injecte | monter |
| `delivered_old` | la valeur interdite apparait dans le contexte injecte | **0** apres phase 1 pour toute paire marquee |
| `rank_new < rank_old` | quand les deux sont servies | monter |
| `retired_ok` | la paire a ete marquee (fait invalide ou memoire superseedee) | phase 2+ |
| `false_retire` | une paire de controle a ete marquee | **0 / 30**, porte bloquante |
| `agent_ok` | en session reelle (`claude -p` avec hooks), la reponse contient la valeur attendue et pas l'interdite ; notation par regex, jamais par LLM | monter |
| `ctx_tokens` | taille du bloc injecte par prompt et en debut de session | a reporter, pas a optimiser avant la phase 4 |
| `hook_p95_ms` | latence du hook `user-prompt-lean.sh` | ne pas regresser |

Controles obligatoires :

- **Sans memoire** (hook desactive) : `agent_ok` doit etre proche de 0, sinon la
  fixture est fuyante (la question contient la reponse).
- **Rendu identique** (reference X1 de muninn) : quand on compare deux variantes de
  livraison, garder la mise en page fixe et ne changer que le mecanisme, sinon on
  mesure la presentation.
- Chaque mesure est faite sur **la meme copie de base**, restauree entre deux
  variantes.

### 4.6 Ligne de base a produire

Remplir ce tableau avant tout patch :

| Condition | delivered_new | delivered_old | retired_ok | false_retire | agent_ok | ctx_tokens |
|---|---|---|---|---|---|---|
| existant, voie A | 4 / 40 | 7 / 40 | 0 / 40 | 0 / 30 | 3 / 40 (controles 13 / 30) | ~97 (389 car.) |
| existant, voie B | 4 / 40 | 1 / 40 | 0 / 40 | 0 / 30 | 3 / 40 (controles 1 / 30) | ~103 (411 car.) |
| sans memoire | 0 / 40 | 0 / 40 | - | - | **0 / 40 (controles 0 / 30)** | 0 |

Mesure du 2026-10-18, instance de banc sur la copie figee, restauree entre voie A et voie B.
Lecture des colonnes :

- `/ 40` = 30 paires + 10 scenarios bruit ; `false_retire` porte sur les 30 controles.
- `delivered_*` = regex sur le bloc reellement injecte par `claude-hooks user-prompt-lean`,
  isole du kleos-cache et du sidecar, donc via son repli `kleos-cli context --limit 8`
  (3 lignes gardees). Le canal kleos-cache, dominant en usage reel, **n'est pas mesure** ici.
- Etat apres 9 (voie A) et 6 (voie B) cycles de `POST /intelligence/run`, le pipeline du
  dreamer ; avant ces cycles, la voie A donnait 1 / 30 et 7 / 30 sur les paires seules.
- `agent_ok` = `claude -p` sonnet, sans reglages utilisateur ni MCP ni outils, seul hook =
  le hook de prompt pointe sur le banc, meme consigne systeme fixe dans les trois conditions.
- `ctx_tokens` = caracteres du bloc memoire hors regles / 4 (approximation), mediane.
- `hook_p95_ms` : 1 415 ms (A), 1 466 ms (B), chemin CLI de repli.
- Voie B = **rejeu** des candidats exacts du dry-run `kleos-ingest` (meme corps,
  `POST /memories`, auth Bearer) : le daemon reel recoit 401, il envoie un en-tete
  `X-API-Key` que le serveur ne lit jamais (`kleos-ingest/src/writer.rs:72-74`). L'extracteur
  ne retient que 13 tours sur 240 (marqueurs lexicaux, 50 caracteres minimum).

Par formulation (voie A, paires, `delivered_new` / `delivered_old`) : 1 = 0 / 0,
2 = 1 / 1, 3 = 0 / 4, 4 = 0 / 1, 5 = 0 / 0. Les remplacements comparatifs et a verbe de
changement perdent contre l'original, plus proche du sujet de la question.

### 4.7 Verdict de la porte de la phase 0 (2026-10-18)

Les trois criteres de la section 11 sont remplis :

1. **Fuite sur donnees reelles : confirmee.** Les 81 memoires marquees `is_superseded`
   encore servables sortent dans 45 cas sur 81 (`/recall`) et 64 sur 81 (`/search`), dont
   20 et 31 fois avant leur gardee (section 4.3). Rien ne filtre le drapeau.
2. **`invalid_at` jamais ecrit : confirme** (0 sur 900 faits ; 0 fait extrait de la fixture).
3. **Fixture valide** : le controle sans memoire donne `agent_ok` = 0 / 70.

Reserves qui conditionnent la suite :

- Sur la fixture, aucune paire n'est jamais marquee (`retired_ok` = 0) : la phase 1 ne peut
  y etre jugee que sur des paires marquees a la main, comme le prevoit la section 5.
- La phase 1 commence par 1e : un filtre seul ferait disparaitre la version la plus
  recente dans 61 des 81 paires reelles, et le fait entier dans 13.
- Le canal kleos-cache (1g) reste a mesurer avec un kleos-cache de test pointe sur le banc.
- La voie B n'existe pas en production : `kleos-ingest` ne tourne nulle part, et son
  authentification par cle est refusee.
- `delivered_new` de base est tres bas (4 / 40) : le probleme de livraison (phase 4) pese
  plus que le probleme de retrait sur cette fixture, comme chez muninn.

---

## 5. Phase 1 : fermer la fuite (patch chirurgical, candidat upstream)

Principe muninn (reference X1) : ce qui paie est **retenir l'enregistrement
invalide et ne pas le servir**, pas un typage plus riche. Cette phase ne detecte
rien ; elle rend la marque existante effective.

- [x] **1a. Filtre `is_superseded = 0`** dans : `memory/fts.rs` (predicat
      `m.is_latest = 1`), `memory/vector.rs`, l'hydratation de `memory/search.rs`
      (c'est le point unique par lequel passent tous les canaux de la recherche
      hybride, graphe et faits compris), `context/deps.rs` (memoires statiques),
      `routes/memory/mod.rs` (`/recall`). Ajouter `is_superseded` a l'index
      `idx_memories_search` seulement si le plan de requete le reclame (mesurer
      avec `EXPLAIN QUERY PLAN`, voir l'experience muninn sur l'index `kind`).
- [x] **1b. `correct_memory`** : passer l'original en `is_latest = 0` et chainer la
      correction (`parent_memory_id`, `root_memory_id`, `version + 1`) selon le
      contrat de version deja utilise par la mise a jour de memoire. Verifier sur
      le PC comment `memory::update` cree les versions avant de choisir.
- [x] **1c. Poids du lien `contradicts`** dans `scoring.rs::link_type_weight` :
      ramener de 1.3 a 1.0 au plus. Ne pas suivre les liens `supersedes` du nouveau
      vers l'ancien dans l'expansion de graphe (couvert par 1a si l'hydratation
      filtre, a verifier par test).
- [x] **1d. Phase Evolution** (`context/mod.rs` autour de la ligne 885) : rendre la
      version courante, puis une ligne par version anterieure `replaces #id (date)`
      **sans son texte**. Filtrer `is_forgotten` / `is_archived` dans
      `get_version_chain`.
- [x] **1e. Dedup `duplicates.rs`** -- **obligatoire, a faire AVANT 1a** (mesure
      4.2 : 815 / 987 liens retirent la plus recente ; 13 des 81 retirees servables
      ont une gardee non servable). Changer le critere (garder la plus recente a
      importance egale, ou ne pas superseder quand les objets SVO different), et
      decider du sort des paires deja marquees. Tant que 1e n'est pas faite, 1a ne
      filtre une memoire retiree que si sa gardee reste servable.
      **Decision operateur 2026-10-18** : l'invariant "une memoire retiree a toujours un
      remplacant servable" est tenu a l'ecriture, pas a la lecture. Trigger SQLite pose
      par le canal overlay (Patch 41, zero delta upstream) : quand une memoire qui en
      remplace d'autres est **archivee**, ses cibles perdent `is_superseded` et
      redeviennent servies ; quand elle est **oubliee** (suppression volontaire), ses
      cibles restent masquees. Reparation ponctuelle idempotente des cas existants (9
      gardees archivees sur la copie ; les 4 chaines finissent sur une memoire
      servable). Le filtre de lecture reste `is_superseded = 0`. Avant d'ecrire le
      trigger : identifier qui archive les gardees.
- [x] **1f. Test de garantie** (equivalent du test de faute muninn) : un test
      d'integration qui insere une memoire, la supersede, puis interroge **chaque**
      canal (FTS, vecteur, graphe via lien, faits, contexte statique, evolution,
      `/recall`, `/search`, `/context`) et echoue si le texte superseede sort.
      Le test doit etre montre **rouge** en retirant un des filtres.
- [x] **1g. kleos-cache** (depot separe, ajoute le 2026-10-18) : c'est le canal
      dominant par prompt (section 3.2). Verifier si `GET /list` exclut deja ou
      expose `is_superseded` (`Memory::is_superseded`, `memory/types.rs:189`), puis
      faire respecter le drapeau par la replique (`sync_worker.rs:468`) et retirer
      de l'index local une ligne qui devient superseedee apres sa replication. Le
      test de garantie 1f doit couvrir `POST /v1/retrieve`.
- [ ] Mettre en coherence les textes faux : README ligne 39, Patch 37.1, header de
      `temporal.rs`.

Tests : `cargo test -p kleos-lib --lib memory` et `--lib context` (Windows :
`--features bundled-sqlite`), puis la fixture 4.4 : `delivered_old` doit tomber a
0 pour les paires que l'on marque **a la main** (`/intelligence/correct`).

Livre (commits locaux de la branche) : 1f `bef8a136` ; 1e Patch 60 `2f592b75` et
Patch 61 `0ea4b143` ; 1a Patch 62 `ae757473` ; 1d Patch 63 `57bba709` ; 1g Patch 64
`efd078bf` + `ed41d140` et kleos-cache `63abf63` ; 1b Patch 65 `9b526c43` + `d89fc2f1` ;
1c Patch 66 `0f9936ac`. Detail par patch : `docs/dev-notes/local-patches.md`.

### 5.1 Verdict de la porte de la phase 1 (2026-10-19)

Les trois criteres de la section 11 sont remplis :

1. **Test de garantie vert, et montre rouge sans filtre.** `supersession_surfaces`
   20 / 20 a HEAD. Rouge avant les patches : 10 / 10 a `bef8a136`. Rouge par mutation,
   une a la fois : sans le predicat de `fetch_memories_batch`, 4 tests (Patch 62) ;
   sans la cle `is_superseded` du JSON, `routes_memory_supersession` 0 / 2 (Patch 64,
   version du test a `efd078bf`, reecrite depuis par `ed41d140` et `9b526c43`) ;
   sans le filtre de la replique, `sync_worker` 2 echecs sur 37 (kleos-cache). Rouges
   avant correction : `correction_versioning` 5 / 5 (Patch 65), `link_weights` 1 / 2
   (Patch 66).
2. **`delivered_old = 0 / 40` sur les remplacements marques a la main.** Banc restaure
   (copie 20261018-082511), serveur construit depuis la branche avec les patches 60 a 66,
   voie A chargee, 40 remplacements (30 paires + 10 bruit) marques en SQL comme le fait
   `deduplicate` (`is_superseded = 1` + lien `supersedes` nouvelle -> ancienne). Le
   marquage passe par SQL et non par `/intelligence/correct`, qui depuis le Patch 65 cree
   une troisieme memoire au lieu de retirer l'ancienne.
3. **Requetes de reference sans regression.** `retrieval_eval` (`golden.json`, 9 requetes,
   FTS seul) : recall@5, recall@10, MRR et nDCG@10 a 1.00 sur chaque requete, a la base
   `ea6b34ec` comme a HEAD (tableaux par requete identiques, plancher stable 0.95 tenu).
   kleos-cache, `local_only`, meme banc marque, replique sans filtre contre replique avec
   filtre : `queries.jsonl` (40) hit@5 0.600 -> 0.625, MRR@5 0.502 -> 0.507 ;
   `real-queries-relabelled-content` (59) 0.458 et 0.340 inchanges. Aucun id attendu de
   ces jeux n'est retire sur le banc : le filtre ne peut retirer qu'un distracteur.

| Condition (voie A) | delivered_new | delivered_old | retired_ok | false_retire | controles new | replique : anciennes indexees | ctx_tokens | hook_p95_ms |
|---|---|---|---|---|---|---|---|---|
| phase 0, 4.6 (existant, apres 9 cycles dreamer) | 4 / 40 | 7 / 40 | 0 / 40 | 0 / 30 | 8 / 30 | non mesure | ~97 (389 car.) | 1 415 |
| phase 0, avant dreamer (`out-hook-existing-a`) | 5 / 40 | 8 / 40 | 0 / 40 | 0 / 30 | 7 / 30 | non mesure | ~97 (386 car.) | 1 681 |
| porte, patches 60-66, avant marquage | 5 / 40 | 8 / 40 | 0 / 40 | 0 / 30 | 7 / 30 | 40 / 40 | ~97 (386 car.) | 1 802 |
| porte, patches 60-66, apres marquage | 6 / 40 | **0 / 40** | 40 / 40 (a la main) | 0 / 30 | 10 / 30 | **0 / 40** | ~99 (394 car.) | 1 762 |

Lecture : meme hook isole et memes regex que 4.6 (`measure_hook.py`). Les mesures de la
porte sont faites sans cycle du dreamer : leur comparable est la ligne "avant dreamer"
de la phase 0, que la ligne "avant marquage" reproduit a l'identique (memes livraisons
par scenario). `retired_ok` apres marquage compte le marquage manuel, pas une detection ;
`false_retire` reste 0 / 30 parce qu'aucun controle n'est marque. `hook_p95_ms` vaut 1 681 ms
(binaire de la phase 0) contre 1 802 puis 1 762 ms (patches 60-66) sur le meme etat : ecart
non attribue, la variance entre deux runs d'un meme binaire n'a pas ete mesuree, et le
chemin du hook (`/context`) porte les predicats des Patches 62 et 63. Replique kleos-cache de test sur le banc, Ollama
`bge-m3` : la premiere passe apres marquage ne supprime rien (regle d'absence a deux
passes), la seconde supprime les 40 ; ensuite 0 des 1 009 memoires retirees du banc
n'est indexee, les 40 nouvelles le restent. Sorties brutes (hash du binaire, port,
comptes, passes de synchronisation) : `gate1-bench-evidence.txt`.

Reserves :

- `retrieval_eval` est sature (tout a 1.00, base et HEAD) et ne mesure pas le canal
  vecteur : signal de non-regression complet mais faible ; `golden.json` compte 9
  requetes, pas 20.
- La replique "sans filtre" est le binaire kleos-cache du 2026-09-24 : hors filtre, sa
  projection differe dans les deux sens (132 ids indexes par le binaire 1g et pas par
  lui, dont 10 nouvelles versions de la fixture ; 148 dans l'autre sens, dont 100
  retirees). La comparaison n'isole pas le seul filtre. Le binaire 1g mesure inclut le WIP non commite du depot kleos-cache
  (`main.rs`, `http.rs`, `lib.rs`), hors `sync_worker.rs`.
- `delivered_new` reste bas (6 / 40) : retirer l'ancienne version ne fait pas livrer la
  nouvelle. C'est l'objet des phases 2 a 4.
- Reste de la phase 1, hors porte : les textes faux (README ligne 39, Patch 37.1, header
  de `temporal.rs`), et `pagerank.rs` / `communities.rs` qui gardent `contradicts` a 1.3.

---

## 6. Phase 2 : supersession a l'ingestion sur `structured_facts` (additif)

C'est la regle 1 de muninn (meme cle, objet different), appliquee a la structure
que Kleos extrait deja. Le filtre de lecture existe : le cout au recall est nul.

Regle, au moment ou les faits de la nouvelle memoire sont inseres (apres
`fast_extract_facts`, `extraction.rs:471`, dans le chemin de store) :

```
for each new fact (subject, predicate, object):
  old = active facts with same user_id, same space, same normalized (subject, predicate),
        object != new.object, invalid_at IS NULL, memory != new memory
  if guards pass:
    UPDATE structured_facts SET invalid_at = now, invalidated_by = <new fact or memory id>
    (verify on the PC what invalidated_by references: fact id or memory id)
    INSERT memory_links (new_mem -> old_mem, 'supersedes')
    INSERT reconsolidations (old_mem, old.object, new.object, 'fact_superseded')
    if old memory has exactly one active fact (or is a decomposed child, is_fact = 1):
      UPDATE memories SET is_superseded = 1 WHERE id = old_mem
```

Gardes (toutes necessaires, un faux retrait est le pire resultat) :

- meme `user_id`, meme espace (Patch 37 impose deja la contrainte d'espace) ;
- la nouvelle memoire vient d'un canal "utilisateur" (`kleos-ingest` tour user,
  `kleos-cli store`, MCP store), jamais de `growth`, `insight`, `consolidation`,
  `dreamer-*` ni d'un tour assistant ;
- `object` normalise reellement different (casse, accents, pluriel : reutiliser
  la normalisation du lexicon) ;
- une question ("on passe a zstd ?") ne supersede pas : detection du `?` et des
  formes interrogatives ;
- negation devant un marqueur ("on ne change pas", "no change") : pas de
  supersession ;
- une memoire multi-faits n'est pas marquee `is_superseded` : seul le fait l'est.
- idempotence : rejouer la meme ingestion ne change rien (l'index unique
  `structured_facts_unique_index` de l'overlay VOCSAP aide).

Optionnel : remplir `current_state.previous_value / previous_memory_id` que les
upserts actuels ecrasent (`extraction.rs:723-726`, `facts.rs:214-218`).

Mesure : fixture 4.4, voies A et B. Porte : `false_retire = 0 / 30`,
`retired_ok` > ligne de base, `delivered_old = 0` sur les paires retirees. Attendu
d'apres muninn : cette regle seule atteint peu de paires de la formulation 5 (pas de
mot commun) ; c'est normal, la phase 4 compense.

---

## 7. Phase 3 : marqueurs de changement et accuse de reception (lexicon + additif)

Deux signaux supplementaires, toujours deterministes.

### 7.1 Classe lexicon `ingest_change_markers`

- Nouvelle classe dans `kleos-lib/lexicon/{fr,en,de}.toml` a cote de
  `ingest_decision_markers`, surchargeable dans `lexicon-overrides/` (hot reload
  5 s). Une nouvelle classe exige un site Rust (CLAUDE.md, Patch 38) : le site est
  `kleos-ingest/src/extractor.rs` (tag `change`) et la regle de la phase 2.
- Contenu a deriver de nos transcripts (phase 0), pas de muninn. Pistes FR :
  "changement de plan", "finalement", "en fait", "on passe a", "desormais",
  "a partir de maintenant", "au lieu de", "plutot que", "on abandonne", "on
  revient a", "laisse tomber", "oublie", "annule", "plus de". Pistes EN : "change
  of plan", "switch to", "instead", "no longer", "from now on", "going forward",
  "revert to", "scratch that", "actually", "after all", "never mind".
- Garde negation ("pas de changement", "didn't switch") dans la meme classe ou
  une classe `ingest_change_negations`.

Effet sur la regle de la phase 2 : quand la nouvelle memoire porte le tag `change`,
autoriser la supersession sur une cle **partielle** : meme `subject` seul, ou au
moins 2 mots de contenu partages avec un taux de recouvrement >= 0.34 (parametres
publies par muninn ; sans marqueur, 0.5). Toujours une seule memoire retiree : la
plus recente qui satisfait la regle, et jamais une memoire elle-meme marquee
`change` (sinon une rafale de changements se retire en chaine).

### 7.2 Accuse de reception de l'assistant

`kleos-ingest` voit les tours assistant. Quand le tour qui suit un tour user tague
`change` contient une forme "de X a Y" / "from X to Y" / "X au lieu de Y" / "replace
X with Y", et que Y figure dans ce que l'utilisateur a dit : chercher le fait actif
dont l'objet contient X comme mot entier, retirer **le plus court** et un seul.
muninn observe que ce signal n'apparait que si l'ancienne decision a ete injectee
dans la session (27 % des cas avec injection, 0 sans) : il depend donc de la
phase 4.

Mesure : fixture 4.4 par formulation. Porte : `false_retire = 0 / 30`, gain visible
sur les formulations 2 a 4. Si les formulations 1 et 5 ne bougent pas, c'est
attendu.

---

## 8. Phase 4 : livraison (catalogue, conflits explicites)

Lecon principale de muninn : un agent qui recoit une selection filtree ne
distingue pas "la memoire n'a rien" de "la requete a rate", et il ecrit "aucune
decision enregistree" avec la bonne decision sous les yeux.

- [ ] **4a. Catalogue par espace** dans `POST /context` (et donc dans le repli
      `kleos-cli context` du hook de debut de session) et, si le budget le permet,
      dans le `/recall` du sidecar : une ligne par memoire active de categorie
      `decision` (et les statiques d'importance >= 8), la plus recente d'abord :
      `#id · date · premiere ligne · replaces #n · conflict with #m`. Le texte d'une
      memoire superseedee n'est **jamais** rendu, seul son id. Derniere ligne : "liste
      complete" ou "N plus recentes" (sans compter le reste si le compte coute).
      Limite configurable (`KLEOS_CATALOG_LIMIT`, defaut a fixer apres mesure de
      `ctx_tokens`).
- [ ] **4b. Conflits actifs : raccorder les trois morceaux du constat 13b.** Deux
      memoires actives liees par `contradicts` sont servies toutes les deux et
      marquees ; le ranking ne tranche jamais (reference N4 : la frontiere qui tient
      est la lecture). Travail, du plus bas niveau au plus haut :
      1. `services/brain.rs:939` : au lieu de `Vec::new()`, renvoyer les paires
         `(a, b)` ou `a` et `b` sont toutes deux dans `activated` et liees par un
         `memory_links.type = 'contradicts'` (meme user, meme espace). Patch
         chirurgical, le format `BrainContradiction { winner_id, loser_id, reason }`
         existe deja.
      2. Choix de `winner` / `loser` : **ne pas trancher par score**. Si l'une des
         deux porte `is_superseded = 1` ou un fait invalide (phases 1 et 2), elle
         est `loser` et `reason = 'superseded'` ; sinon les deux sont actives et la
         paire est rendue comme **conflit non resolu**, ce qui demande d'ajuster le
         gabarit de `prompts.rs:332-345` ("brain settled these conflicts" devient
         faux). Verifier si ce gabarit est surchargeable via `prompts-overrides`
         (catalogue des prompts) ; sinon patch chirurgical du texte.
      3. Meme information dans `POST /context` et le `/recall` du sidecar (ligne
         `conflict with #m` du catalogue 4a), car le repli `kleos-cli context` ne
         passe pas par le prompt vivant.
      4. Ne pas toucher a `brain/hopfield/interference.rs` : son arbitrage par
         score est exactement ce que la reference N4 mesure comme defaillant. Si
         la feature est compilee, verifier qu'aucune arete `contradiction` n'est
         creee par la phase 2 ; sinon la memoire remplacee serait attenuee a x0.1
         **et** marquee, ce qui est redondant mais pas nuisible. A noter, pas a
         construire.
      Mesure : la section apparait dans `/prompt/generate` pour une paire de la
      fixture ; `delivered_old` reste 0 pour une paire superseedee ; une paire
      reellement en conflit est rendue avec ses deux valeurs et marquee.
- [ ] **4c. Cadre epistemique** : le bloc memoire est presente comme evidence, pas
      comme instruction ; les memoires issues de `growth` / `insight` / assistant
      portent une marque "a verifier". Le texte du cadre passe par
      `prompts-overrides` si l'id est surchargeable (a verifier dans le catalogue
      des prompts), sinon patch chirurgical.
- [ ] **4d. Mesure a rendu fixe** : comparer "catalogue on / off" sur la meme base,
      meme mise en page, et reporter `agent_ok`, `ctx_tokens`, `hook_p95_ms`. muninn
      paie environ 2,4 fois le contexte d'un concurrent pour ce gain ; decider en
      connaissance de cause, et garder un interrupteur.

Porte : `agent_ok` monte a rendu fixe ; `delivered_old` reste a 0 ; cout contexte
reporte dans le journal.

---

## 9. Phase 5 : retrait par les commits git (additif, apres les phases 1 a 4)

Le seul levier non lexical. Chez muninn il fait passer le retrait de 17 a 29 sur
30 et reste a 0 faux retrait sur 30, mais ne couvre que les decisions qui
atteignent un fichier.

Esquisse (a ecrire dans nos termes) :

- Declencheur : hook `post-commit` installe par `scripts/install-git-hooks.sh`
  (a etendre), ou `kleos-ingest` qui lit `git log` du projet surveille a intervalle.
  Jamais dans un hook de lecture. Jamais sur l'arbre de travail non committe.
- Pour chaque commit nouveau : parser le diff ; ne garder que les hunks ou
  **une ligne supprimee devient une ligne ajoutee** ; tokens presents dans
  l'ancienne et absents de la nouvelle = candidats "valeur partie".
- Un candidat ne compte que si un fait actif (`structured_facts.object`) ou une
  memoire `decision` active le contient comme **mot entier**. Le diff ne cree
  jamais de sujet.
- Gardes : un token present dans plus de 3 fichiers suivis (`git grep -l`, compte
  borne) est du vocabulaire, pas une valeur, sauf s'il a une forme de nom
  (majuscule interieure, chiffre, point, tiret) ; un fichier supprime ou renomme ne
  retire rien ; un commit qui ne retire rien n'ecrit rien.
- Effet : fait invalide avec raison `commit_swapped`, nouvelle memoire
  `decision` source `git-commit` contenant fichier, ligne avant / apres, hash,
  liee par `supersedes`. Quantites : meme unite, nombre different = meme regle.
- Regle de disparition (mot retire et absent de tout l'arbre) en second temps,
  bornee en nombre de `git grep`.

Mesure : fixture dediee de 30 paires dont le remplacement est aussi dans un
commit au sujet volontairement muet ("update dependencies"), plus 30 commits de
controle touchant une valeur qu'aucune decision ne nomme. Porte : `false_retire =
0 / 30` dans toutes les conditions.

---

## 10. Phase 6 (optionnelle) : provenance `origin`

Colonne `origin` sur `memories` (`user_said`, `assistant_ack`, `inferred`,
`imported`, `commit_linked`) via l'overlay `VOCSAP_MONOLITH_OVERLAYS` /
`VOCSAP_TENANT_OVERLAYS` (Patch 41, zero conflit), posee a l'ingestion. Usages :
ne jamais couper une memoire `user_said` au budget ; cadrer `inferred` comme indice
(phase 4c) ; restreindre les gardes de la phase 2 a `user_said`. A faire seulement
si la phase 2 montre des retraits provoques par des memoires non utilisateur.

---

## 11. Portes de decision

| Apres | Go si | Sinon |
|---|---|---|
| Phase 0 | la ligne de base montre la fuite (`delivered_old` > 0 sur des paires reelles, `invalid_at` jamais ecrit) et la fixture est validee par le controle sans memoire | reexaminer les constats R, corriger le plan |
| Phase 1 | test de garantie vert et montre rouge sans filtre ; `delivered_old = 0` sur paires marquees a la main ; les 20 requetes de reference ne regressent pas | ne pas passer a la phase 2 |
| Phase 2 | `false_retire = 0 / 30` ; `retired_ok` monte ; idempotence prouvee | revoir les gardes, pas les assouplir |
| Phase 3 | `false_retire = 0 / 30` ; gain par formulation documente | garder la classe lexicon, retirer la regle |
| Phase 4 | `agent_ok` monte a rendu fixe ; `ctx_tokens` et `hook_p95_ms` reportes et acceptes | garder l'interrupteur a off |
| Phase 5 | `false_retire = 0 / 30` sur toutes les conditions | ne pas activer par defaut |

---

## 12. Ce que l'on ne construit pas, avec la raison mesuree chez muninn

| Tentation | Pourquoi non | Reference |
|---|---|---|
| Un juge LLM a l'ecriture pour decider **seul** ce qu'un message remplace | trouve le bon enregistrement mais marque aussi les voisins ; 46 / 54 avec juge contre 48 / 54 sans ; runtime de 6 Go. **Amende le 2026-10-18 (decision operateur)** : un juge de classe Sonnet / Haiku 5.5, servi par LiteLLM, devient une **variante mesuree** (phase 3b), desactivee par defaut. Forme retenue : agent dedie a prompt systeme et outils (lecture des memoires et faits, un seul outil d'ecriture qui *propose* un conflit ou une supersession avec ids et valeurs citees), applique par un validateur deterministe (meme user, meme espace, deux actives, nouvelle plus recente, canal utilisateur). Porte : `false_retire = 0 / 30` sur la fixture, comparaison a rendu fixe avec les regles des phases 2-3. Hebergement local : boucle d'outils via LiteLLM (recommande), workflow Dify (exploration). Modele de fonctionnement repris de l'automatisation de reference Anthropic (claude.dev, "Building effective agent automations", 2026-10-08) : marque-page par source, lecture ratee jamais prise pour une journee calme, reverification avant ecriture, ecriture comptee seulement si confirmee, preferences relues en lecture seule a chaque passage, plafond de depense par passage | Z11 |
| La consolidation / reflexion LLM comme mecanisme de contradiction | la memoire consolidee passe sous la ligne "sans memoire" ; fusionne des strategies superficiellement proches et incompatibles | C1, K10 |
| Un ledger bi-temporel fin, etats "non prouvable", force d'evidence | a rendu egal, indistinguable de zero ; l'invalidation grossiere bat le ledger fin de 0,084 | X1 |
| Trancher un conflit entre deux actifs par le ranking | les cinq systemes mesures servent la politique revoquee en premier dans 42 a 44 % des essais ; la frontiere qui tient est **la lecture**, pas l'outil | K7 / N4 |
| Decroissance type Ebbinghaus comme reponse au changement d'avis | aucune mesure ne la reclame ; Kleos a deja FSRS pour un autre usage | P2 |
| Supprimer les enregistrements retires | ils restent en base, quittent l'index, ne sont jamais servis ; `--all` les montre | scope.md |
| Vecteurs dans le chemin chaud du hook | non deterministe entre deux runs sur 80 % des requetes ; cout de chargement | I2, V4 |

---

## 13. Livrables et documentation

- [ ] Une section par phase de code dans `docs/dev-notes/local-patches.md`
      (Patch 60+) : symptome sous upstream pur, approche et pourquoi pas le niveau
      en dessous, fichiers, tests, conditions de retrait.
- [ ] Un fait Kleos par patch avec le tag `delta-upstream`.
- [ ] PR upstream candidate pour la phase 1 (bugs purs : filtre `is_superseded`,
      `correct_memory`, poids `contradicts`, Evolution).
- [ ] Mesures brutes et fixtures dans `docs/dev-notes/supersession-bench/`
      (gitignore) ; ce fichier ne recoit que les tableaux de synthese.
- [ ] Corriger README ligne 39, Patch 37.1, header `temporal.rs`.
- [ ] Mise a jour de ce plan : statut des cases, journal.

---

## 14. Journal

Format : `date · phase · condition · resultat · fichier de mesure`.

| Date | Phase | Condition | Resultat | Source |
|---|---|---|---|---|
| 2026-10-17 | redaction | lecture muninn 1.0.5 + audit code Kleos | constats section 3 | cette session |
| 2026-10-17 | redaction | question : Chiasm / Broca appellent-ils la detection ? | non (13c) ; plomberie exacte du prompt vivant (13b) ; phase 4b reecrite | cette session |
| 2026-10-18 | 0 / 4.1 | inventaire serveur | `/health` = 1.10.0 ; binaire deploye 2026-10-11 ; depuis, seuls des commits `cred` touchent kleos-lib / kleos-server (chemins de recall identiques a la branche) | session PC |
| 2026-10-18 | 0 / 4.1 | chiffrement | `KLEOS_ENCRYPTION_MODE=env` ; en-tete SQLite absent sur la base monolith et sur le shard tenant 2 : les deux sont chiffrees (sqlcipher) | session PC |
| 2026-10-18 | 0 / 4.1 | `kleos-ingest` | aucun processus sur le PC, aucune unite sur le LXC : la voie B n'a pas de consommateur actif | session PC |
| 2026-10-18 | 0 / 4.1 | hook par prompt | `claude-hooks user-prompt-lean` (port Rust, pas `hooks/full/user-prompt-lean.sh`) : cascade kleos-cache `POST /v1/retrieve` (`top_k: 3`) > sidecar `/recall` (`limit: 8`) > `kleos-cli context --limit 8`. Sur les 2000 dernieres lignes de log : 206 CACHE HIT, 12 TIMEOUT, 1 DOWN | session PC |
| 2026-10-18 | 0 / 4.1 | kleos-cache | replique via `/list`, garde une ligne si `!is_archived && !is_forgotten && is_latest` (`kleos-cache/src/sync_worker.rs:468`), ignore `is_superseded` | session PC |
| 2026-10-18 | 0 / 4.1 | hook debut de session | `session-start-kleos.sh` passe par Eidolon `/prompt/generate` sur 30 / 30 demarrages logges ; le repli `kleos-cli` n'a pas servi | session PC |
| 2026-10-18 | 0 / 4.1 | `brain_hopfield` | compilee : `kleos-server/Cargo.toml:31` l'active explicitement ; les chemins `brain/hopfield/*.rs` figurent dans le binaire deploye | session PC |
| 2026-10-18 | 0 / 4.1 | espace de test | aucun espace `supersession-bench` ; 40 espaces existants, dont des espaces de test de patchs anterieurs | session PC |
| 2026-10-18 | 0 / 4.1 | copie de base | `.backup` refuse par SQLCipher ; `sqlcipher_export` depuis la source attachee `mode=ro` ; `integrity_check` ok sur les deux copies ; tenant : 28 987 memoires, schema v84 ; monolith : 0 memoire, schema v102 | session PC |
| 2026-10-18 | 0 / 4.1 | decisions operateur | banc sur une seconde instance pointee sur la copie ; kleos-cache ajoute au perimetre de la phase 1 (1g) ; plan corrige en 3.2, 4.1, 4.4, 5 | session PC |
| 2026-10-18 | 0 / 4.1 | jeu d'evaluation existant | `kleos-lib/tests/fixtures/retrieval/golden.json` ; `kleos-cache/eval/` (requetes reelles, ligne de base remesuree le 2026-09-22) : candidats au garde de regression de la phase 1 | session PC |
| 2026-10-18 | 0 / 3.1 | reverif. constat 10 (R) | confirme : `duplicates.rs:76-84` garde l'importance la plus haute, a egalite **la plus ancienne** (id le plus bas) ; lien `supersedes` garde -> retiree (`duplicates.rs:104-106`) | code branche |
| 2026-10-18 | 0 / 4.2 | Q1 servie-mais-superseedee | **81** memoires `is_superseded = 1` encore servables (plus 17 archivees, 889 oubliees) | copie tenant, out-4.2.txt |
| 2026-10-18 | 0 / 4.2 | Q3 `invalid_at` ecrit | **0** sur 900 `structured_facts` : constat 3 confirme | idem |
| 2026-10-18 | 0 / 4.2 | Q2 contradictions latentes | 15 cles, toutes `subject = user` avec un predicat verbal (`fait` : 703 objets distincts) : bruit d'extraction, **aucune paire de decision exploitable** | idem |
| 2026-10-18 | 0 / 4.2 | Q4 liens | `similarity` 87 350, `consolidates` 3 912, `supersedes` 987 ; **0 `contradicts`**, 0 `corrects`, 0 `updates` | idem |
| 2026-10-18 | 0 / 4.2 | Q5 risque dedup | **815 / 987** liens `supersedes` retirent la memoire la plus recente | idem |
| 2026-10-18 | 0 / 4.2 | Q6 decisions actives | 1 395 memoires `decision` sur 19 espaces (+ 51 sans espace) | idem |
| 2026-10-18 | 0 / 3.1 | reverif. constat 7 (R) | confirme : `get_version_chain` ne filtre que `user_id` et `status != 'pending'` (`context/deps.rs:172-176`), ni `is_forgotten` ni `is_archived` | code branche |
| 2026-10-18 | 0 / 3.1 | reverif. constat 8 (R) | confirme dans le code : `contradiction_penalty` rend 1.0 des que `is_latest` (`memory/scoring.rs:627-645`), appele en `memory/search.rs:1186` ; ses marqueurs sont anglais seulement | code branche |
| 2026-10-18 | 0 / 3.1 | reverif. constat 9 (R) | confirme en pratique : un texte identique stocke deux fois (fillers N06 / N10) renvoie l'id existant, sans lien ni supersession | load-a.log |
| 2026-10-18 | 0 / 4.4 | fixture | 70 scenarios synthetiques valides (30 paires 3 par formulation et par langue, 30 controles, 10 bruit) ; aucune question ne contient la valeur attendue ni interdite | fixture.json |
| 2026-10-18 | 0 / 4.4 | voie A, etat apres store | 220 memoires dans l'espace de test (fillers identiques dedoublonnes), 0 superseedee, **0 `structured_facts` extrait**, 0 lien | out-4.4 SQL |
| 2026-10-18 | 0 / 4.4 | voie A, pipeline dreamer x 9 (`POST /intelligence/run`) | 1 242 liens `similarity`, 0 `contradicts`, 0 `supersedes`, 0 retrait dans l'espace de test (1 fusion ailleurs dans le tenant) | supersession-run-a.json |
| 2026-10-18 | 0 / 4.4 | voie A, livraison par le hook | apres dreamer : paires 1 / 30 nouvelle, 6 / 30 ancienne ; bruit 3 / 10 et 1 / 10 ; controles 8 / 30 ; `/recall` brut (8) : paires 3 / 30 et 8 / 30 | out-hook-existing-a-dreamed.jsonl |
| 2026-10-18 | 0 / 4.4 | voie A, `agent_ok` | paires 1 / 30 (ancienne citee 10 / 30), bruit 2 / 10, controles 13 / 30 ; premier run tue par manque de RAM, relance a 2 sessions avec consigne systeme fixe (sans elle, le modele emettait des appels d'outils simules) | out-agent-existing-a.jsonl |
| 2026-10-18 | 0 / 4.4 | sans memoire, `agent_ok` | **0 / 70** : fixture non fuyante | out-agent-no-memory.jsonl |
| 2026-10-18 | 0 / 4.4 | voie B, `kleos-ingest` | crate Linux-only, compile en release dans WSL ; dry-run : 13 tours retenus sur 240 ; envoi reel : 401 sur les 14 (en-tete `X-API-Key` jamais lu par le serveur) ; rejeu des candidats en Bearer sur copie restauree | ingest-b-candidates.log |
| 2026-10-18 | 0 / 4.4 | voie B, mesures | 13 memoires, 0 retrait, 0 fait, 40 liens `similarity` ; hook : paires 3 / 30 et 1 / 30 ; `agent_ok` paires 2 / 30, bruit 1 / 10, controles 1 / 30 | out-hook-existing-b-dreamed.jsonl, out-agent-existing-b.jsonl |
| 2026-10-18 | 0 / 4.7 | porte de la phase 0 | criteres remplis (voir 4.7), validee par l'operateur ; branche poussee a `ea6b34ec` | cette session |
| 2026-10-18 | 1 / 1f | test de garantie, rouge | `kleos-lib/tests/supersession_surfaces.rs`, 10 tests, **10 rouges** sur l'assertion "retiree servie" : `hybrid_search` (resultats et payload `linked`), voisin de graphe, canal des faits, contexte statique et recent, `/pack`, `generate_prompt`, `graph_search`, recherche par tags, phase Evolution (texte de v1 injecte, constat 7 confirme a l'execution). Les controles positifs passent. Surfaces ajoutees au-dela de 1a : celles du gate "pending" (tags, pack, prompt, graph search). Premier essai du test Evolution vert a tort : une memoire statique n'est pas un bloc semantique, la phase ne tournait pas (`context/mod.rs:847-853`) | spec_01a1d168 |
| 2026-10-18 | 0 / 4.3 + 5 | decision operateur | 4.3 reecrite sur les 81 paires reelles ; 1e obligatoire et avant 1a | session PC |
| 2026-10-18 | 0 / 4.1 | instance de banc | `kleos-server` sur `:4290`, donnees = copies 20261018-082511, dreamer / backup / pagerank / communautes coupes ; 429 au premier run (liste d'IP de confiance preauth absente du dossier de banc, `rate_limit.rs:59-86`), copiee puis run refait | session PC |
| 2026-10-18 | 0 / 4.3 | 81 paires reelles | retiree servie : `/recall` 45 / 81, `/search` 64 / 81 ; retiree classee avant la gardee : 20 et 31 ; population : des doublons, mais aussi de vraies corrections retirees au profit de l'ancienne version (ex. memoires qui disent "corrige / supersede #n" et qui sont elles-memes les retirees) | out-4.3-existing.jsonl |
| 2026-10-18 | 0 / 4.2 | detail des 81 (Q1) | les 81 viennent toutes de `duplicates.rs` (0 sans lien `supersedes`) ; 61 sont plus recentes que la gardee ; **13 ont une gardee elle-meme non servable** ; similarite 0.90 a 1.0, contenu identique sur 86 / 987 liens ; echantillon lu : reformulations du meme fait stockees a quelques secondes d'ecart, pas des changements de decision | out-4.2b.txt |
| 2026-10-19 | 1 / 5.1 | banc pour la porte | restauration 20261018-082511 (28 987 memoires), binaire patches 60-66 sha256 `7ebb0c10` ; reparation du Patch 61 appliquee a l'ouverture du shard : 969 retirees dont 70 projetables | gate1-counts.sql |
| 2026-10-19 | 1 / 5.1 | porte, `delivered_old` | voie A, 40 remplacements marques en SQL : hook 8 / 40 -> **0 / 40** ; replique kleos-cache 40 / 40 anciennes indexees -> **0 / 40** en deux passes, 0 des 1 009 retirees indexee | out-hook-gate1-unmarked.jsonl, out-hook-gate1-marked.jsonl |
| 2026-10-19 | 1 / 5.1 | porte, requetes de reference | `retrieval_eval` 9 / 9 a 1.00 a `ea6b34ec` et a HEAD ; kleos-cache `local_only` sans / avec filtre : 40 requetes hit@5 0.600 -> 0.625, 59 requetes inchangees | gate1-retrieval-base.log, gate1-retrieval-head.log, gate1-eval/ |

---

## 15. References

### muninn (AGPL, lecture seule)

- `README.md`, `docs/technical-overview.md` : mesures publiees.
- `docs/scope.md`, `docs/claims.md` : ce qui n'est pas fait et pourquoi.
- `design/ENGINE.md` section 5 (invalidation), 6.1 (catalogue), 12 (exclusions).
- `crates/muninn-core/src/schema.sql` : vue `served_record`, triggers FTS,
  `invalid_reason` enumere.
- `crates/muninn-capture/src/ingest.rs` : les quatre regles de supersession a
  l'ecriture et leurs gardes.
- `crates/muninn-cli/src/maintain.rs` : retrait par commits.
- `crates/muninn-core/src/recall.rs::catalog`, `filter.rs::conflicts_of`.

### Kleos (cette branche)

- `kleos-lib/src/db/schema_sql.rs` (memories 24-103, structured_facts 402-433,
  memory_links 106-113, reconsolidations 517-526, current_state 436-449).
- `kleos-lib/src/memory/{fts.rs, vector.rs, search.rs, scoring.rs, facts_channel.rs}`.
- `kleos-lib/src/context/{mod.rs, deps.rs}`, `kleos-lib/src/prompts.rs`.
- `kleos-server/src/routes/prompts/mod.rs` (`/prompt/generate`, construction des
  paires de contradiction), `kleos-lib/src/services/brain.rs` (vecteur vide ligne 939),
  `kleos-lib/src/brain/hopfield/interference.rs`.
- `kleos-lib/src/services/{chiasm/, broca.rs}` : hors perimetre (constat 13c).
- `kleos-lib/src/intelligence/{contradiction.rs, correction.rs, duplicates.rs,
  consolidation.rs, reconsolidation.rs, temporal.rs, extraction.rs, scheduler.rs}`.
- `kleos-ingest/src/{extractor.rs, tailer.rs, config.rs}`,
  `kleos-lib/lexicon/{fr,en,de}.toml`.
- `hooks/full/{user-prompt-lean.sh, session-start-kleos.sh}`.
- `kleos-lib/src/db/vocsap/mod.rs` (canal overlay schema, Patch 41).
- `CLAUDE.md`, `docs/dev-notes/local-patches.md` (Patches 37, 37.1, 38, 41).

---

## 16. Prompt de demarrage de la session dediee (a copier-coller)

```text
Session dediee : etude et implementation de la supersession des memoires dans Kleos (intention muninn).

Contexte
- Lis d'abord en entier docs/studies/2026-10-17-supersession-muninn-plan.md. C'est le plan de reference ; ne le reformule pas, applique-le.
- Lis ensuite CLAUDE.md, CLAUDE.local.md (chemins, hotes, procedure de deploiement) et les sections Patch 37, 37.1, 38 et 41 de docs/dev-notes/local-patches.md.
- Branche de travail : ccr-bfdfcf62-1j7z1k (a jour sur origin). Toute modification de code se fait sur cette branche.

Regles de la session
1. Phase 0 d'abord, sans aucune modification de code : mesurer l'existant. Les phases 1 a 6 ne demarrent que si la porte de decision de la phase precedente (section 11 du plan) est passee, et je valide explicitement chaque passage de phase.
2. Base de donnees : travailler uniquement sur une copie (sqlite3 ".backup"). Ne jamais ecrire dans la base vive. Si une base tenant est chiffree, me demander avant toute manipulation.
3. Avant de t'appuyer sur un constat marque R dans la section 3.1 du plan, reverifie-le dans le code et note la ligne exacte dans le journal (section 14).
4. Toute mesure suit les metriques de la section 4.5, fixees avant de lancer la mesure. Pas de juge LLM pour noter les resultats : regex et comparaison exacte uniquement. Chaque comparaison de variantes se fait a rendu fixe et sur la meme copie de base restauree.
5. Fixtures : contenu synthetique uniquement. Aucun extrait de transcript reel, aucun hote, aucune IP dans un fichier suivi par git. Les mesures brutes et les fixtures vont dans docs/dev-notes/supersession-bench/ (gitignore). Seuls les tableaux de synthese remontent dans le plan.
6. Code : niveau d'ecart upstream le plus bas qui resout le probleme (grille de CLAUDE.md). Avant d'editer un fichier suivi par upstream : git log -p sur le fichier, puis verifier si un patch numerote existant couvre la zone. Chaque phase de code = une section Patch 60+ dans local-patches.md (symptome, approche et pourquoi pas le niveau en dessous, fichiers, tests, conditions de retrait) + un fait Kleos tague delta-upstream. Rien de muninn n'est copie : ni code, ni regex, ni fixture (licence AGPL incompatible).
7. Tests : cargo test -p kleos-lib avec --features bundled-sqlite sur Windows ; build Linux via WSL. Le test de garantie de la phase 1 doit etre montre rouge sans le filtre avant d'etre accepte vert.
8. Un faux retrait est pire qu'un retrait manque : si une garde est en doute, elle reste restrictive et tu me le signales.
9. Commits atomiques par sous-etape, messages en anglais, push sur la branche apres chaque porte passee. Pas de PR sans mon accord.
10. Signale en tete de reponse toute contradiction entre le plan et ce que tu observes dans le code ou la base. Si le plan est faux, on corrige le plan avant de continuer.

Livrable attendu pour cette session
- Section 4.6 du plan remplie (ligne de base : existant voie A, existant voie B, sans memoire).
- Journal (section 14) alimente a chaque mesure.
- Un verdict argumente sur la porte de la phase 0 : la fuite est-elle confirmee sur donnees reelles, la fixture est-elle valide ?
- Si la porte passe et que je le confirme : phase 1, avec le test de garantie en premier.

Demarre par un inventaire de l'environnement (serveur, kleos-ingest, hooks actifs, version de la base, espace de test disponible), puis la section 4.1 du plan. Fais-moi un point court apres l'inventaire avant de lancer les requetes SQL de la section 4.2.
```
