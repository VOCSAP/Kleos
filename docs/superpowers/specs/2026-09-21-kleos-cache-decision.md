# ADR : kleos-cache, magasin vectoriel local en Rust, en remplacement de Chroma

Date : 2026-09-21
Statut : decision operateur actee (magasin local, projet separe, Rust) ; mesure de qualite soumise a decision operateur avant toute decommission Chroma
Precede par : `docs/superpowers/specs/2026-09-20-kleos-context-gateway-design.md` (POC) et `docs/superpowers/plans/2026-09-20-kleos-context-gateway-plan.md`
Suivi par : `docs/superpowers/plans/2026-09-21-kleos-cache-plan.md`

Etiquettes utilisees dans ce document :

- **MESURE** : chiffre issu des rapports du POC (`eval/reports/` du prototype) ou d'une commande lancee pendant la redaction, citee.
- **DEDUIT** : inference depuis le code lu, avec le `fichier:ligne`.
- **SUPPOSE** : cru mais non verifie. Toute ligne non etiquetee se lit comme SUPPOSE.

## 1. Decision

Le magasin vectoriel local qui projette la memoire Kleos sur le poste de travail devient un projet
autonome, **`kleos-cache`**, ecrit en **Rust**, dans un **depot separe** du fork (`VOCSAP/kleos-cache`),
sans dependance de code sur `kleos-lib`. Il remplace le conteneur Chroma et le gateway Python du POC.
Le harnais d'evaluation (`eval/`) reste en Python et devient l'actif de mesure du nouveau projet.

Ce que la decision NE rouvre PAS (arbitre avant cette ADR, repris tel quel) :

- Kleos reste canonique ; kleos-cache est une projection **jetable**, reconstruite en 11 min 8 s (MESURE, bootstrap POC).
- kleos-cache **n'ecrit jamais** vers Kleos. Raison dirimante : le texte projete a subi une redaction destructive
  (secrets remplaces par un marqueur) ; une ecriture de retour ecraserait le contenu canonique par sa version amputee.
  S'y ajoutent `updated_at` inutilisable comme horloge (bumpe a chaque lecture) et la perte de la reconstruction triviale.
  Si un artefact doit un jour remonter, c'est le PRODUCTEUR qui ecrit par le canal authentifie existant
  (`POST /sync/receive`, idempotent par `sync_id`, provenance forcee, incapable d'exprimer une mise a jour :
  DEDUIT, `kleos-lib/src/sync.rs:31-111` et `kleos-server/src/routes/platform/mod.rs:23-32`).
- Topologie : Kleos sur LXC 121 (3 Go, swap actif), kleos-cache sur le poste. C'est le design, pas un biais.
- Filtre de portee **inclusif** : `{space demande, "default", "__unscoped"}`, parce que Kleos l'est.
  Un filtre strict rendrait 24 des 33 cibles invisibles (MESURE, audit du jeu).
- Les memoires sans space sont un choix deliberé (fait cross-projet), pas un residu.

## 2. Faits mesures qui fondent la decision

### 2.1 Jeu de reference

40 requetes, 33 memoires cibles distinctes, aucune sollicitee plus de deux fois. La stratification est de
21 requetes sans token rare commun avec leur cible (`lexical_overlap: none`) et 19 avec (`shared`).

DECISION OPERATEUR (2026-09-25) : cette stratification 21/19 n'est pas prouvable par git, car le commit unique
`43573a6` contient les labels et les rapports du 2026-09-20. Elle est acceptee par l'operateur.

La premiere version du jeu (`eval/queries-v1-2026-09-20.jsonl`, 40 lignes, MESURE `grep -c`) etait
circulaire sur 35 requetes et surevaluait hit@5 d'environ 0,30 : **la baseline publiee au depart, 0,775, etait fausse.**

### 2.2 Tableau final sur les rapports de reference (MESURE)

Sources versionnees du depot `kleos-cache` :
`eval/baseline/2026-09-20T193256-kleos_only-mid.json`,
`eval/baseline/2026-09-24T121907-local_only-mid.json` et
`eval/baseline/2026-09-20T211951-hybrid-mid.json`. Les cohortes sont lues depuis
`eval/queries.jsonl`, pas depuis les rapports de bras.

| Profil | hit@5 | MRR@5 | nDCG@5 | p50 | IC95 hit@5 apparie vs Kleos, cas complets |
|---|---:|---:|---:|---:|---|
| `kleos_only-mid` | 19/39 scorees, plus `q031` en erreur | 0,441026 | 0,471347 | 2 692 ms | Reference |
| `local_only-mid` | 23/40 | 0,451667 | 0,494220 | 35,5 ms | [-0,076923 ; +0,230769], n=39 |
| `hybrid-mid` | 23/40 | 0,470417 | 0,505255 | 2 933 ms | [-0,025641 ; +0,179487], n=39 |

Le `ReadTimeout` de `q031` dans `kleos_only-mid` explique la divergence avec le chiffre attendu de 20/40 : le
rapport source ne porte aucun `hit_at_5` pour cette requete et ne compte que 19 succes sur 39 requetes scorees.
L'analyse qualite exclut `q031` des deux bras. L'analyse disponibilite compte toute erreur explicite comme miss,
dans l'un ou l'autre bras, afin de conserver les groupes 21/19 et l'agrege n=40 sans politique asymetrique.

### 2.3 Gain net apparié sur hit@5 (MESURE)

La commande `eval/.venv/Scripts/python.exe eval/paired_hit_gain.py` produit les artefacts versionnes
`eval/baseline/2026-09-25-paired-hit-gain.json` et
`eval/baseline/2026-09-25-paired-hit-gain.txt`. Elle verifie que chaque rapport porte exactement les IDs de
`eval/queries.jsonl`, lit les sous-groupes `lexical_overlap` depuis ce jeu canonique, puis applique 10 000
tirages bootstrap apparies, graine 0, pour `local_only - comparateur`.

**Qualite, cas complets.** Une erreur explicite dans l'un ou l'autre bras exclut la paire des deux bras.

| Comparaison | Sous-groupe | n | Gagnees / perdues / egales | Ecart hit@5 | IC95 bootstrap |
|---|---|---:|---:|---:|---|
| local vs Kleos | Paraphrasees | 21 | 5 / 0 / 16 | +0,238095 | [+0,047619 ; +0,428571] |
| local vs Kleos | Lexicales | 18 | 1 / 3 / 14 | -0,111111 | [-0,333333 ; +0,111111] |
| local vs Kleos | Agrege | 39 | 6 / 3 / 30 | +0,076923 | [-0,076923 ; +0,230769] |
| local vs Chroma | Paraphrasees | 21 | 1 / 0 / 20 | +0,047619 | [+0,000000 ; +0,142857] |
| local vs Chroma | Lexicales | 19 | 0 / 1 / 18 | -0,052632 | [-0,157895 ; +0,000000] |
| local vs Chroma | Agrege | 40 | 1 / 1 / 38 | +0,000000 | [-0,075000 ; +0,075000] |

`q031`, erreur `ReadTimeout` de Kleos, est exclue du calcul local contre Kleos : 22 succes locaux contre 19
succes Kleos sur 39 cas complets. Les identifiants de chaque categorie et les erreurs exclues sont dans les deux
artefacts produits.

**Disponibilite, toutes requetes.** Les 40 IDs canoniques sont conserves ; toute erreur explicite vaut miss,
symetriquement pour le bras local et le comparateur.

| Comparaison | Sous-groupe | n | Gagnees / perdues / egales | Ecart hit@5 | IC95 bootstrap |
|---|---|---:|---:|---:|---|
| local vs Kleos | Paraphrasees | 21 | 5 / 0 / 16 | +0,238095 | [+0,047619 ; +0,428571] |
| local vs Kleos | Lexicales | 19 | 2 / 3 / 14 | -0,052632 | [-0,263158 ; +0,157895] |
| local vs Kleos | Agrege | 40 | 7 / 3 / 30 | +0,100000 | [-0,050000 ; +0,250000] |
| local vs Chroma | Paraphrasees | 21 | 1 / 0 / 20 | +0,047619 | [+0,000000 ; +0,142857] |
| local vs Chroma | Lexicales | 19 | 0 / 1 / 18 | -0,052632 | [-0,157895 ; +0,000000] |
| local vs Chroma | Agrege | 40 | 1 / 1 / 38 | +0,000000 | [-0,075000 ; +0,075000] |

Le critere de l'arbitrage du 2026-09-21 est conserve : NO-GO si le bootstrap montre que le gain vient d'un
solde du type 5 gagnees / 2 perdues plutot que d'un gain net. Lecture A : si ce critere s'applique aux 40
requetes agregees, la disponibilite observe un solde de 7 gagnees et 3 perdues. Lecture B : si le critere
s'applique aux 21 paraphrases, qui sont la mesure de decision, le resultat est 5 gagnees, 0 perdue et 16
egales ; l'agrege est alors le garde-fou de non-regression prevu par l'arbitrage. La mesure est soumise a
decision operateur ; le gain de latence local reste mesure independamment.

### 2.3.1 Examen qualitatif des pertes lexicales (MESURE)

DECISION OPERATEUR (2026-09-25) : l'operateur tranche sur la pertinence des pertes, pas sur le seul chiffre
agrege. Les pertes locales `q004`, `q008` et `q020` contre Kleos sont reelles : cibles valides,
`acceptable_ids` vides et top-5 local hors sujet (memoire Kleos #19069). Contre Chroma, la parite reste une
requete gagnee et une perdue. La suite est la carte `addc6587`, qui ajoute un canal lexical FTS5 a
`kleos-cache`.

Defaut connu, carte `e6d5c410` : `acceptable_ids` est ignore par le harnais. Il peut sous-estimer le hit@5 en
ignorant une reponse alternative acceptable ; ce calcul ne le corrige pas.

### 2.4 Recherche exacte vs Chroma (MESURE)

- Metriques identiques au chiffre pres : hit@5 0,575, MRR 0,45625, nDCG 0,498541.
- Latence de recherche : 1,58 ms de mediane (matrice en memoire) contre 68,96 ms (Chroma), dont 31,6 ms
  d'aller-retour HTTP Docker au heartbeat. Ecart imputable a la recherche : facteur 23.
- HNSW rend 474 des 480 vrais voisins (98,75 % de rappel) ; top-5 different sur 1 requete sur 40.
  Approximation reelle, effet metrique nul.
- Matrice 5 227 x 1 024 float32 : 21,41 Mo. Chargement `.npy` : 8,8 ms. Extraction complete depuis Chroma : 1,31 s.
- Seuil estime de bascule vers un index approximatif : environ 100 000 vecteurs, soit **19 fois** le corpus
  actuel. A exprimer en ratio, pas en nombre absolu.

### 2.5 Projection et synchronisation (MESURE)

- 5 227 memoires projetables sur un corpus total de 17 785 (archivees et oubliees comprises).
- Bootstrap 11 min 8 s ; passages incrementaux 1,5 s a zero ecriture ; parite 100.
- Empreinte Chroma : 44,57 MiB a vide, 119,3 MiB charge (74,7 MiB pour 5 227 memoires), 66 Mio sur disque.
- Embeddings : bge-m3, 1 024 dimensions, via Ollama 192.168.10.16, meme modele des deux cotes.
  Vecteurs normes a 1e-7 pres : produit scalaire, cosinus et L2 classent identiquement.
- Embedding d'une requete : 180 ms de mediane, soit 99 % d'une recherche locale. C'est le prochain levier.
- Le hash de contenu exclut `updated_at` (bumpe a chaque lecture ; l'inclure re-embarquerait tout sans fin).
- Kleos fragmente au-dela de 1 440 octets ; 3,9 % du corpus depasse ce seuil et 32 des 33 cibles tiennent en
  un fragment : biais de granularite negligeable.

## 3. Questions d'architecture tranchees

### 3.1 Depot separe, pas un crate du workspace Kleos

**Faits.** Les 27 membres du workspace ont tous ete crees par upstream : `git log --format='%h %an %s'
--diff-filter=A -- '*/Cargo.toml'` ne liste que des commits `GhostFrame` (MESURE). La ligne `members` de
`Cargo.toml` n'a ete touchee que par des commits upstream (`git log -L '/^members/,+1:Cargo.toml'` : #126,
#83, #84... MESURE). `Cargo.lock` est touche par upstream a chaque release (3 des 8 derniers commits, MESURE).
La CI upstream lance `cargo clippy --workspace -- -D warnings` et `cargo nextest run --workspace`
(`.github/workflows/ci.yml:55,127`, MESURE). Le fork n'a jamais ouvert ce site de conflit.

**Options.**

- A -- Depot separe `VOCSAP/kleos-cache`, workspace Cargo propre, contrat avec Kleos = HTTP seulement.
  Delta upstream nul. Cout : quelques centaines de lignes re-ecrites (client d'embeddings, normalisation).
  Reversible : un crate autonome se rapatrie dans le workspace en une ligne `members` si un jour la politique change.
- B -- Membre du workspace. Touche `Cargo.toml` (ligne `members`, site upstream actif) et `Cargo.lock`
  (touche a chaque merge). Entre dans `clippy --workspace -D warnings` : tout nouveau lint d'une bump toolchain
  upstream casse le crate local. Le build Windows du workspace est deja contraint (`bundled-sqlite` obligatoire,
  crates Linux-only, cf. `CLAUDE.md`). Avantage : `use kleos_lib::...` par path dep.
- C -- Membre du workspace mais `exclude` du CI : incoherent, et ne supprime pas le conflit sur `members`.

**Verdict : A.** Force decisive : la politique du fork ("choisir toujours le niveau le plus bas", `CLAUDE.md`) place
le depot separe au niveau "zero delta upstream", alors que B ouvre un site de conflit sur le fichier racine
le plus touche par upstream, pour un gain (reutiliser `kleos-lib`) que la section 3.2 refuse de toute facon.
Le POC du 2026-09-20 avait deja ecarte le crate workspace pour une raison differente (rebuild WSL) ; cette
raison tombe (kleos-cache se compile nativement sur le poste, toolchain 1.94.0 MSVC active, MESURE
`rustup toolchain list`), la raison de politique de fork reste.

Conditions du depot separe : `rust-toolchain.toml` epingle a la meme version que Kleos (1.94.0), pour ne pas
laisser deriver deux toolchains sur le meme poste.

### 3.2 Aucune dependance de code sur kleos-lib

**Ce que kleos-lib offre** (DEDUIT, signatures) : le trait `EmbeddingProvider` et `OpenAiProvider`
(`kleos-lib/src/embeddings/openai.rs:15-211`, client `/v1/embeddings` compatible Ollama, verification de
dimension) ; `l2_normalize` (`embeddings/normalize.rs:2-9`) ; le trait `VectorIndex` et `LanceIndex`
(`vector/mod.rs:39-64`, `vector/lance.rs`) ; `toolbox::embedding::{to_blob,from_blob,cosine}`
(`toolbox/embedding.rs:13-48`, patch local 53).

**Ce que la dependance couterait** (DEDUIT, `kleos-lib/Cargo.toml:15-80`) : meme avec `default-features =
false`, kleos-lib tire `kleos-config`, rusqlite, deadpool, opentelemetry, pdf-extract, zip, whatlang... et sur
Windows exige `bundled-sqlite` ou `sqlcipher`. Une dependance `git = "VOCSAP/Kleos"` fige kleos-cache sur un
commit d'un fork qui rebase regulierement : chaque merge upstream devient un risque de casse du cache, pour
reutiliser ~250 lignes.

**Verdict : reecrire, pas reutiliser.** Le client d'embeddings compatible OpenAI (~100 lignes : body
`{"model","input"}`, tri des items par `index`, verification `len == 1024`, cf. `embedder.py` du POC),
la normalisation L2 et le produit scalaire sont triviaux. Lance n'est pas necessaire : la force brute est
mesuree a 1,58 ms et le seuil de bascule est a 19x le corpus. **La seule chose partagee avec Kleos est le
protocole HTTP**, et il l'est de toute facon : `GET /list`, `GET /spaces`, `GET /me`, `POST /memories/search`.
Ce contrat est verrouille par des fixtures enregistrees sur le vrai serveur et un test de forme rejoue au debut
de chaque lot (Lot 0 du plan).

Ce qui reste vrai independamment : l'identite de l'index = (modele, runtime, quantification, pooling,
normalisation). kleos-cache embarque via Ollama bge-m3, comme le POC. Le fait que Kleos embarque cote serveur
avec un ONNX quantifie n'a aucune incidence : la fusion est par rang (RRF), jamais par score, donc les deux
espaces n'ont pas a coincider.

### 3.3 Persistance : SQLite seul, matrice en memoire, ecriture transactionnelle

**Options.**

- A -- `.npy` + `meta.jsonl` + manifest : chargement 8,8 ms mesure, mais deux fichiers a ecrire dans un ordre
  precis avec fsync, et un manifest pour detecter l'incoherence. C'est le POC de mesure, pas un magasin.
- B -- **SQLite (rusqlite bundled, WAL)** comme unique store durable ; matrice `Vec<f32>` contigue en memoire
  chargee au demarrage ; force brute.
- C -- LanceDB : ANN inutile sous le seuil, arrow/lancedb lourds, format qui bouge entre versions.
- D -- sqlite-vec : plausible, mais ajoute une extension native pour une recherche que 1,58 ms de force brute
  couvre. C'est le chemin de montee en charge, pas le point de depart.

**Verdict : B.** Schema (une seule base `kleos-cache.db` dans le dossier de donnees) :

```
docs(id INTEGER PRIMARY KEY,           -- id Kleos
     family TEXT NOT NULL DEFAULT 'memory',
     space TEXT NULL,                   -- nom resolu, '__unscoped' si NULL cote Kleos
     category TEXT, tags TEXT,          -- tags en JSON, REDIGES
     importance INTEGER, is_static INTEGER,
     created_at INTEGER, updated_at INTEGER,
     content_hash TEXT NOT NULL,        -- sha256 du record source, sans updated_at
     text TEXT NOT NULL,                -- contenu REDIGE
     missing_passes INTEGER NOT NULL DEFAULT 0,
     schema_version INTEGER NOT NULL)
vectors(id INTEGER NOT NULL, chunk_idx INTEGER NOT NULL DEFAULT 0,
        dim INTEGER NOT NULL, vec BLOB NOT NULL,   -- f32 little-endian
        PRIMARY KEY (id, chunk_idx))
sync_state(key TEXT PRIMARY KEY, value TEXT)      -- reference_count, embedding_identity,
                                                  -- redaction_version, owner_user_id, last_pass
```

Ordre d'ecriture qui rend un crash benin :

1. Un lot d'upsert = une transaction : `INSERT OR REPLACE` dans `docs` puis `vectors`, commit, puis seulement
   mise a jour de la matrice en memoire. Un crash avant commit laisse l'etat precedent ; un crash apres commit
   est rattrape au redemarrage, la matrice etant toujours rechargee depuis SQLite. Il n'y a pas d'etat en
   memoire qui ne soit pas derivable de la base.
2. Les suppressions d'un passage et la nouvelle `reference_count` du garde anti-suppression sont ecrites dans
   la **meme** transaction. Le POC les separe (`sync_worker.py:293-304` : `delete` puis `write_reference_count`
   dans un fichier a part, DEDUIT) : un crash entre les deux laisse un garde arme sur une reference perimee.
3. `embedding_identity` et `redaction_version` sont verifies a l'ouverture : toute divergence avec la
   configuration courante refuse de servir et impose un rebuild. Un changement de modele ou de regles de
   redaction ne peut pas cohabiter silencieusement avec des vecteurs anciens.
4. Un `docs.id` sans ligne `vectors` (lot interrompu entre deux `INSERT` -- impossible dans une transaction,
   mais verifie par un `PRAGMA foreign_keys` + un test d'integrite au demarrage) est re-embarque au passage suivant.

`chunk_idx` et `family` existent des le premier jour avec une seule valeur, pour que le fragmentage et les
familles handoff/conversations n'exigent pas de migration.

### 3.4 Le corpus arrive par tirage `/list`, pas par export ni push

**Faits sur l'API existante** (DEDUIT) :

- `GET /list` : `limit` borne a 1 000, `offset`, `ORDER BY id DESC`, filtres `include_unscoped`,
  `include_forgotten`, `include_archived`, `space` (`kleos-server/src/routes/memory/mod.rs:924-955`,
  `kleos-lib/src/memory/mod.rs:1115`).
- `GET /sync/changes?since=` : `WHERE updated_at > ?1 AND user_id = ?2 ORDER BY updated_at` et la structure
  `SyncChange` ne porte **ni `space_id` ni `is_latest`** (`kleos-lib/src/webhooks.rs:58-73,747-751`). Inutilisable
  seul pour decider de la projetabilite ; et rehydrater par `GET /memory/{id}` bumpe `updated_at`, donc remet la
  ligne dans sa propre fenetre de delta.
- Les webhooks sortants refusent les adresses privees par defaut (`webhooks.rs:81-133`) : un push vers un poste en
  RFC1918 demanderait d'ouvrir le garde SSRF cote serveur.

**Options.**

- A -- Rescan complet de `/list` a intervalle (POC) : 6 pages par passage, 1,5 s a zero ecriture (MESURE), delta
  upstream nul. Cout LXC non mesure isolement (voir section 7).
- B -- `/sync/changes` : bloque par l'absence de `space_id`, et par `updated_at` comme horloge.
- C -- Patch 54 "export additif" cote kleos-server (vecteur stocke, `since`, tombstones) : un patch numerote, un
  build WSL, un deploiement sur un LXC sature, et le probleme d'horloge persiste tant que l'export ordonne par
  `updated_at`. A ne considerer que si A a un cout mesure sur LXC 121.
- D -- Push par webhooks : garde SSRF a ouvrir, canal non idempotent, un composant de plus. Ecarte.

**Verdict : A**, avec deux durcissements par rapport au POC :

- une suppression exige l'absence sur **deux passages consecutifs** (`docs.missing_passes`), ce qui absorbe la
  ligne sautee par un decalage d'offset quand une memoire est supprimee pendant un passage (`ORDER BY id DESC` +
  offset, DEDUIT) ;
- le garde anti-suppression de masse du POC (refus si l'ecart a la reference depasse 200 lignes ou 10 %,
  `sync_worker.py:30-31,159-172`) est conserve tel quel et sa reference est transactionnelle (3.3).

La cle Kleos utilisee par le tireur est une cle **dediee, a scope lecture**, et n'est jamais celle de l'operateur.
Au demarrage, `GET /me` (`kleos-server/src/routes/auth_keys/mod.rs:37,62-87`) verifie la cle et fixe
`owner_user_id` dans `sync_state` : l'index appartient a un utilisateur Kleos et un seul.

### 3.5 Ce que cette architecture rend impossible ou couteux plus tard

1. **Second poste de travail.** Chaque poste tient son propre kleos-cache : sa propre boucle de tirage sur LXC 121
   et son propre bootstrap (5 227 appels d'embedding, 11 min). N postes = N tireurs et N bootstraps. Il n'y a pas
   d'index partage, par construction : l'index contient du texte redige et n'a qu'un jeton local. Attenuation
   concue des le depart : `kleos-cache export` / `import` du fichier SQLite, avec verification de
   `embedding_identity`, `redaction_version` et `owner_user_id` ; un second poste du meme utilisateur demarre en
   secondes depuis le fichier du premier, seule la boucle de tirage reste par poste. Rendre l'index reellement
   partage plus tard revient a reintroduire un service reseau authentifie : c'est la phase 2 du plan du
   2026-09-20, sur un LXC qui n'a pas la RAM.
2. **Multi-utilisateur.** Un index = un utilisateur Kleos (la cle de lecture fixe `user_id`). Deux utilisateurs
   sur un poste = deux instances (dossier de donnees et port distincts). Pas de cloisonnement intra-index.
3. **Changement de modele d'embedding** : rebuild complet (11 min), sans effet sur la fusion (par rang).
   Le levier de latence identifie (180 ms d'embedding de requete, 99 % du temps local) passe par un embarqueur
   local sur le poste ; il change l'identite de l'index, donc un rebuild, mais rien d'autre. Le trait
   `Embedder` du plan est la seule couture a garder pour cela.
4. **Croissance du corpus** : la force brute est lineaire. A 19x le corpus (100 000 vecteurs), ~30 ms par
   recherche (DEDUIT par proportion depuis 1,58 ms) et ~400 Mo de matrice. La bascule vers sqlite-vec ou un ANN
   ne change ni `docs` ni le contrat HTTP.
5. **Fragmentage et familles** : reserves par `chunk_idx` et `family`, sans migration. Le fragmentage ajouterait
   une variable a la mesure ; il n'entre qu'avec une re-mesure sur le jeu de reference.
6. **Ecriture vers Kleos** : impossible par construction. Le client HTTP de kleos-cache n'a que quatre methodes
   (`list_page`, `list_spaces`, `whoami`, `search`), et un test verifie qu'aucune requete autre que GET ou
   `POST /memories/search` ne sort. Ce n'est pas une discipline, c'est un type.
7. **Fusion avec Kleos dans le hook** : le hook a un budget de 3 s (`hooks/full/user-prompt-lean.sh:122`,
   `--max-time 3`, MESURE) ; Kleos seul est a 2 789 ms de p50 et la fusion a 3 588 ms de p95. La fusion ne
   tient pas dans le hook. **Le hook sert le mode local ; la fusion reste disponible par HTTP et outil MCP**
   pour un appel explicite. C'est un choix de produit, signale en section 8.

## 4. Architecture cible

```
Claude Code (poste Windows)
   |  UserPromptSubmit : user-prompt-lean.sh
   |    1. POST http://127.0.0.1:8765/v1/retrieve  mode=local  (timeout 1 s, Bearer = jeton LOCAL)
   |    2. fallback sidecar /recall (existant), 3. fallback kleos-cli context (existant)
   v
kleos-cache (Rust, 127.0.0.1:8765, %LOCALAPPDATA%\kleos-cache\)
   |-- http       : axum ; /health ; /v1/retrieve ; /v1/status ; /v1/sync ; garde Host ; jeton local
   |-- retrieve   : local | kleos | hybrid (RRF k=10, poids 1,0 / 1,0, tie-break par id)
   |-- index      : matrice f32 en memoire, produit scalaire, filtre space inclusif
   |-- store      : SQLite WAL (docs, vectors, sync_state)
   |-- sync       : tirage /list toutes les 60 s, hash sans updated_at, redaction, garde, 2 passages avant suppression
   |-- kleos      : client HTTP LECTURE SEULE (list_page, list_spaces, whoami, search)
   |-- embedder   : Ollama /v1/embeddings bge-m3, lots de 32, dim 1024 verifiee
   |-- redaction  : motifs du POC + entropie + bearer insensible a la casse + tags et categorie
   v
Kleos LXC 121 (inchange)         Ollama 192.168.10.16 (inchange)
```

Deux credentials, deux roles, jamais interchangeables :

| Credential | Detenu par | Sert a | Accepte comme |
|---|---|---|---|
| `KLEOS_CACHE_TOKEN` (64 hex, genere au premier demarrage, fichier 0600) | hook, MCP, harnais | acceder aux routes de kleos-cache | seule credential des routes |
| cle Kleos scope lecture | le tireur de sync uniquement | `/list`, `/spaces`, `/me` | jamais une credential de route |
| Bearer Kleos de l'appelant (modes `kleos` et `hybrid` seulement) | l'appelant | relaye tel quel a `POST /memories/search`, valide par Kleos | jamais une credential de route |

Modele repris de `kleos-sidecar` (`kleos-sidecar/src/auth.rs:33-71` : comparaison en temps constant,
`routes.rs:205-218` : `/metrics` hors couche d'auth ; `main.rs:73-80` : bind non-loopback interdit sans jeton,
DEDUIT). Difference voulue : le sidecar tolere l'absence de jeton en loopback ; kleos-cache **exige** le jeton
meme en loopback, parce qu'il sert un index sans autre protection et parce que le rebinding DNS vise justement
le loopback.

## 5. Defauts du prototype et leur traitement dans la conception

| Defaut (etat du prototype) | Traitement kleos-cache |
|---|---|
| Une page vide transitoire de `/list` supprimait l'index entier, parite affichee 100 % ; corrige par un garde (200 lignes / 10 %) reproduit par un test rouge (`test_an_empty_listing_deletes_nothing_and_degrades_the_pass`, MESURE : `pytest tests/unit/test_sync_worker.py` -> `25 passed`) | Garde present des le Lot 2, reference transactionnelle, plus la regle des deux passages ; test rouge porte en premier |
| La route de recherche ne verifie que la FORME du bearer ; en mode local n'importe quel jeton ouvre l'index (`app.py:126-133`, DEDUIT) | Jeton local obligatoire, compare en temps constant, sur toute route sauf `/health` ; la cle Kleos n'est jamais une credential de route |
| Index sans authentification, tout `Host` accepte (rebinding DNS) | Bind 127.0.0.1 ; requete refusee (421) si `Host` n'est pas `127.0.0.1[:port]` ou `localhost[:port]` ; jeton exige en plus ; bind non-loopback refuse |
| La redaction laisse passer une chaine nue a haute entropie et `bearer` en minuscules (une cle reelle est passee par ces deux trous) | Detecteur d'entropie sur les jetons de 32 caracteres et plus (seuil et classes de caracteres calibres sur le corpus reel, Lot 2) ; motif `bearer` insensible a la casse ; `redaction_version` dans `sync_state`, tout changement de regles force une re-projection |
| Tags et categorie non rediges | Redaction appliquee a `text`, `tags`, `category` ; test qui l'atteste sur les trois champs |
| Reference du garde ecrite dans un fichier separe apres les suppressions | Meme transaction SQLite (3.3) |
| Prototype hors controle de version (`git -C kleos-context-gateway log` -> `fatal: not a git repository`, MESURE) ; le jeu de 40 requetes, seule mesure de qualite possedee, n'est versionne nulle part | Lot 0 : `eval/` entre dans le depot `kleos-cache` au premier commit |

Le risque residuel accepte par l'operateur (sa propre cle, deja passee) reste accepte ; la classe de defaut,
elle, est corrigee.

## 6. Ce qui se porte, ce qui se jette, ce qui reste en Python

Portage en Rust (le comportement est specifie par les tests Python existants, exportes en fixtures JSON) :

| Module POC | Lignes | Devient | Ce qui change |
|---|---:|---|---|
| `sync_worker.py` | 394 | `sync.rs` | reference transactionnelle, deux passages avant suppression, `family`, `chunk_idx` |
| `embedder.py` | 70 | `embedder.rs` (trait `Embedder` + impl Ollama) | identique |
| `redaction.py` | 46 | `redaction.rs` | + entropie, + bearer casse, + tags/categorie, + `redaction_version` |
| `rank_fusion.py` | 94 | `fusion.rs` | identique (arrondi a 12 decimales, tie-break par id) |
| `kleos_adapter.py` | 108 | `kleos.rs` | lecture seule par construction du type, pas par garde runtime |
| `app.py` + `config.py` + `metrics.py` | 376 | `http.rs`, `config.rs` (TOML + env `KLEOS_CACHE_*`), `metrics.rs` | + jeton, + garde Host, mode `chroma_only` renomme `local` |

Jete : `chroma_index.py` (et son contournement du timeout `client._server._session.timeout`), le compose
Docker, `_sanitize_metadata` (tableaux aplatis en CSV pour Chroma), l'import circulaire
`chroma_index -> sync_worker`, le conteneur et son volume.

Reste en Python, versionne dans `kleos-cache/eval/` : `evaluate.py` (475 lignes, `test_evaluate.py` 674
lignes), `queries.jsonl` (40), `queries-v1-2026-09-20.jsonl` (40, conserve comme piece du biais), `reports/`.
Modification unique : le constructeur `chroma_only` appelle `POST /v1/retrieve` de kleos-cache au lieu de
`chroma_index` ; le harnais ne connait plus que HTTP. Ajout : bootstrap apparie (2.3).

## 7. Mesures et gates de decision

| # | Mesure | Resultat / gate | Lot |
|---|---|---|---|
| 1 | Temps de chargement des 5 227 BLOB f32 depuis SQLite au demarrage | MESURE : p50 20,896 ms, PASS (< 200 ms). Commande Criterion : `cargo bench --manifest-path C:/Users/Olivier/workspace/kleos-cache/Cargo.toml --bench index`. | 1 |
| 2 | Produit scalaire en Rust sur 5 227 x 1 024, p50 | MESURE : p50 1,865 ms, PASS (< 5 ms). Commande Criterion : `cargo bench --manifest-path C:/Users/Olivier/workspace/kleos-cache/Cargo.toml --bench index`. | 1 |
| 3 | Cout d'un passage `/list` sur LXC 121 | Non faite, non bloquante (arbitrage operateur 2026-09-25). | 2 |
| 4 | Taux de faux positifs du detecteur d'entropie | Non faite, non bloquante (arbitrage operateur 2026-09-25). | 2 |
| 5 | `GET /me` avec une cle scope lecture | Non faite, non bloquante (arbitrage operateur 2026-09-25). | 2 |
| 6 | Bootstrap apparie de hit@5 | MESURE : section 2.3, 10 000 tirages, graine 0 ; deux lectures du critere sont soumises a decision operateur. | 5 |
| 7 | `memories.version` bumpe-t-il a la lecture ? | Non faite, non bloquante (arbitrage operateur 2026-09-25). | Hors plan |
| 8 | Parite locale contre Chroma sur le jeu corrige | MESURE : 23/40 dans les deux cas ; une requete gagnee et une perdue, IC95 [-0,075000 ; +0,075000]. PASS pour la parite agregee, avec ecarts par requete consignes en 2.3. | 3 |

## 8. Options ecartees et reserves

- Crate du workspace Kleos : 3.1.
- Dependance `kleos-lib` : 3.2.
- LanceDB, sqlite-vec, HNSW au depart : 3.3 ; sqlite-vec reste le chemin a 19x.
- `/sync/changes`, Patch 54 export, webhooks : 3.4.
- Valider le bearer Kleos de l'appelant par `GET /me` a chaque requete : rendrait le mode local dependant de LXC 121
  et assimilerait "possede une cle Kleos" a "peut lire cet index local", ce qui n'est pas la meme chose.
- Fusion par defaut dans le hook : ne tient pas dans 3 s (3.5.7).
- Fragmentage en v1 : ajoute une variable a la mesure ; 3,9 % du corpus concerne.
- Serveur MCP en v1 : le hook est le consommateur principal ; le harnais et un `curl` couvrent le reste. Un
  outil MCP `context_search` est un lot optionnel, pas une condition de la decision.

Reserves a garder en tete :

- L'ecart agrege n'est pas significatif (2.3). Si la mesure du Lot 0 (bootstrap apparie) contredit la
  stratification, la decision "magasin local" tient toujours (latence, independance de LXC 121), mais la
  promesse de qualite tombe a "au moins aussi bon".
- Le jeu de 40 requetes est petit et audite par la meme personne qui a construit le POC. L'enrichir depuis
  l'usage reel (JSONL des requetes du hook, hash de requete par defaut) est la seule facon de le faire grandir.
- `kleos-cache` ne remplace pas Kleos : `kleos-cli`, le sidecar et les hooks continuent d'ecrire et de lire
  Kleos comme aujourd'hui.

## 9. Decisions de l'operateur

1. A1 : le hook sert `local` dans son budget de 3 s ; la fusion reste un appel explicite.
2. A2 : le tirage est configurable a 300 s et un trigger PostToolUse le complete apres `kleos-cli store`.
3. A3 : les deux variables d'environnement user-scope restent distinctes ; aucun fichier de jeton 0600 n'est cree.
4. A4 : le prototype Python reste sur disque, hors git et hors `attic` ; le harnais `eval/` versionne est distinct.
5. A5 : `kleos-cache` reste un depot neuf sur `main` et les modifications du fork restent sur `local/kleos-cache`, jamais sur `local/patches`.
6. A6 (2026-09-25) : `kleos-cache` est adopte sur le critere de latence ; Chroma est conserve, conteneur arrete et bind mount garde.
7. A7 (2026-09-25) : les pertes lexicales reelles `q004`, `q008` et `q020` ouvrent la carte `addc6587` pour le canal FTS5.

## 10. Decommission de Chroma annulee

DECISION OPERATEUR (2026-09-25) : Chroma est conserve. Le conteneur `kleos-context-gateway-chroma-1` reste arrete
et le bind mount hote `~/.local/share/kleos-chroma` est garde. Aucune suppression n'est autorisee par cette ADR.

Si une decision ulterieure relance la decommission, un GO explicite de l'operateur, donne juste avant l'operation
irreversible, sera requis avant ces deux etapes de reference :

1. Executer `docker rm kleos-context-gateway-chroma-1`.
2. Supprimer ensuite le bind mount hote `~/.local/share/kleos-chroma`.

Le compose reference n'est pas une cible : son chemin n'existe plus et le conteneur ne porte aucun volume nomme.
La suppression du bind mount est irreversible sans copie verifiee. Aucun `docker compose down -v` ne fait partie de
cette procedure.
