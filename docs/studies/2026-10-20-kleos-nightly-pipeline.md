# Etude d'architecture : passe du soir, etat de traitement et routage des modeles

Statut : **brainstorm redige le 2026-10-20, challenge le meme jour (deux challengers
independants, verdicts en section 11), aucun code**. Les corrections du challenge sont
portees dans le corps. Premieres mesures de volume faites sur la copie figee
`tenant2-20261018-082511.db` (section 6) ; le reste des chiffres vient d'etudes
anterieures (references en fin de document) ; les estimations sont marquees comme
telles.

Etude soeur : `docs/studies/2026-10-17-supersession-muninn-plan.md` (la section 9 dit
ce que cette etude reprend du plan de supersession).

Schema visuel du cycle de vie actuel (16 diapositives) : `docs/dev-notes/memory-lifecycle.html`,
fichier local non suivi par git.

---

## 1. Pourquoi : ce que la lecture du code a etabli

Trois explorations independantes du code, en lecture seule (Kleos #30300, #30301) :

1. **Aucun lien n'est cree par un modele.** `similarity` = cosinus >= 0,55 (dreamer,
   `linker.rs`) ; `supersedes` = dedup a cosinus >= 0,9 tranchee par l'importance
   (`duplicates.rs:80-108`) ou `/correct` ; `contradicts` = seulement la route manuelle
   `GET /contradictions/{id}`. `corrects`, `updates`, `extends`, `caused_by` n'ont
   **aucun ecrivain automatique** (`POST /batch` op `link` accepte un type libre,
   `kleos-server/src/routes/batch/mod.rs:229-239`), alors que le scoring leur donne deja
   des poids (1,5 / 1,3 / 2,0, `scoring.rs:682-689`), jamais mesures. Ces poids sont
   recopies dans `graph/pagerank.rs:13-22` et `communities.rs:18-20`, ou `contradicts`
   vaut encore 1,3 (le Patch 66 n'a corrige que `scoring.rs`). Copie tenant :
   similarity 87 350, supersedes 987 (dont ceux de `/correct`, `similarity = 1.0`,
   `correction.rs:61`), consolidates 3 912, contradicts 0.
2. **Les retraits automatiques ne lisent pas le sens.** Deux retraits par regle :
   - la dedup du dreamer pose `is_superseded` sur un cosinus >= 0,9 et garde la memoire
     d'**importance la plus haute sans regarder la date** (`duplicates.rs:80-88`) : une
     decision recente d'importance plus basse, proche de l'ancienne (negation, retour en
     arriere), est retiree au profit de l'ancienne. Frequence inconnue (M-N12). Cadence :
     chaque tick du dreamer, 300 s par defaut, seulement apres 60 s sans requete HTTP
     (`config.rs:729-736`, `dreamer.rs:193-206`), 500 paires au plus par passe
     (`duplicates.rs:65`) ;
   - la consolidation pose `is_latest = 0` sur les sources a cosinus >= 0,85, 10 par
     tick, et les remplace par une concatenation `is_static = 1`
     (`consolidation.rs:139-141`). Desactivee par defaut (`KLEOS_CONSOLIDATION_ENABLED`,
     `config.rs:1235`) ; les 3 912 liens `consolidates` montrent qu'elle a deja tourne.
     Etat en production : a verifier.
3. **Qwen n'intervient ni sur les liens ni sur le chemin servi.** Il sert `growth`
   (observations archivees), les skills, la decomposition et l'extraction a la demande,
   le resume de kleos-ingest, l'inference de `/context` en mode Deep, le juge Thymus.
   Remplacer Qwen sur ces sites ne change pas la qualite de la base.
4. **Aucune notion de "deja traitee".** D'ou des taches qui se repetent a chaque
   passage : reconsolidation non idempotente, `temporal_patterns` et `reflections`
   reinseres toutes les 5 minutes sans dedup.
5. **L'agent recoit des lignes sans date, sans lien, sans conflit**, a tous les niveaux
   (demarrage, cache, sidecar, CLI). Le reranker ne sert que `/search` (niveau sidecar,
   environ 12 prompts sur 219) et `/context` (aucun hook). Le chemin servi principal
   (hook vers kleos-cache, `"mode":"local","top_k":3`, hooks-rs
   `user_prompt_lean.rs:193`) est un produit scalaire pur (kleos-cache `index.rs:76-100`) :
   **il ignore tous les liens**.
6. **La detection seule ne livre pas la nouvelle decision.** Fixture 4.4 : marquer a la
   main les 40 remplacements fait passer l'ancienne servie de 8 a 0 sur 40, mais la
   nouvelle servie de 5 a 6 seulement (Kleos #30297).

Conclusion de cadrage : un modele plus fort n'ameliore la base que dans des **etapes de
jugement nouvelles**, posees sur les candidats que les regles produisent deja, et
l'effet n'atteint l'agent que si le **rendu** change aussi. Le challenge renforce le
second terme : un lien type ne change pas ce que le cache sert (point 5) ; il n'agit
sur l'agent que par un retrait ou par un pointeur rendu.

---

## 2. Decisions operateur deja prises (2026-10-20)

| # | Decision | Kleos |
|---|---|---|
| D1 | L'espace `default` / NULL porte les decisions et faits globaux. Il a sa propre passe, bornee a lui-meme. Une memoire d'un espace projet n'est jamais comparee a `default` / NULL. | #30298 |
| D2 | Marquage complementaire : l'agent marque en direct a l'ecriture quand un candidat existe ; une passe differee complete "au cas ou". **Amendee le 2026-10-20 apres challenge (H1)** : cette passe tourne a cadence parametree sous la garde d'inactivite du dreamer, pas dans une fenetre du soir ; la fenetre horaire reste reservee a l'arriere et aux lots Batch. | #30298, #30827 |
| D3 | La confidentialite n'est pas un prealable : le contenu est deja passe par Claude Code et Codex. Une redaction legere des secrets reste souhaitable, pas obligatoire. | #30326 |
| D4 | Routage : Qwen la ou il suffit (attention a son contexte de 16k), un modele frontier la ou il faut plus de reflexion ou de contexte. | #30326 |
| D5 | Deux regimes de cout : un retraitement global ponctuel qui peut couter cher (ordre de grandeur accepte : 150 USD), et un cycle quotidien borne au budget API (100 USD par mois, travail surtout en semaine : environ 4,5 USD par soir sur 22 soirs ouvres, ou 3,3 USD sur 30 soirs). | #30326 |
| D6 | kleos-ingest ne peut pas etre servi la nuit : les postes clients sont hors ligne. Le transcript doit etre envoye a Kleos en journee et traite cote serveur. | #30326 |

---

## 3. Principes

1. **Servir tout de suite, juger plus tard.** Le store reste rapide et la memoire est
   servie immediatement. Les jugements couteux se font hors du chemin de la requete.
   Le volume a juger ne depend pas de la cadence : chaque memoire est jugee une fois,
   au meme cout unitaire ; un traitement differe au soir n'achete que la remise de
   l'API Batch (section 4.4).
2. **Les regles proposent, le modele tranche, la lecture montre.** Les seuils actuels
   (cosinus, SimHash, detecteur M1) deviennent des generateurs de candidats. Verdict du
   challenge : **indecidable** tant que la precision de la dedup n'est pas mesuree
   (M-N12) ; le juge qui la remplacerait n'est pas mesure non plus (accord operateur 6 a
   7 sur 10, rappel 0 sur 3 en phase 2). Variante hybride a trancher par M-N12 : la
   regle garde le retrait automatique pour la classe sure (paire hors decision, la plus
   recente gardee, ecart de creation < 1 h) et n'envoie au juge que le reste (une
   decision, un candidat M1, ou une paire ou la regle garderait la plus ancienne).
3. **Chaque etape est idempotente par construction**, grace a une table d'etat
   (section 4.2), et rejouable en incrementant sa version.
4. **Un faux retrait est pire qu'un retrait manque** (intention du plan de
   supersession). Un retrait exige un consensus ou un signal explicite ; le reste est
   annote ou pointe.
5. **Additif d'abord** : tables par canal overlay (Patch 41), modules nouveaux, taches
   hors DAG. Les patchs dans du code upstream sont listes en section 10.

---

## 4. Architecture cible

### 4.1 Ecriture et marquage en direct (inchange, plus un retour)

Le store ne change pas. Quand le detecteur trouve un candidat (decision du meme espace,
plus ancienne), la reponse du store porte une ligne courte : id, date, extrait borne a
150 caracteres, commande pour declarer le remplacement. L'agent qui stocke a la
conversation en contexte : c'est lui qui juge le mieux. Cout estime : environ 150 tokens
par candidat, environ 1,3 candidat par jour avec le detecteur lexical (194 en 5 mois).
Les ecritures sans agent dans la boucle (ingestion, imports) n'en profitent pas.

### 4.2 Table d'etat par etape

Overlay tenant, nom semantique `memory_pipeline_state` :

```
memory_id      INTEGER NOT NULL
stage          TEXT    NOT NULL   -- 'triage', 'neighbourhood', 'decision_conflicts', ...
stage_version  INTEGER NOT NULL   -- incremented when the model, prompt or rule changes
content_sha256 TEXT    NOT NULL   -- sha256 of content at processing time
done_at        TEXT    NOT NULL
outcome        TEXT               -- 'ok', 'skipped', 'failed', 'deferred'
model          TEXT
cost_usd       REAL
UNIQUE (memory_id, stage)
```

Une memoire a besoin de l'etape S si aucune ligne n'a la version courante et l'empreinte
courante. Jamais `updated_at` : il est reecrit a la lecture (`context/deps.rs:381`,
`memory/mod.rs:1005-1007`, `reconsolidation.rs:212`). Effets :

- les etapes deviennent idempotentes ;
- changer de modele ou de prompt = incrementer `stage_version` = retraitement ;
- le retraitement global n'est pas un chantier a part : c'est l'arriere du meme job ;
- `cost_usd` alimente le plafond de budget (4.4).

### 4.3 Zone d'attente des transcripts (kleos-ingest)

Une table dediee, pas des memoires `pending` : `pending` est le statut de la boite de
revue, qui serait noyee.

```
transcript_staging(id, session_id, space, received_at, chars, payload, status)
```

- En journee, kleos-ingest envoie le transcript tronque (tours user, reponses finales
  de l'assistant) par une route d'envoi nouvelle. Il ne resume plus lui-meme.
- La passe du soir en tire le resume de session et les decisions, puis marque la ligne
  traitee ; une purge borne la retention.
- Prerequis connus : le daemon envoie `X-API-Key`, que le serveur ne lit jamais
  (`kleos-ingest/src/writer.rs:72-74`, 401 mesure en phase 0) ; le resumeur
  (`summarizer.rs`) n'envoie pas d'`Authorization` et son prompt est code en dur. Le
  plan global prevoit deja de reactiver kleos-ingest.

### 4.4 Passe du soir : declenchement, ordre, budget

- **Declenchement** (corrige apres challenge, H1 tombe ; D2 amendee par l'operateur) :
  un seul job, **cadence parametree**, sous la garde d'inactivite du dreamer
  (`dream_idle_threshold_secs`, `dreamer.rs:194`, chaque requete HTTP horodatee par
  `audit.rs:135`). C'est le modele du lot B de la phase 2, qui juge toutes les 21 600 s
  avec cette garde (`2026-10-19-supersession-phase2-spec.md:181-184`). La fenetre
  horaire du soir est reservee a l'arriere et aux lots Batch. Raisons : le volume ne
  baisse pas le soir (principe 1) ; la contention avec le chemin servi est deja reglee
  par la garde, que `dreamer-growth` utilise environ 842 fois par jour pour Qwen ; les
  etapes Haiku et Sonnet ne passent pas par Ollama ; une decision changee le matin a ses
  liens le jour meme. Le test qui tranche est M-N13.
- **Ordre** : (1) transcripts en attente ; (2) memoires du jour, passe de tri ;
  (3) memoires du jour, passe de voisinage ; (4) arriere, par priorite (decisions et
  importance haute d'abord, puis le reste).
- **Plafond** : un budget par jour (nom propose `KLEOS_NIGHTLY_BUDGET_USD`), decompte
  depuis `cost_usd`. Plafond atteint = arret propre, la suite reprend au cycle suivant.
  Relever le plafond quelques nuits = retraitement global. Avec l'API Batch, le cout
  d'un lot n'est connu qu'au retour : le decompte se fait sur une **estimation a la
  soumission**, sinon le plafond peut etre depasse d'un lot entier.
- **API Batch** : divise le cout par deux. Delai documente : la plupart des lots en
  moins d'une heure, 24 h au plus, lot expire non facture, traitement ralenti selon la
  demande (platform.claude.com/docs/en/build-with-claude/batch-processing, lu le
  2026-10-20). LiteLLM l'expose en passthrough `/anthropic/v1/messages/batches` ; le
  suivi de cout exige le modele declare dans `proxy_config.yaml` (version deployee non
  verifiee). Le chemin tri, escalade, voisinage fait **deux allers-retours** ; les
  resultats arrives hors fenetre s'appliquent hors de la garde d'inactivite. Gain
  quotidien faible (environ 0,4 USD par jour au volume mesure), gain reel sur
  l'arriere (74 -> 37 USD sur les actives en Sonnet). Test : M-N10.

### 4.5 Passe de tri (une memoire a la fois)

Entree : une memoire, son espace, sa date. Sortie JSON stricte :

```
{ "kind": "decision|state|fact|discovery|other",
  "durable": true,
  "importance": 1-10,
  "title": "<= 80 chars",
  "change_marker": true,
  "facts": [ { "subject": "...", "attribute": "...", "value": "..." } ] }
```

- Ecrite d'abord dans une table d'annotations (overlay), **pas** dans les colonnes de
  `memories`. L'application d'un champ (importance, categorie) ne vient qu'apres mesure
  d'accord.
- Les faits ne se melangent pas au bruit regex de `structured_facts` (`(user, fait)`,
  308 faits sur des decisions, phase 2) : table separee ou marquage de source, a
  trancher. Ce sont eux qui rendraient possible la regle "meme cle, valeur differente"
  abandonnee en 6.0 du plan de supersession.
- Modele : Qwen si la memoire tient dans son contexte, Haiku au-dela (section 5).
- Prerequis de fiabilite : sortie contrainte par schema (`format` ou `response_format`
  d'Ollama), absente du client actuel (`intelligence/llm.rs:51-58`) ; compteur des
  echecs de parsing (aujourd'hui un `warn` puis `None`, `decomposition.rs:251`) ;
  `max_tokens` par defaut a 1 024 (`intelligence/types.rs:371`), qui peut tronquer une
  liste de faits ; `KLEOS_LLM_THINK=0` pose, sinon qwen3 raisonne par defaut
  (`llm/mod.rs:76-78`).
- **Verdict du challenge (H2, indecidable)** : au lancement, les sorties du tri ne
  servent qu'au canal de candidats par faits et a `change_marker`. Si les vraies paires
  de remplacement sont deja atteintes par les liens `similarity` et le detecteur M1
  (M-N11), le tri se replie dans l'appel de voisinage : `kind`, `durable`,
  `change_marker` et `title` deviennent des champs de sa sortie, sans table ni etape
  nouvelles. Si le tri est garde, son seul gain de cout defendable est de servir de
  filtre : `durable = false` saute la passe de voisinage.
- Risque non couvert par M-N5 : des cles `(sujet, attribut)` produites memoire par
  memoire, sans le vocabulaire existant, peuvent ne jamais se recouper entre deux
  memoires ecrites a des mois d'ecart. Cela ne se voit que sur des paires (M-N5
  amende).

### 4.6 Passe de voisinage (groupes de memoires)

- **Candidats** de chaque memoire du jour, dans son espace (D1) : ses liens
  `similarity` (cosinus >= 0,55, 6 liens sortants au plus au moment du lien ; le linker
  ecrit les deux sens, `linker.rs:199-231`, donc une memoire ancienne en porte
  davantage), les decisions qui passent le detecteur M1,
  les memoires qui partagent un `(sujet, attribut)` issu du tri, les paires de dedup a
  cosinus >= 0,9 (4.7).
- **Groupe** : la memoire du jour et ses candidats, dates et tries, juges ensemble comme
  le juge par chaine de la phase 2 (6.1 bis du plan de supersession).
- **Verdicts par paire** et ecriture :

| Verdict | Ecrit | Retrait |
|---|---|---|
| `doublon` | `supersedes` | seulement sur consensus ; hors decisions, consensus a definir (sinon : jamais retire) |
| `remplace` | `supersedes` | seulement sur consensus (regle de la phase 2) ; hors decisions, idem |
| `met_a_jour` | `updates` (nouvelle vers ancienne) | non, l'ancienne reste servie avec un pointeur ; prerequis N1 |
| `partiel` | annotation du point perime | non |
| `contredit` | `contradicts` | non ; prerequis : reconsolidation idempotente (section 7) |
| `prolonge` | `extends` | non |
| `distinct` | rien | non |

- **Validateur deterministe** (repris de 6.1 bis) : un lien de remplacement va du plus
  recent au plus ancien, pas de cycle, chaque groupe garde au moins une memoire active,
  aucune memoire remplacee n'est reactivee.
- Les decisions passent par le juge deja choisi (Sonnet `medium` par chaine, consensus
  avec le juge par paire). Le reste par Haiku, sous reserve de mesure. Le consensus
  n'est defini que pour les decisions (plan de supersession, 6.1 bis) : un Haiku seul
  qui retire violerait le principe 4.
- **Effet des liens ecrits sur le classement (H4 tombe)**. Le cache les ignore
  (section 1, point 5). Cote serveur, ils jouent plutot pour l'ancienne :
  - l'expansion de graphe est non orientee (UNION des deux sens, `search.rs:526-536`) :
    quand la nouvelle est graine, `updates` injecte l'ancienne ; le poids ne fait que
    reordonner le canal graphe, a 1/(90+rang+1) pres (`scoring.rs:38`, `:611-613`,
    `search.rs:1275-1282`), soit environ 6 % entre le rang 0 et le rang 5 ;
  - PageRank est oriente vers la cible (`pagerank.rs:103-104`) : `updates` nouvelle vers
    ancienne donne la masse a l'ancienne. Il regroupe les paires par `MAX(ml.type)`
    (`pagerank.rs:58`) : `similarity` l'emporte textuellement sur `extends`,
    `contradicts`, `corrects` et `caused_by`, qui disparaissent de PageRank des qu'un
    lien `similarity` existe dans le meme sens ;
  - la reconsolidation use la cible de tout `corrects`, `updates` ou `contradicts`
    (confiance x0,5, importance -2 a chaque passe, 20 memoires par tick,
    `reconsolidation.rs:86-98`, `:245-252`) : l'ancienne d'un `met_a_jour`, censee rester
    servie, s'efface en quelques semaines.

  Consequence : avant N1, ecrire les verdicts non retirants dans la table d'annotations
  ou sous un type que ni la reconsolidation ni PageRank ne lisent ; leur valeur passe
  par le rendu (4.9), pas par le classement. Test : M-N14.

### 4.7 Dedup du dreamer en mode proposition

**Corrige apres challenge (H3 tombe telle qu'ecrite).**

- `dry_run` ne renvoie qu'un compte (`pairs_found`) et jette la liste
  (`duplicates.rs:68-74`) : le rendre pilotable par variable d'environnement revient a
  **couper** la dedup, pas a produire des candidats. Les paires se lisent par
  `find_duplicates` (publique, `duplicates.rs:10`) ; pour les memoires du jour, elles
  sont deja dans les liens `similarity` de 4.6.
- Couper la dedup degrade le chemin servi le jour meme : un doublon cree le matin sur le
  sujet actif peut occuper 2 des 3 places du cache (`top_k 3`), alors qu'aujourd'hui il
  disparait au premier tick inactif. Et hors decisions, faute de consensus defini
  (4.6), il ne serait jamais retire.
- Option retenue a mesurer : la variante hybride du principe 2. La dedup garde le
  retrait pour la classe sure et ne propose que le reste ; elle corrige au passage le
  choix par importance sans la date (section 1, point 2).
- Volume : les 987 `supersedes` comprennent ceux de `/correct` ; la part de la dedup
  (`similarity < 1.0`) et son rythme par jour se mesurent par M-N12 (requetes R1a a
  R1c, R3).

### 4.8 Espace `default` / NULL

Passe separee (D1), memes etapes, candidats tires uniquement de `default` / NULL. Une
memoire de projet ne recoit jamais de candidat global, et inversement.

### 4.9 Rendu aux agents

| Ajout | Ou | Cout |
|---|---|---|
| date de creation sur chaque ligne | prompt vivant, cache, sidecar, CLI | environ 4 tokens par ligne |
| `-> plus recente : #n (date)` sans texte | memoire servie qui a un `updates` / `supersedes` entrant | environ 10 tokens |
| `conflit avec #m` | paire `contradicts` ou conflit ouvert | environ 34 tokens (M3) |
| `created_at`, liens, conflits repliques | kleos-cache (`LocalResult` ne renvoie pas `created_at`, `http.rs:52-58`) | patch kleos-cache, empreinte separee des conflits |
| reranker sur le chemin du cache | kleos-cache | **a mesurer** contre le budget p50 <= 250 ms (Kleos #18697) ; `bge-reranker-v2-m3` sur CPU : 49 s pour 24 documents cote serveur (#10270) |

Le rendu est le seul levier qui atteint le chemin principal (le cache ignore les liens,
section 1, point 5) : les pointeurs et dates de cette table passent avant tout gain de
classement attendu des liens types (H4).

Le rendu se mesure par `agent_ok` via l'API (pas `claude -p`), en rejouant les blocs
enregistres dans `out-hook-*.jsonl`, mise en page fixe entre variantes.

---

## 5. Routage des modeles

| Tache | Modele | Raison |
|---|---|---|
| Tri d'une memoire courte | Qwen (`qwen3:8b-ctx16k`) | une memoire, sortie courte, tient dans 16k |
| Tri d'une memoire au-dela d'environ 8k tokens | Haiku | garde seulement : sur la copie, la plus longue memoire non archivee fait 6 663 caracteres (environ 2 000 tokens), aucune au-dela de 28 000 ; limite du store 102 400 octets (`validation.rs:14`) |
| Faits `(sujet, attribut, valeur)` des decisions | Qwen, escalade Haiku si sortie invalide ou incertaine | la qualite des cles conditionne la regle de remplacement |
| Resume d'un transcript en attente | Qwen | borne a fixer ; le resumeur actuel tronque a 3 000 caracteres (`kleos-ingest/src/summarizer.rs:16`) et sort 300 tokens (`:104`) |
| Voisinage hors decisions, arbitre de dedup | Haiku | plusieurs memoires a comparer |
| Conflits de decisions en chaine | Sonnet `medium` + consensus | mesure et decision de la phase 2 |

- Chaque juge a son propre client et ses variables (modele du client `conflict_judge`
  de la spec phase 2). `KLEOS_LLM_*` reste sur Qwen pour ses sites actuels : aucun
  patch du client partage.
- Chaque attribution se mesure avant d'etre figee : accord avec Sonnet sur un
  echantillon, comme le balayage de la phase 2.
- `KLEOS_LLM_THINK` injecte `reasoning_effort` ; son effet sur un modele Anthropic
  derriere LiteLLM n'est pas verifie.
- Ollama repond sur 192.168.10.50:11434 ; l'ancienne adresse 192.168.10.16 refuse toute
  connexion TCP (Kleos #30170).

---

## 6. Budget

Couts unitaires mesures sur le lot de la phase 2 (192 composantes, fichiers
`sweep-*.jsonl`) : Haiku 5.5 `medium` 1 461 tokens d'entree et 1 003 de sortie en
moyenne, soit environ 0,00065 USD par composante (77 % du cout vient de la sortie) ;
Sonnet 5.5 `medium` 1 461 / 195, environ 0,0049 USD. Tarifs publics 0,10 / 0,50 et 2 / 10
USD par million de tokens (platform.claude.com, lu le 2026-10-20) ; sortie et
raisonnement sont comptes. Un groupe de voisinage reel (memoire du jour et 5 candidats)
est **suppose** couter le double ; comme la sortie domine, le cout suit plutot le nombre
de paires a juger que la taille d'entree. Le consensus ajoute un second juge sur les
decisions (environ 0,12 USD par jour pour environ 12 decisions).

**Volume mesure** (copie figee, 31 jours du 2026-09-18 au 2026-10-18, lecture seule par
`q.sh`) :

- 28 987 memoires au total, dont 16 944 archivees (16 939 de `dreamer-growth`) et
  7 623 actives ;
- `claude-session` : 2 511 lignes, environ 81 par jour, 2 325 caracteres au plus ;
  environ 12 decisions par jour ;
- `dreamer-growth` : 8 957 lignes, toutes archivees, environ 216 caracteres ; le volume
  journalier passe de 83 a 251 lignes jusqu'au 2026-10-09 a 605 a 1 517 a partir du
  2026-10-10, porte par `dreamer-growth` (origine du saut non verifiee) ; les samedis ne
  contiennent que du `dreamer-growth`.

**Portee : `is_archived = 0`.** C'est la condition pour que le budget tienne : si
`dreamer-growth` entrait dans la portee (842 par jour en moyenne, 1 517 en pointe),
Sonnet sur tous les groupes couterait 8 a 15 USD par jour. La reactivation de
kleos-ingest ajoutera des ecritures automatiques non encore mesurees.

**Regime quotidien** (plafond D5) :

```
cout_jour ~= N_nouvelles_non_archivees x (cout_tri + cout_groupe)
cout_tri  ~= 0 (Qwen local)
```

A environ 81 memoires par jour : environ 0,1 USD avec Haiku, environ 0,8 USD avec Sonnet
sur tous les groupes (facteur 2 suppose).

**Retraitement global** :

| Portee | Voisinage Haiku | Voisinage Sonnet | Tri Qwen a 6 s |
|---|---|---|---|
| 7 623 actives | environ 10 USD | environ 74 USD | environ 12,7 h |
| 12 043 non archivees | environ 16 USD | environ 117 USD | environ 20 h |
| 28 987 au total | environ 38 USD | environ 281 USD | environ 48 h |

Le chiffre anterieur de 60 USD pour Haiku sur le total etait une erreur de calcul (38).
Le plafond accepte (D5, environ 150 USD) couvre Sonnet partout sur les non archivees,
sous reserve du cout reel d'un groupe (M-N4 : 0,043 USD par groupe ou moins pour le
quotidien, 0,0197 pour le global sur les actives, 0,0125 pour les non archivees). Les
6 s par appel Qwen viennent d'une requete Broca, pas d'un appel de tri (M-N6).

---

## 7. Noeuds a corriger avant de construire dessus

Deduits du code, aucun reproduit (Kleos #30301) :

| # | Noeud | Pourquoi c'est bloquant | Action |
|---|---|---|---|
| N1 | `reconsolidation_sweep` non idempotente : avec un lien `contradicts`, `updates` ou `corrects`, confiance x0,5 et importance -2 a chaque passe (`reconsolidation.rs:86-98`) | tout `contradicts` ou `updates` ecrit par la passe de voisinage userait sa cible | corriger avant les verdicts `contredit` et `met_a_jour` ; candidat upstream |
| N2 | `memory::list` ne filtre pas `is_superseded` (`memory/mod.rs:1021-1115`), utilise par les 5 memoires recentes du demarrage | ecart possible avec la garantie de la phase 1 | mesurer sur le banc, puis etendre la phase 1 |
| N3 | Brain sur le monolithe, ids de memoire par shard, `INSERT OR REPLACE` sur l'id (`pattern.rs:43`) | collisions possibles entre tenants | reproduire avant tout autre usage du brain |
| N4 | `contradiction_scan` calcule puis jette son resultat (`scheduler.rs:276-279`) | tache morte | remplacee par la passe de voisinage |
| N5 | `temporal_patterns`, `reflections` reinseres a chaque tick (`temporal.rs:257`, `reflections.rs:62`) | croissance sans fin | placer sous la table d'etat, ou couper si aucun lecteur (non verifie) |
| N6 | Ligne parasite `- Relevant memories:` du sidecar (`kleos-sidecar/src/routes.rs:833`, `user_prompt_lean.rs:357`) | bruit dans le contexte | correction du hook |
| N7 | Derives jamais nettoyes au forget / delete : `current_state`, faits, liens (`memory/mod.rs:1446`, `:1942`) | un fait d'une memoire supprimee survit | a traiter avec la table des faits du tri |
| N8 | La dedup garde l'importance la plus haute sans regarder la date (`duplicates.rs:80-88`) | une decision recente peut etre retiree au profit de l'ancienne | mesurer (M-N12), puis corriger avec la variante hybride (4.7) |
| N9 | PageRank regroupe par `MAX(ml.type)` (`pagerank.rs:58`) et garde `contradicts` a 1,3 (`pagerank.rs:17`) | les types `extends`, `contradicts`, `corrects`, `caused_by` disparaissent de PageRank des qu'un lien `similarity` existe ; ecart avec le Patch 66 | a traiter si des liens types sont ecrits (4.6) |

Challenge (H4) : ces noeuds, N1 et N9 surtout, signifient que les liens types ecrits
aujourd'hui joueraient contre l'intention. Ils sont bloquants pour toute ecriture de
lien type, pas seulement pour `contradicts`.

---

## 8. Mesures a faire avant tout code (ordre propose)

| # | Mesure | Sur | Decide |
|---|---|---|---|
| M-N1 | nouvelles memoires par jour, par source, par categorie, par espace | copie figee, SQL | le budget quotidien et l'echelle de tout le reste. **Partiellement faite** au challenge (section 6) : reste la ventilation par categorie et par espace, et la regle de portee `is_archived = 0` a confirmer |
| M-N2 | part ecrite par un agent (MCP, CLI) face aux chemins automatiques | copie figee | la portee du marquage en direct (4.1) ; verifier que `claude-session` correspond a un agent dans la boucle et pas a un hook |
| M-N3 | precision des retraits proposes : paires 6, 20, 23 de la feuille operateur, puis un echantillon des 12 autres | etiquetage operateur | si un retrait automatique est possible |
| M-N4 | cout reel et verdicts d'un groupe de voisinage, Haiku et Sonnet ; d'abord 20 groupes reels en Sonnet, puis 200 memoires | copie figee via LiteLLM | la repartition des modeles et le cout du global ; criteres en section 6 |
| M-N5 | accord Qwen / Sonnet sur le tri, 200 memoires ; plus, sur les 40 paires de la fixture 4.4 et les 3 paires `remplace` de l'operateur, la part des paires qui partagent une cle `(sujet, attribut)` normalisee, et le taux de JSON valide sous schema impose | idem | ce que Qwen peut garder ; critere : >= 50 % de paires reliees, >= 0,8 fois Sonnet, >= 99 % de JSON valide |
| M-N6 | debit de Qwen (appels par heure) avec `dreamer-growth` actif, sur un vrai appel de tri | Ollama 192.168.10.50 | la duree du retraitement global |
| M-N7 | fuite de `memory::list` (N2) | banc | l'extension de la phase 1 |
| M-N8 | `agent_ok` avec dates et pointeurs, rendu fixe | fixture 4.4, API | l'effet du rendu |
| M-N9 | latence d'un reranker dans le cache | poste | 4.9 |
| M-N10 | delai de l'API Batch : un lot de 100 requetes soumis vers 22 h via LiteLLM, 5 soirs, `ended_at - created_at` | API | 4.4 ; critere : < 2 h les 5 soirs pour garder Batch au quotidien, sinon Batch pour l'arriere seulement |
| M-N11 | rappel ajoute par le canal de faits : part des vraies paires de remplacement (40 de la fixture 4.4, 39 chaines de la phase 2) qu'aucun lien `similarity` ni le detecteur M1 n'atteint | copie figee, SQL | H2 : <= 5 % hors d'atteinte, le tri se replie dans le voisinage ; >= 10 %, Qwen sur ces seules memoires, le tri tient si ses cles en relient la moitie |
| M-N12 | precision et rythme de la dedup : requetes R1a a R1c et R3 (annexe A), puis lecture de 30 paires `supersedes` a `similarity < 1.0` | copie figee, SQL, lecture | principe 2 et H3 : la regle tient si `newer_kept` >= 95 %, ecart < 1 h pour >= 80 % des paires et <= 1 retrait faux sur 30 ; le juge est justifie a >= 3 sur 30 ; H3 tombe en volume a >= 3 retraits dedup par jour ouvre a ecart < 1 h |
| M-N13 | part intra-journee : sur les 194 paires candidates M1 et les 15 retraits du consensus, part dont l'ecart de creation est < 24 h ; puis rejeu de `out-hook-*.jsonl`, blocs servis contenant une memoire remplacee plus tot le meme jour | copie figee, journaux de hook | H1 : < 5 % des blocs et < 10 % des paires, le soir suffit ; >= 20 %, cadence continue au moins pour les decisions |
| M-N14 | effet de 40 liens `updates` (nouvelle vers ancienne) de la fixture 4.4 sur le chemin serveur (`/search`, `kleos-cli context`), avec et sans | copie du banc, jamais la base vive | H4 cote serveur : tient si la nouvelle servie gagne >= 5 / 40 sans que l'ancienne servie augmente ; cote cache, effet nul par construction |

Contraintes de mesure inchangees : lectures sur la copie figee, jamais la base vive ;
secrets via `cred exec` ; juge via LiteLLM.

---

## 9. Articulation avec le plan de supersession

| Plan de supersession | Dans cette etude |
|---|---|
| Phase 1 (garantie structurelle) | acquise ; N2 la complete |
| Phase 2 lot A (detecteur, indicateur) | le detecteur M1 devient un generateur de candidats (4.6) ; l'indicateur passe dans le rendu (4.9) |
| Phase 2 lot B (juge, retrait sur consensus) | la branche "decisions" de la passe de voisinage ; la porte reste M-N3 |
| Phase 2 lot C (kleos-cache, hooks) | le rendu (4.9) |
| Phase 3 (marqueurs de changement) | le champ `change_marker` du tri |
| Phase 4 (livraison) | le rendu (4.9) ; le catalogue complet au demarrage est ecarte (trop gros, pertinence non mesuree) au profit des pointeurs et d'un outil a la demande |
| Phase 5 (commits git) | inchangee, hors de cette etude |

---

## 10. Ecart upstream estime

- **Additif** (cout de rebase tres faible) : overlays `memory_pipeline_state`,
  annotations, faits du tri, `transcript_staging` ; modules de tri, de voisinage, de
  juges ; job du soir hors DAG, appele depuis `run_cycle` / `run_cycle_tenants` comme le
  lot B.
- **Patchs chirurgicaux** : dedup hybride (classe sure retiree, reste propose, date
  prise en compte, N8), forme a fixer apres M-N12 ;
  correction de la reconsolidation (N1, candidat upstream) ; filtre `is_superseded` de
  `memory::list` (N2, candidat upstream) ; route d'envoi des transcripts ; en-tete
  d'authentification de kleos-ingest.
- **Hors depot Kleos** : kleos-cache (dates, liens, conflits, reranker eventuel),
  hooks-rs (rendu, ligne parasite).

---

## 11. Hypotheses a attaquer (pour le challenger)

Challenge du 2026-10-20 : deux challengers independants, l'un sur le principe 2 et
H2 a H4, l'autre sur H1 et H5 a H7. Les corrections sont portees dans le corps.

| # | Hypothese | Verdict | Premisse cachee | Test qui tranche |
|---|---|---|---|---|
| P | Les regles proposent, le modele tranche (principe 2) | indecidable | la dedup retirerait a tort assez souvent pour justifier un juge, lui-meme non mesure | M-N12 |
| H1 | Le soir vaut mieux que le continu | **tombe** | attendre reduirait le volume ; le continu concurrencerait le chemin servi. Les deux cedent : le volume ne depend pas de la cadence, la garde d'inactivite regle la contention ; et le lot B de la phase 2 juge deja toutes les 6 h | M-N13 ; en attendant, cadence parametree (4.4) |
| H2 | Le tri par memoire vaut son cout face au voisinage seul | indecidable | des cles libres de Qwen se recouperaient entre memoires ecrites a des mois d'ecart | M-N11, M-N5 amende |
| H3 | La dedup en simple proposition ne laisse pas grossir les doublons | **tombe** telle qu'ecrite | la passe du soir retirerait les doublons ; or `dry_run` ne rend aucune paire et le consensus hors decisions n'existe pas | M-N12 (volume) |
| H4 | Les liens types ameliorent le classement servi | **tombe** | un lien nouvelle vers ancienne ferait remonter la nouvelle ; le cache ignore les liens et, cote serveur, expansion, PageRank et reconsolidation jouent pour l'ancienne | M-N14 |
| H5 | Qwen suffit pour le tri | indecidable | volet contexte de 16k : tient sur les donnees (6 663 caracteres au plus) ; volet cles de faits et JSON : non mesure | M-N5 amende |
| H6 | Le budget de 3,5 USD par soir et environ 150 USD en global tient | tient, sous condition | « nouvelles memoires » = toutes les insertions ; avec `dreamer-growth`, le budget tombe | portee `is_archived = 0`, M-N4 |
| H7 | L'API Batch est compatible avec la passe du soir | tient | un seul aller-retour par nuit ; il en faut deux, et le cout n'est connu qu'au retour | M-N10 |

Le challenge note un desaccord qui couterait un refactor : si P et H2 survivent toutes
deux a leur test, le choix de forme (tri separe ou replie, juge partout ou hybride) va
au consultant.

---

## 12. References

- Kleos : #30297 (detection parfaite, nouvelle servie 5 -> 6 / 40), #30298 (D1, D2),
  #30300 (cartographie des liens et des sites LLM), #30301 (anomalies), #30276 et
  #30283 (juge de la phase 2 et correction), #18697 et #19072 (latence comme raison
  d'etre de kleos-cache), #19086 (mode hybride du cache), #10270 et #10295 (reranker
  CPU puis iGPU), #30170 (hote Ollama 192.168.10.50).
- Code : `kleos-lib/src/intelligence/{linker,duplicates,reconsolidation,contradiction,temporal,reflections,scheduler}.rs`,
  `kleos-server/src/dreamer.rs`, `kleos-server/src/routes/memory/mod.rs`,
  `kleos-lib/src/memory/{mod,search,scoring}.rs`, `kleos-ingest/src/{writer,summarizer}.rs`,
  kleos-cache `src/{http,index,sync_worker}.rs`, hooks-rs `src/hooks/user_prompt_lean.rs`.
- Etudes : `docs/studies/2026-10-17-supersession-muninn-plan.md`,
  `docs/studies/2026-10-19-supersession-phase2-spec.md`.

---

## Annexe A. Requetes de M-N12 (copie figee, lecture seule, non executees)

Redigees par le challenger, a lancer par `q.sh` sur
`/root/supersession-bench/tenant2-20261018-082511.db`.

```sql
-- R1a: supersedes per month, dedup-like vs /correct-like
SELECT substr(created_at,1,7) mon, COUNT(*), SUM(similarity<1.0) dedup_like, SUM(similarity>=1.0) correct_like
FROM memory_links WHERE type='supersedes' GROUP BY mon ORDER BY mon;
-- R1b: creation gap between the two memories, same source, newer kept
SELECT CASE WHEN g<60 THEN 'a<1m' WHEN g<3600 THEN 'b<1h' WHEN g<86400 THEN 'c<1d' ELSE 'd>=1d' END gap,
       COUNT(*), SUM(same_src), SUM(newer_kept)
FROM (SELECT abs(julianday(ms.created_at)-julianday(mt.created_at))*86400 g, (ms.source=mt.source) same_src,
             (ms.created_at>=mt.created_at) newer_kept
      FROM memory_links ml JOIN memories ms ON ms.id=ml.source_id JOIN memories mt ON mt.id=ml.target_id
      WHERE ml.type='supersedes' AND ml.similarity<1.0) GROUP BY gap ORDER BY gap;
-- R1c: lag between the newer memory and the link
SELECT CASE WHEN lag<3600 THEN 'a<1h' WHEN lag<86400 THEN 'b<1d' ELSE 'c>=1d' END, COUNT(*)
FROM (SELECT (julianday(ml.created_at)-julianday(max(ms.created_at,mt.created_at)))*86400 lag
      FROM memory_links ml JOIN memories ms ON ms.id=ml.source_id JOIN memories mt ON mt.id=ml.target_id
      WHERE ml.type='supersedes' AND ml.similarity<1.0) GROUP BY 1;
-- R3: new memories per day and near-duplicates at cosine >= 0.9
SELECT substr(m.created_at,1,10) d, COUNT(*) n_new,
  SUM(EXISTS(SELECT 1 FROM memory_links ml WHERE ml.source_id=m.id AND ml.type='similarity' AND ml.similarity>=0.9 AND ml.target_id<m.id)) dup09
FROM memories m WHERE m.created_at>=date('2026-10-18','-60 day') GROUP BY d ORDER BY d;
```
