# Étude d'architecture v2 : jugement différé, état de traitement et livraison

Statut : **architecture v2 rédigée le 2026-10-20 après challenge et mesures consignées
parmi M-N1 à M-N15 ; conception seulement, aucun code livré par cette étude**.
M-N9, M-N10 et M-N14 restent à faire ; M-N11 n'est pas mesurable tel que posé,
M-N12 reste partielle et complétée par M-N12 bis. La section 4 et ses
conséquences en sections 5, 6, 9 et 10 remplacent le premier jet. Les historiques des
sections 7, 8 et 11 sont conservés ; leurs amendements sont explicites. Les résultats
cités sont ceux des mesures consignées, non des mesures réexécutées pour la v2.
Les hypothèses et portes restantes figurent en section 13.

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
     d'**importance la plus haute**, la plus recente seulement a importance egale
     (`duplicates.rs:78-88`, Patch 60, en prod depuis le 2026-10-19 ; avant, l'ancienne
     a egalite) : une decision recente d'importance plus basse, proche de l'ancienne
     (negation, retour en arriere), est retiree au profit de l'ancienne. Cadence :
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
second terme : un lien type ne change pas ce que le cache sert (point 5). Amendement
v2 : M-N8 invalide le pointeur sans texte comme levier sur cette fixture ; livrer le
contenu de la remplaçante devient une hypothèse à mesurer, pas un effet acquis du lien.

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

1. **Servir tout de suite, juger plus tard.** Le store reste immédiatement servable ;
   les jugements coûteux sont hors du chemin de la requête (proposition, M-N13).
   Reporter au soir ne supprime pas les nouvelles mémoires à juger. Le nombre réel
   d'appels dépend toutefois des regroupements, changements de voisins et relances ;
   l'hypothèse d'un groupe par mémoire sert au budget, pas à garantir l'idempotence
   ou un coût constant (M-N4, sections 4.2 et 6).
2. **Les candidats alimentent le juge ; le juge annote, le signal explicite autorise
   le retrait.** M-N3 exclut le consensus comme autorisation. La déduplication existante
   conserve Patch 60 : M-N12 et M-N12 bis justifient un jugement de contenu du résidu,
   mais aucune nouvelle classe de retrait automatique dite sûre (N8, section 4.7).
3. **L'idempotence est un contrat à prouver**, pas un effet acquis d'une table : version,
   empreinte de toutes les entrées et application transactionnelle sont proposées en
   4.2 (hypothèse de conception ; N1 et N5 motivent cette exigence).
4. **Un faux retrait est pire qu'un retrait manqué.** Dans le nouveau pipeline, seul un
   signal explicite de l'agent ou de l'opérateur autorise un retrait (M-N3). Un jugement
   seul ne change ni `is_superseded`, ni `is_latest`, ni la visibilité. Cette règle ne
   prétend pas modifier les retraits existants de Patch 60 (M-N12 bis, N8).
5. **Additif d'abord** : tables par canal overlay (Patch 41), modules nouveaux, taches
   hors DAG. Les patchs dans du code upstream sont listes en section 10.

---

## 4. Architecture cible v2

**DÉDUIT des mesures M-N3, M-N5 et M-N8 :** il faut séparer production de candidats,
annotation, autorisation de retrait et livraison du contenu. Ni un consensus de juges,
ni des clés de faits libres, ni un pointeur sans texte ne suffisent aux portes mesurées.
Les paragraphes « proposition » ci-dessous sont des hypothèses de conception à valider,
pas des capacités déjà livrées.

### 4.1 Écriture et marquage en direct

**Proposition fondée sur M-N2, M-N13 et M-N15 :** garder le store immédiatement servable,
sans appel frontier bloquant. S'il existe un candidat de remplacement du même propriétaire
et du même espace, retourner son id, sa date et un extrait borné, avec une action
explicite de confirmation. L'extrait de 150 caractères reste une hypothèse ergonomique,
pas une mesure de précision. L'agent peut confirmer, refuser ou s'abstenir ; le silence
n'autorise jamais un retrait (M-N3).

**DÉDUIT de M-N2 et M-N13 :** le marquage cible une population largement couverte
(au moins 99,8 % de sources attribuées aux agents) et des changements rapprochés (62 %
des candidats M1 à moins d'une heure). Cela ne prouve ni que les deux écritures viennent
de la même session, ni que l'agent confirme correctement. Sa précision reste à mesurer.

**Résultat consigné M-N15 :** 54 stores avec candidat sur 1 841 ; coût estimé à 0,005 %
de la fenêtre de quota en moyenne, 0,41 % dans le pire scénario testé. Le coût ne bloque
pas cette piste ; la couverture des futures écritures kleos-ingest reste inconnue
(M-N2). Une confirmation doit nommer exactement ancienne et remplaçante, conserver sa
provenance et repasser le validateur de 4.6 avant toute mutation (proposition, M-N3).

### 4.2 État du traitement et annotations, sans table de faits

**Proposition motivée par N1, N5 et M-N5 :** un overlay tenant
`memory_pipeline_state` pour le travail, une zone d'annotations pour les résultats ;
aucune étape `triage`, aucune table de faits nouvelle. La forme SQL reste à spécifier.

| Objet proposé | Contrat à prouver | Motivation |
|---|---|---|
| Identité du travail | Propriétaire, espace, mémoire racine ou groupe, étape, version du contrat modèle/prompt/candidats | N3 interdit de supposer un id global entre shards ; D1 impose les frontières |
| Empreinte d'entrée | Contenus, ids, dates et états pertinents de tous les membres, ainsi que l'ensemble des candidats ; pas seulement le texte de la racine | N1/N5 : ne pas rejouer indéfiniment ; hypothèse : un voisin ajouté ou modifié invalide le jugement |
| État d'exécution | En attente, en cours avec reprise bornée, terminé, erreur ou différé ; seul un succès validé pour l'empreinte courante clôt le travail | M-N4 : 1 JSON invalide sur 20 ; une erreur ne vaut pas « déjà traité » |
| Résultat | `kind`, `change_marker`, `title`, verdicts par paire, preuve textuelle, modèle/version ; aucun écrasement automatique de catégorie ou importance | M-N5 : accord insuffisant pour transformer le tri en vérité |
| Journal de coût | Tentatives et lots identifiables, dépense réelle et réservation en cours ; ne pas perdre le coût d'un échec ou d'une ancienne version | M-N4 et M-N10 ; hypothèse de comptabilité nécessaire au plafond |

**Contrat proposé, non prouvé :** réserver le travail, appeler le modèle hors transaction,
puis revérifier empreinte et droits avant de publier résultat et état atomiquement. Un
résultat Batch devenu périmé est compté mais non appliqué. Un second passage sur les
mêmes entrées doit faire zéro appel ; un nouveau voisin doit rouvrir le travail. Une
réponse perdue après facturation peut provoquer un nouvel appel : ne pas promettre
l'exactement-une-fois côté fournisseur (hypothèse à tester, M-N10).

**DÉDUIT du premier état des lieux, N1/N5 :** `updated_at` n'est pas une version de
contenu fiable ; le contrat utilise des empreintes. Suppression, oubli ou archivage
invalident aussi les annotations et références dérivées ; ne pas reproduire N7.

### 4.3 Transcripts en attente : extension distincte

**Proposition sous D6, non mesurée par M-N2 :** kleos-ingest transmet en journée un
transcript borné vers une zone `transcript_staging`, isolée des mémoires servables et
de la boîte de revue. Le serveur produit ensuite résumé et mémoires, qui entrent dans
le même voisinage. Réception, traitement, réessai et purge doivent être identifiables
par session et empreinte (hypothèse de conception).

**Limite de périmètre :** cette extension n'est pas un préalable au marquage en direct
ni au voisinage des stores actuels (M-N2). Son volume, sa qualité et sa rétention sont
à mesurer séparément ; M-N6 mesure du tri, pas du résumé. Le défaut d'authentification
et le résumé client décrits par la phase 0 du plan de supersession (section 4) restent
des préalables à revérifier
avant cette réactivation, pas des travaux déclarés achevés ici.

**Décision opérateur du 2026-10-20, complément à D6** (source consultée :
`memory_get(30938)`, décision rapportée, sans nouvelle mesure de sécurité) : la sécurité
des transcripts n'est pas un frein à cette cible. Les transcripts doivent être stockés
côté serveur, éventuellement réduits, puis traités en tâche de fond pendant l'inactivité.
La première marche est la carte **must `3f45e0f8`**, portage Windows de kleos-ingest ;
la cible serveur reste suivie par **`f18dc3c4`**. Cet arbitrage ne déclare pas le staging
ni le traitement serveur déjà implémentés. **DÉDUIT du périmètre actuel rapporté par
kleos-7 :** le démon d'extraction lexicale, POST `/memories` et résumé Ollama local
n'est pas cette cible. Kleos #21290 consigne qu'il n'a jamais été déployé ; un binaire
compilable n'est donc pas une preuve de déploiement. Voir l'[étude d'usage](2026-10-20-kleos-agent-usage.md), section 5.
**Cible détaillée (2026-10-21) :** [étude cible ingestion des transcripts](2026-10-21-kleos-transcript-ingest-target.md), qui place `transcript_staging` dans un relais à côté du serveur plutôt que dans Kleos (verdict du consultant, section 1.3 de cette étude).

### 4.4 Cadence, priorité et budget

**Décision D2 amendée, étayée par M-N13 :** une cadence paramétrée sous la garde
`dream_idle_threshold_secs`, pas une fenêtre du soir pour le flux courant. Le code
actuel saute le tick occupé (`kleos-server/src/dreamer.rs:193-206`, DÉDUIT). Cette garde
contrôle le départ ; elle ne prouve pas une absence de contention pendant tout un lot.
La valeur de cadence et le délai maximal avant annotation restent des paramètres à
mesurer, pas une garantie de traitement le jour même.

**Ordre proposé (hypothèse, M-N1/M-N13) :** confirmations explicites dès leur réception,
puis voisinages du flux courant, puis arriéré en fenêtre du soir. Les futurs transcripts
ont une file bornée distincte pour ne pas affamer le courant. Une seule logique de
jugement traite courant et arriéré ; priorité et budget diffèrent.

**Contrat de budget proposé (M-N4/M-N10) :** plafond quotidien et enveloppe d'arriéré
séparés, avec un plafond mensuel commun au flux courant. Dépense engagée = coût des
tentatives terminées + réservations des appels ou lots en cours. Réserver avant envoi,
rapprocher au retour, différer ce qui dépasse ; ne pas compter seulement les résultats
valides. La précision des réservations reste à éprouver en M-N10.

**Batch est une option de transport pour l'arriéré, non une condition du flux courant.**
La remise de moitié retenue par H7 sert à l'estimation, pas à une facture observée sur
le déploiement. M-N10 doit valider le passage LiteLLM, les délais, la récupération après
redémarrage et le rapprochement des coûts avant l'arriéré global. La v2 supprime la
chaîne tri puis voisinage : un seul jugement par groupe, hors relances (M-N5).
Une réponse peut être reçue hors fenêtre, mais son application garde les contrôles de
version et de portée de 4.2 ; aucune exemption implicite à l'ordonnanceur n'est requise.

### 4.5 Un seul appel : métadonnées et jugement de voisinage

**Décision issue de M-N5, H2 et H5 :** supprimer la passe de tri séparée et la table de
faits envisagée. `kind`, `change_marker` et `title` sont des champs de sortie du juge
de voisinage, dans la même annotation que les verdicts par paire. Ils ne deviennent
ni une nouvelle taxonomie obligatoire, ni des critères automatiques de retrait.
Le champ `durable` ne filtre pas les groupes : son accord Qwen/Sonnet mesuré est de
7 % (M-N5). Pas de modification automatique de l'importance.

**DÉDUIT de M-N5 :** les clés libres n'ont relié que 3 vraies paires sur 36 avec Qwen,
1 avec Sonnet, dans le contrat testé à un fait par mémoire. Cela suffit à abandonner
ce canal dans cette v2, pas à prouver qu'aucune extraction de faits ne fonctionnerait.
M-N11 reste à refaire sur une vérité terrain indépendante : abandonner les faits ne
prouve pas que `similarity` + M1 ont un rappel suffisant.

**Hypothèse à mesurer, M-N4 :** ajouter ces trois champs conserve un coût voisin de
0,0151 USD par groupe. Le contrat élargi doit être rejoué sur 200 mémoires avant de
figer son budget ; une sortie invalide ne produit aucune annotation partielle appliquée.

### 4.6 Voisinage : le juge propose une relation, pas un retrait

**Proposition fondée sur M-N4/M-N11 :** assembler la mémoire racine et les candidats
`similarity`, M1 et déduplication dans le même propriétaire/espace, puis les trier par
date. Les chaînes de changements sont jugées ensemble, selon 6.1 bis du plan de
supersession. Ne pas recréer un canal de candidats par faits (M-N5). Le bornage des gros
groupes sans casser une chaîne reste une hypothèse à tester sur le complément de M-N4.

**Modèle de référence : Sonnet `medium`** sur le voisinage, décisions et autres contenus.
M-N4 mesure 0,0151 USD par groupe sur 20 groupes ; l'effort `medium` est consigné dans
Kleos #30835, consultée pour cette réécriture. Ce coût ne prouve pas une précision
suffisante pour retirer. Schéma de sortie, validation des ids et relance bornée sont requis : 1 groupe
sur 20 était invalide. La limite de relance est à fixer avant implémentation.

| Verdict proposé | Annotation conservée | Mutation automatique par le juge |
|---|---|---|
| `doublon`, `remplace` | Couple orienté et justification ; confirmation explicite requise | Aucun retrait, aucun lien `supersedes` (M-N3) |
| `met_a_jour`, `partiel` | Point modifié, références et preuve textuelle ; les mémoires restent visibles | Aucun `updates`, aucune baisse d'importance (N1/N9) |
| `contredit` | Conflit ouvert avec les deux références, sans autorité de retrait | Aucun `contradicts` (N1/N9) |
| `prolonge` | Verdict de diagnostic seulement | Aucun `extends` : 102/275 paires en M-N4, environ 5 par mémoire |
| `distinct` | Résultat de traitement pour éviter un rejeu inutile | Aucun lien, aucun retrait (M-N4) |

**Frontière impérative N1/N9 :** aucune nouvelle écriture de lien typé par ce pipeline
avant correction des deux nœuds. Stocker les références dans les annotations, pas dans
un type de lien improvisé que certains lecteurs pourraient interpréter. Même après
correction, M-N14 doit mesurer l'effet serveur ; un gain de classement n'est pas acquis
(H4). Le cache ne devient pas sensible aux liens par cette seule correction.

**Signal explicite, proposition motivée par M-N3 :** sa réception autorise une demande,
non un contournement des prérequis. Tant que N1/N9 bloquent l'application des liens,
conserver la confirmation en attente, sans mutation partielle des flags. Après ces
prérequis, l'application doit être atomique et validée : propriétaire/espace identiques,
contenus encore conformes à la confirmation, remplacement total du plus récent vers
le plus ancien, aucun cycle, au moins une mémoire active par chaîne, aucune résurrection.
Ces invariants reprennent 6.1 bis ; leur réalisation reste à tester. Un conflit partiel
ne devient jamais un retrait total parce qu'il est confirmé.

### 4.7 Déduplication : conserver Patch 60, examiner le résidu

**DÉDUIT du code `duplicates.rs:78-109` et de N8 :** Patch 60 conserve la plus haute
importance, puis le plus grand id à égalité, et écrit déjà un retrait et un lien
`supersedes`. La v2 ne modifie pas cette règle ni ses écritures existantes. L'interdiction
de 4.6 vise les nouveaux producteurs du pipeline, pas une désactivation implicite de
Patch 60 ou de `/correct`.

**Résultats consignés M-N12/M-N12 bis :** environ 3 retraits fautifs sur 25 étiquetés
avec la règle actuelle ou son rejeu, sans séparation fiable par date ou écart
d'importance (N8). Ne pas présenter ce résidu comme une classe sûre, ni 3/25 comme une
estimation représentative de toute la base. La variante « date prioritaire partout »
et la mise en simple `dry_run` sont écartées ; ce dernier ne renvoie qu'un compte
(`duplicates.rs:68-74`, DÉDUIT).

**Proposition de première étape :** faire examiner les candidats et retraits de
déduplication par le même juge de contenu, consigner l'erreur suspectée, demander une
validation explicite ; ne pas réactiver automatiquement une mémoire ni inverser un
lien (M-N3/N8). Le périmètre `is_archived = 0` n'interdit pas cet audit des retirées
non archivées ; il ne les rend pas servables (M-N1/M-N12).

**Limite explicite :** un audit après coup n'empêche pas le retrait fautif. Intercepter
avant retrait les paires suspectes nécessiterait une politique de suspension et un
chemin d'application nouveaux. Aucune règle simple ne sachant isoler le résidu, ce
choix est ouvert en section 13, pas dissimulé sous le mot « hybride » (N8/M-N12 bis).

### 4.8 Portée et séparation des espaces

**D1 et M-N1 :** même propriétaire et même espace à toutes les étapes, y compris dans
les annotations, confirmations et redirections. Le traitement global `default` / NULL
est séparé des espaces projet ; aucune comparaison croisée. La sélection coûteuse
exclut `is_archived = 1`, notamment `dreamer-growth` (M-N1).

**Hypothèse de migration budgétaire :** commencer l'arriéré par les 7 623 actives, puis
traiter les autres non archivées en audit, dans une enveloppe distincte. Actives et non
archivées ne sont pas synonymes : 12 043 non archivées coûteraient environ 182 USD en
synchrone au coût M-N4 (section 6). Ne pas lever les filtres de visibilité pour les juger.
Le libellé « actives » reprend celui du comptage ; le prédicat SQL exact ayant produit
7 623 n'est pas documenté ici. Le retrouver et le reproduire avant de sélectionner
l'arriéré, sans l'assimiler implicitement à `is_archived = 0` ni à « servables ».

### 4.9 Livraison : redirection à éprouver, pas pointeur sans texte

**Résultat consigné M-N8 :** dates et pointeur nu laissent `agent_ok` à 4/40 dans les
trois variantes ; les dates augmentent même l'ancienne citée de 11 à 13. Le pointeur
sans texte est abandonné comme levier d'amélioration. Un pointeur conservé dans une
annotation est une référence pour revue, pas une promesse d'effet sur l'agent.

**Hypothèse R, non démontrée par M-N8 :** quand la recherche aurait sélectionné une
mémoire retirée, remplacer cette place par le contenu de sa remplaçante validée, avec
l'id et la provenance de celle-ci. Ne jamais rediriger sur la seule prédiction du juge,
ni transformer `partiel` ou `met_a_jour` en remplacement total (M-N3/N8).

**Deux options à comparer :**

| Option | Coût et risque estimés | Réversibilité et périmètre |
|---|---|---|
| A. Filtrage actuel, aucune redirection | Pas de nouvelle projection ; la nouvelle peut rester absente malgré le retrait, comme dans la fixture 4.4 | Aucun changement du chemin servi ; témoin obligatoire de M-N16 |
| B. Résolution vers la remplaçante validée | Projection/résolveur supplémentaire, risque de mauvaise destination ou d'entrée périmée | Activation indépendante ; impact cache et rendu, sans nouveau retrait ; hypothèse à mesurer |

**Recommandation :** tester B contre A sans l'activer en production ; conserver A tant
que M-N16 n'a pas passé ses portes. Le besoin de livrer le texte, plutôt qu'un id,
est motivé par M-N8, mais le mécanisme reste non validé.

**Contrat candidat (hypothèse, N2/N7/N8) :** la mémoire retirée n'est jamais réinjectée
comme contenu. Sa trace de recherche ne peut servir que d'alias vers une cible active,
accessible, du même propriétaire/espace ; cible absente, oubliée, archivée, boucle ou
ambiguïté = aucune redirection. Dédupliquer les destinations avant `top_k`, borner la
chaîne et conserver le même budget de contexte. Les retraits Patch 60 sans confirmation
ne sont pas une preuve suffisante de remplacement sémantique (N8).

**DÉDUIT du plan de supersession, 5.1 lignes 434 et 443-447 :** sur le banc de phase 1,
les 40 anciennes sortent de l'index après synchronisation. Un simple remplacement
après `top_k` ne peut donc pas y résoudre ces anciennes. Le mécanisme de récupération
est un point architectural à mesurer avant code (N2, M-N8). La fixture doit comparer le vrai index filtré à un mécanisme explicite
d'alias, pas injecter artificiellement la bonne réponse dans le bloc. La réplication
brute `/list` doit continuer à transmettre les tombstones tandis que les lecteurs
agents restent filtrés (N2, Patch 67). Reranker et dates ne sont pas des prérequis de
cette expérience ; M-N9 reste séparée.

### 4.10 Migration proposée et portes

1. **Cadre et signaux :** préserver les garanties phase 1/Patch 67, spécifier les
   confirmations explicites et l'état transactionnel, puis éprouver le second passage
   sans appel et les résultats périmés (N2, M-N3, M-N4).
2. **Observation :** lancer le voisinage Sonnet sous garde d'inactivité, annotations
   seules et budget borné, sans table de faits ni retrait nouveau ; mesurer le contrat
   élargi et le rappel indépendant des candidats (M-N4, M-N5, M-N11, M-N13).
3. **Application contrôlée :** corriger N1/N9, vérifier les invariants et la provenance
   des confirmations avant le moindre nouveau lien typé ; M-N14 reste une porte
   distincte pour revendiquer un gain serveur.
4. **Livraison et volume :** éprouver M-N16 avant la redirection ; valider M-N10 avant
   l'arriéré global Batch. Transcripts et éventuelle suspension pré-déduplication sont
   des lots ultérieurs, pas des conditions cachées du premier lot (M-N2/N8).


---

## 5. Routage des modèles v2

| Travail | Routage proposé | Base et limite |
|---|---|---|
| Voisinage, décisions et autres mémoires, résidu de déduplication | Sonnet `medium`, un appel par groupe avec métadonnées et verdicts | M-N4 mesure le coût ; M-N3 interdit d'en déduire une autorité de retrait |
| Confirmation au store | Agent déjà présent, action explicite ; pas de second juge obligatoire | M-N2/M-N13/M-N15 couvrent population, moment et coût, pas encore la précision |
| Résumé de transcript | Qwen reste candidat, non validé pour ce nouveau contrat | D4/D6 ; M-N5/M-N6 ne mesurent pas des résumés |
| Tri ou extraction de faits séparés | Aucun appel, aucune escalade dédiée | H2/H5 tombées, M-N5 |
| Double jugement par consensus | Pas de second appel systématique pour autoriser un retrait | Consensus réfuté par M-N3 |

**Options :** Sonnet partout dans le voisinage fournit une référence de coût mesurée
(M-N4), au prix d'une facture supérieure à un routage mixte. Haiku pour les non-décisions
réduirait possiblement le coût, mais sa qualité sur ce contrat n'est pas mesurée ; son
activation serait réversible par configuration, avec un risque sur toutes les
annotations hors décisions. **Recommandation :** garder Sonnet comme référence tant que
le plafond tient, mesurer Haiku avant toute substitution. Le facteur décisif est la
qualité non démontrée, pas la seule taille de contexte (M-N4/M-N5).

**Proposition additive :** client et paramètres propres au juge, inspirés du
`conflict_judge` de la spec phase 2 ; laisser `KLEOS_LLM_*` aux sites actuels de Qwen.
Fixer explicitement l'effort testé, le schéma et les limites de sortie ; ne pas supposer
que `KLEOS_LLM_THINK` se transpose à Anthropic via LiteLLM. Compatibilité du transport
et facturation sont à vérifier dans M-N10, sans changement du client partagé.

---

## 6. Budget

**Base v2 : M-N4, pas un facteur appliqué aux composantes de phase 2.** Le rapport
consigne 20 groupes, 275 paires, 0,302 USD au total, soit 0,0151 USD par groupe en
moyenne et 0,0163 au p90. Un groupe a produit un JSON invalide. Ces résultats mesurent
le contrat testé, pas le contrat élargi de 4.5 ni le coût des reprises en production.

Les anciens coûts phase 2 (environ 0,00065 USD par composante Haiku et 0,0049 Sonnet)
ne sont plus utilisés pour chiffrer le voisinage. Ni tri séparé ni deuxième juge par
consensus ne figurent au budget v2 (M-N3/M-N5).

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

**Portée maintenue : `is_archived = 0` (M-N1).** Inclure 842 écritures `dreamer-growth`
par jour ajouterait environ 12,7 USD/jour au coût moyen M-N4, avant les autres sources
(calcul extrapolé, non facture). Le volume futur de kleos-ingest reste non mesuré.

**Synthèse quotidienne, calculs extrapolés de M-N1 x M-N4 :** supposer un groupe par
nouvelle mémoire, sans rejeu ni relance, donne environ 1,66 USD pour 110 mémoires par
jour ouvré et 2,94 USD au pic de 195. Sur 22 jours ouvrés, environ 36,5 USD, auxquels
s'ajoutent week-ends, reprises, invalidations de groupes et futurs transcripts. Le
plafond de 100 USD/mois reste une limite effective à imposer, pas une garantie donnée
par cette extrapolation. L'ajout de métadonnées et le regroupement réel demandent le
complément de M-N4.

**Synthèse de l'arriéré, estimations au coût M-N4 :**

| Portée | Sonnet synchrone | Batch si remise effective de moitié | Face à D5, environ 150 USD |
|---|---|---|---|
| 7 623 actives | environ 115 USD | environ 58 USD | Synchrone compatible avant reprises ; Batch à valider par M-N10 |
| 12 043 non archivées | environ 182 USD | environ 91 USD | Synchrone incompatible ; Batch conditionnel |
| Archivées | Hors périmètre | Hors périmètre | Ne pas les réintroduire pour remplir un lot |

**Correction explicite de H6 :** exclure les archivées est nécessaire, mais ne suffit
pas à rendre le global Sonnet synchrone compatible avec 150 USD. Le coût plafond par
groupe est environ 0,0197 USD sur les actives et 0,0125 sur toutes les non archivées ;
la moyenne M-N4 de 0,0151 est entre les deux. Les verdicts historiques sont conservés
en section 11 avec cet amendement.

**Recommandation budgétaire :** flux courant synchrone borné ; arriéré actif en premier,
avec M-N10 avant tout lancement global prévu en Batch. Sans transport Batch validé,
réduire explicitement la portée ou demander un budget révisé, jamais consommer les
182 USD sous une autorisation de 150. Pas de durée de tri Qwen au planning v2 : M-N6
reste une mesure conservée, mais l'étape est supprimée (M-N5).

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
| N8 | La dedup garde l'importance la plus haute ; la date ne joue qu'a egalite (`duplicates.rs:78-88`) | **mesure sur la copie figee, ANTERIEURE au Patch 60** : a egalite (852 paires sur 904) elle gardait l'ancienne ; 9 des 20 retraits de la fenetre ont garde la mauvaise version. Le Patch 60 (2f592b75, prod le 2026-10-19, liens repares par le Patch 61) en evite 8 (deduit). **Residuel sous Patch 60 : 2 sur 20** : la paire 1 (importance 9 contre 8, la plus recente retiree) et la paire 14 (distinctes, ecart de 1,5 min) | **Re-mesure M-N12 bis** (copie `tenant2-20261020-104711`, les 5 retraits poses depuis le Patch 60, Kleos #30860) : 4 O, 1 P, le P ayant la forme de la paire 1 (8 contre 7, la plus recente retiree). Sur les 6 cas ou l'ancienne gagne par l'importance : 2 P, 4 O ; a ecart de 1 : 2 P, 3 O. Aucune regle simple (date, ecart d'importance) ne les separe : **pas de patch de regle** ; le residuel (environ 3 fautifs sur 25 etiquetes) va au juge du contenu (4.6, 4.7) |
| N9 | PageRank regroupe par `MAX(ml.type)` (`pagerank.rs:58`) et garde `contradicts` a 1,3 (`pagerank.rs:17`) | les types `extends`, `contradicts`, `corrects`, `caused_by` disparaissent de PageRank des qu'un lien `similarity` existe ; ecart avec le Patch 66 | a traiter si des liens types sont ecrits (4.6) |

Challenge (H4) : ces noeuds, N1 et N9 surtout, signifient que les liens types ecrits
aujourd'hui joueraient contre l'intention. Ils sont bloquants pour toute ecriture de
lien type, pas seulement pour `contradicts`.

**Amendement v2, historique conservé :** N2/M-N7 a été corrigé par Patch 67 ; déploiement
confirmé dans le brief du lead, non remesuré par l'architecte. La documentation du patch
(`docs/dev-notes/local-patches.md:7063-7089`) distingue le brut de réplication, le filtre
serveur et les lecteurs agents (DÉDUIT). N7 reste pertinent pour les annotations et
références, mais sa mention d'une future table de faits est caduque (M-N5). N1/N9
bloquent les nouveaux liens du pipeline ; ils ne désactivent pas les producteurs
existants de Patch 60 (4.7).

---

## 8. Mesures a faire avant tout code (ordre propose)

| # | Mesure | Sur | Decide |
|---|---|---|---|
| M-N1 | nouvelles memoires par jour, par source, par categorie, par espace | copie figee, SQL | le budget quotidien et l'echelle de tout le reste. **Faite le 2026-10-20** : 2 515 non archivees en 31 jours (81 par jour calendaire, 110 par jour ouvre, 20 le week-end) ; discovery 1 249, decision 369, state 357, issue 282, task 198, reference 52 ; 15 espaces, koryphaios 1 273 en tete, `default` 184, aucun NULL ; la portee `is_archived = 0` exclut tout `dreamer-growth` |
| M-N2 | part ecrite par un agent (MCP, CLI) face aux chemins automatiques | copie figee | la portee du marquage en direct (4.1). **Faite** : 2 511 `claude-session` et 4 `unknown`, aucune autre source non archivee ; aucun hook n'ecrit sous `claude-session` (deduit des hooks lus) ; part agent 99,8 % ou plus |
| M-N3 | precision des retraits proposes : paires 6, 20, 23 de la feuille operateur, puis un echantillon des 12 autres | etiquetage operateur | si un retrait automatique est possible. **Faite le 2026-10-20 sur les 3 premieres** : 6 R, 20 R, 23 D, soit 2 sur 3 et un faux retrait (paire 23). Le consensus tombe comme critere de retrait automatique (regle de la spec phase 2 : un seul `distinct` suffit) ; Kleos #30829 |
| M-N4 | cout reel et verdicts d'un groupe de voisinage, Haiku et Sonnet ; d'abord 20 groupes reels en Sonnet, puis 200 memoires | copie figee via LiteLLM | la repartition des modeles et le cout du global ; criteres en section 6. **20 groupes faits** (2026-10-20, Kleos #30835) : 275 paires, 0,0151 USD par groupe en moyenne, p90 0,0163 ; quotidien environ 1,7 USD par jour ouvre ; global Sonnet partout environ 115 USD sur les actives (sous 150), environ 182 sur les non archivees (au-dessus). Verdicts : distinct 121, prolonge 102, met_a_jour 37, partiel 11, doublon 4, remplace 0, contredit 0. JSON invalide 1 groupe sur 20 : schema impose et relance obligatoires. `prolonge` a environ 5 par memoire : ne pas l'ecrire en lien |
| M-N5 | accord Qwen / Sonnet sur le tri, 200 memoires ; plus, sur les 40 paires de la fixture 4.4 et les 3 paires `remplace` de l'operateur, la part des paires qui partagent une cle `(sujet, attribut)` normalisee, et le taux de JSON valide sous schema impose | idem | ce que Qwen peut garder ; critere : >= 50 % de paires reliees, >= 0,8 fois Sonnet, >= 99 % de JSON valide. **Faite** (2026-10-20, Kleos #30852) : JSON valide Qwen 92,3 % sous schema impose (critere manque), Sonnet 100 % avec un contrat de sortie compact ; cles partagees sur 36 vraies paires Qwen 3, Sonnet 1 (borne basse : un seul fait par memoire) ; accord sur 100 memoires kind 81, importance a +-1 73, change_marker 62, durable 7. Qwen ne garde pas le tri en l'etat, et le canal par faits libres ne relie presque rien, quel que soit le modele |
| M-N6 | debit de Qwen (appels par heure) avec `dreamer-growth` actif, sur un vrai appel de tri | Ollama 192.168.10.50 | la duree du retraitement global. **Faite** : 1 587 appels par heure en sequentiel, p50 1,9 s, p90 3,8 s ; tri des 12 043 non archivees environ 7,6 h, des 7 623 actives environ 4,8 h |
| M-N7 | fuite de `memory::list` (N2) | banc | l'extension de la phase 1. **Faite** (2026-10-20, Kleos #30857) : FUITE confirmee. GET /list (`routes/memory/mod.rs:953`) est lu par le hook de demarrage (`session-start-kleos.sh:393`, `kleos-cli list --limit 5`, bloc RECENT MEMORIES, drapeau non affiche) et par l'outil MCP `memory_list` (`kleos-client/src/routes.rs:196-203`). Banc : 1 retiree sur 5 sans espace, 48 sur 1000 ; 0 sur 5 avec l'espace `kleos` ce jour-la. `/recall` ne fuit pas |
| M-N8 | `agent_ok` avec dates et pointeurs, rendu fixe | fixture 4.4, API | l'effet du rendu. **Faite** (2026-10-20, Kleos #30868) : blocs voie A rejoues hors ligne, Sonnet medium sans outils. agent_ok regex 4/40 en V0 (sans ajout), V1 (dates) et V2 (dates + pointeur sans texte) ; re-notation Haiku a l'aveugle 4, 5, 5. Les dates font monter l'ancienne citee (11 a 13). **Le pointeur sans texte n'aide pas** : l'agent a besoin du contenu de la nouvelle |
| M-N9 | latence d'un reranker dans le cache | poste | 4.9 |
| M-N10 | delai de l'API Batch : un lot de 100 requetes soumis vers 22 h via LiteLLM, 5 soirs, `ended_at - created_at` | API | 4.4 ; critere : < 2 h les 5 soirs pour garder Batch au quotidien, sinon Batch pour l'arriere seulement |
| M-N11 | rappel ajoute par le canal de faits : part des vraies paires de remplacement (40 de la fixture 4.4, 39 chaines de la phase 2) qu'aucun lien `similarity` ni le detecteur M1 n'atteint | copie figee, SQL | H2 : <= 5 % hors d'atteinte, le tri se replie dans le voisinage ; >= 10 %, Qwen sur ces seules memoires, le tri tient si ses cles en relient la moitie. **Non mesurable tel que pose** (2026-10-20) : les ids de la fixture 4.4 sont posterieurs au gel (29 607 a 29 826, copie au max 29 606), et les 39 chaines de la phase 2 sont toutes des candidats M1 par construction. Indicatif : le canal `similarity` seul rate 2 des 33 paires que la chaine Sonnet classe R (6,1 %, entre les deux seuils). A refaire sur une verite terrain independante de M1 |
| M-N12 | precision et rythme de la dedup : requetes R1a a R1c et R3 (annexe A), puis lecture de 30 paires `supersedes` a `similarity < 1.0` | copie figee, SQL, lecture | principe 2 et H3 : la regle tient si `newer_kept` >= 95 %, ecart < 1 h pour >= 80 % des paires et <= 1 retrait faux sur 30 ; le juge est justifie a >= 3 sur 30 ; H3 tombe en volume a >= 3 retraits dedup par jour ouvre a ecart < 1 h. **Partielle** (2026-10-20) : 853 des 904 liens dedup datent d'un passage de juin sur des traces d'outils `[Bash]`, `[Read]` de mai (source `sidecar`), qui dominent R1b ; le tirage au hasard en est le reflet et ne vaut rien. Flux courant (lien depuis le 2026-07-01) : 51 liens, 48 a ecart < 1 h (94 %), plus recente gardee 6 fois sur 51 (critere de 95 % tres loin, N8) ; dans la fenetre de 31 jours, 20 liens, environ 0,9 par jour ouvre (H3 ne tombe pas en volume). **Etiquetage operateur des 20 liens de la fenetre** (2026-10-20, Kleos #30841) : 10 O, 9 P, 1 D, soit **10 retraits fautifs sur 20**. Dans les 9 P, la retiree est toujours la plus recente : une correction explicite (#19155 corrige #19154), un etat plus recent (#27235), et 3 versions re-stockees par l'agent apres une troncature des backticks par le shell (#19581, #21028, #27186). **Reserve (Kleos #30850)** : la copie figee precede le Patch 60 ; sous la regle deployee, 2 retraits fautifs sur 20 (10 %) au lieu de 10. Le critere de la regle (<= 1 faux sur 30) reste manque ; le juge reste justifie (>= 3 sur 30 par extrapolation) |
| M-N13 | part intra-journee : sur les 194 paires candidates M1 et les 15 retraits du consensus, part dont l'ecart de creation est < 24 h ; puis rejeu de `out-hook-*.jsonl`, blocs servis contenant une memoire remplacee plus tot le meme jour | copie figee, journaux de hook | H1 : < 5 % des blocs et < 10 % des paires, le soir suffit ; >= 20 %, cadence continue au moins pour les decisions. **Partie SQL faite** (2026-10-20) : 161 des 194 candidats a moins de 24 h (83 %, dont 62 % a moins d'une heure), 15 des 15 retraits du consensus (100 %). Seuil de 20 % largement depasse sur les paires ; le volet des blocs servis reste a faire, mais il ne peut plus renverser la cadence continue retenue en D2 |
| M-N15 | cout du marquage en direct sur la fenetre de quota Claude de 5 h : stores retrouves dans les transcripts, drapeau candidat depuis la copie figee, tokens ajoutes ponderes comme le tarif face aux tokens consommes par fenetre | transcripts Claude Code, copie figee | l'activation de 4.1. **Faite** (2026-10-20, Kleos #30845) : 0,005 % en moyenne, 0,41 % au pire ; regle d'ouverture des fenetres approximee, limite absolue du quota inconnue |
| M-N14 | effet de 40 liens `updates` (nouvelle vers ancienne) de la fixture 4.4 sur le chemin serveur (`/search`, `kleos-cli context`), avec et sans | copie du banc, jamais la base vive | H4 cote serveur : tient si la nouvelle servie gagne >= 5 / 40 sans que l'ancienne servie augmente ; cote cache, effet nul par construction |
| M-N16 | utilité du texte sous oracle (a), puis accessibilité réelle après phase 1 (b) | fixture 4.4, rejeu API puis vrai index filtré | **M-N16a faite le 2026-10-20** (MESURÉ, rapport final ; détail 13.1, Kleos #30922) : `agent_ok` V0/A/B = 4/5/11, 4/4/12, 4/5/12 ; `old_cited` = 8/0/0 aux trois passes ; k=7/7 à chaque passe. Verdict de grille **EFFET LOCAL**, seule régression non éligible N10 en passe 1, entrée A/B hash-identique. **Décision opérateur** (Kleos #30923) : mesure considérée réussie, **M-N16b débloqué** avec règle de régression amendée en 13.1 ; utilité du rendu sous oracle seulement, pas validation des chaînes en production |

Contraintes de mesure inchangees : lectures sur la copie figee, jamais la base vive ;
secrets via `cred exec` ; juge via LiteLLM.

---

## 9. Articulation avec le plan de supersession

**Conflit documentaire explicite :** le consensus retenu dans le plan du 17 octobre
(6.1 ter) est réfuté par M-N3. La spec du 19 octobre, section 5 lignes 341-348, bloque
déjà le lot B tel qu'écrit. La v2 ne déclare donc pas cette porte passée et ne réactive
pas le retrait par faits abandonné en 6.0 (M-N5 confirme l'abandon du canal testé).

| Plan de supersession | Reprise dans la v2 |
|---|---|
| Phase 1, garantie structurelle | Préserver le filtrage. N2/M-N7 a reçu Patch 67, corrigé et déployé selon le compte rendu du lead ; le listing brut de réplication reste distinct du listing agent |
| Phase 2 lot A, détecteur et indicateur | M1 produit des candidats ; les références et conflits sont des annotations. Pas de bénéfice agent présumé pour un pointeur nu (M-N8) |
| Phase 2 lot B, jugement | Reprendre le jugement de groupe et les invariants de chaîne de 6.1 bis, pas l'autorisation par consensus. Annotations seules puis signal explicite ; N1/N9 précèdent tout nouveau lien (M-N3) |
| Phase 2 lot C et phase 4, livraison | Remplacer l'objectif « pointeur sans texte » par l'expérience conditionnelle de redirection M-N16 ; cache/hooks ne sont modifiés qu'après ses portes (M-N8) |
| Phase 3, marqueurs | `change_marker` sort du jugement de voisinage ; pas de passe de tri séparée (M-N5) |
| Phase 5, commits git | Hors de cette étude ; aucune nouvelle autorisation de retrait introduite ici |

**Complément d'architecture du 2026-10-20, canal de livraison dominant :**
**MESURÉ par kleos-7, sources #30932/#30933 consultées et détaillées dans
l'[étude d'usage](2026-10-20-kleos-agent-usage.md)** : environ 12 700 récupérations
automatiques par prompt contre environ 1 230 recherches choisies par les agents en
30 jours. Le premier chemin est `UserPromptSubmit -> kleos-cache local /v1/retrieve`,
puis repli sidecar `/recall`, puis CLI. `kleos-cli context` appelle POST `/recall`, pas
POST `/context` ; l'audit #30932 ne trouve aucun appelant de `/context` dans CLI,
sidecar, kleos-sh, hooks et registre MCP (absence rapportée sur ce périmètre, non
recherchée à nouveau ici). **DÉDUIT :** un mécanisme serveur de supersession ou de
redirection n'atteint le chemin principal local que par la réplique kleos-cache ;
une amélioration de `/search` ou `/recall` ne prouve pas une amélioration des injections
locales. Les mesures de livraison doivent couvrir cache, synchronisation et rendu,
au-delà du serveur. M-N16d doit précisément couvrir ce canal avant le nouvel arbitrage.

**Limite de cette livraison :** les deux études précédentes ne sont pas réécrites dans
ce lot documentaire. Toute future spec d'implémentation doit citer M-N3/M-N8 et les
portes v2 plutôt que recopier leur consensus ou leur pointeur sans texte historiques.

---

## 10. Écart upstream estimé, pas encore implémenté

| Niveau proposé | Surface | Limite de la v2 |
|---|---|---|
| Overlay/additif | État, annotations et journal de tentatives dans le canal Patch 41 ; modules de voisinage, validation, confirmation et budget | Aucune migration numérotée upstream, aucun module de tri ni table de faits (M-N5) |
| Raccordement minimal | Job sous garde d'inactivité dans les chemins monolithe et tenants ; retour de candidat au store | Modèle du lot B, mais sans son retrait par consensus ; cadence paramétrée (M-N3/M-N13) |
| Correctifs préalables aux liens | Reconsolidation N1 et PageRank N9 | Reproduire et tester avant d'autoriser les écritures ; pas de refonte du scoring justifiée par H4 |
| Déjà traité, à préserver | N2 par Patch 67, et déduplication Patch 60 | Patch 67 corrigé/déployé selon le lead ; aucun nouveau patch de règle déduplication (M-N7/M-N12 bis) |
| Lots conditionnels distincts | Staging et authentification kleos-ingest ; projection de redirection dans kleos-cache et rendu hooks | Volume/qualité d'ingest à mesurer ; M-N16 avant redirection ; M-N9 avant reranker |

**Recommandation de périmètre :** commencer par l'observation et les confirmations,
pas par une réécriture du graphe ou du cache. Le canal prompt overlay porte le contrat
LLM ; il ne peut pas fournir l'état transactionnel, l'autorisation ou le filtrage de
lecture (hypothèse de découpage, N1/N2/M-N3). Chaque futur site source reste à chiffrer
contre `docs/dev-notes/local-patches.md` avant code ; aucun nombre de lignes de patch
n'est garanti par cette étude.

---

## 11. Hypotheses a attaquer (pour le challenger)

Challenge du 2026-10-20 : deux challengers independants, l'un sur le principe 2 et
H2 a H4, l'autre sur H1 et H5 a H7. Les corrections sont portees dans le corps.

| # | Hypothese | Verdict | Premisse cachee | Test qui tranche |
|---|---|---|---|---|
| P | Les regles proposent, le modele tranche (principe 2) | indecidable | la dedup retirerait a tort assez souvent pour justifier un juge, lui-meme non mesure. Depuis M-N3, le juge par consensus a 1 faux retrait sur 3 : "le modele tranche" ne vaut plus pour un retrait, seulement pour un pointeur ou une annotation. M-N12 tranche cote regle : sous la regle deployee (Patch 60), la dedup se tromperait encore 2 fois sur 20 ; ni la regle seule ni le juge seul ne retirent sans erreur | M-N12 fait ; P devient : la regle propose et retire la classe sure, le juge annote |
| H1 | Le soir vaut mieux que le continu | **tombe** | attendre reduirait le volume ; le continu concurrencerait le chemin servi. Les deux cedent : le volume ne depend pas de la cadence, la garde d'inactivite regle la contention ; et le lot B de la phase 2 juge deja toutes les 6 h. M-N13 le confirme : 83 % des candidats M1 et 100 % des retraits du consensus a moins de 24 h | D2 amendee : cadence parametree sous la garde d'inactivite (4.4) |
| H2 | Le tri par memoire vaut son cout face au voisinage seul | **tombe** (M-N5) | des cles libres de Qwen se recouperaient entre memoires ecrites a des mois d'ecart ; mesure : 3 paires sur 36 avec Qwen, 1 avec Sonnet. Le canal de candidats par faits n'apporte rien | replier `kind`, `change_marker`, `title` dans la sortie du juge de voisinage ; pas de table de faits |
| H3 | La dedup en simple proposition ne laisse pas grossir les doublons | **tombe** telle qu'ecrite | la passe du soir retirerait les doublons ; or `dry_run` ne rend aucune paire et le consensus hors decisions n'existe pas. En volume, elle tiendrait (environ 0,9 retrait dedup par jour ouvre, M-N12) : c'est le mecanisme qui tombe, pas l'echelle | etiquetage des 20 liens de la fenetre |
| H4 | Les liens types ameliorent le classement servi | **tombe** | un lien nouvelle vers ancienne ferait remonter la nouvelle ; le cache ignore les liens et, cote serveur, expansion, PageRank et reconsolidation jouent pour l'ancienne | M-N14 |
| H5 | Qwen suffit pour le tri | **tombe** (M-N5) | volet contexte de 16k : tient ; volet JSON : 92,3 % valide sous schema impose (critere 99 %) ; accord avec Sonnet 7 % sur `durable`, 62 % sur `change_marker` | Qwen reste rapide (1 587 appels par heure) mais ne tranche rien sans relance et validation ; le tri suit H2 |
| H6 | Le budget de 3,5 USD par soir et environ 150 USD en global tient | tient, sous condition | « nouvelles memoires » = toutes les insertions ; avec `dreamer-growth`, le budget tombe | portee `is_archived = 0`, M-N4 |
| H7 | L'API Batch est compatible avec la passe du soir | tient | un seul aller-retour par nuit ; il en faut deux, et le cout n'est connu qu'au retour | M-N10 |

Le challenge note un desaccord qui couterait un refactor : si P et H2 survivent toutes
deux a leur test, le choix de forme (tri separe ou replie, juge partout ou hybride) va
au consultant.

**Amendement v2, sans suppression des verdicts :**

- **P/H3 :** M-N12 bis et N8 ne fournissent aucune nouvelle classe sûre séparable par
  règle simple. Patch 60 reste en place ; le juge annote le résidu, sans droit de
  retrait. Une suspension pré-déduplication serait un changement distinct (13.3).
- **H2/H5 :** le choix tri séparé contre replié n'est plus ouvert pour ce contrat :
  M-N5 conduit au repli dans le voisinage, sans table de faits.
- **H6 :** M-N4 donne environ 115 USD sur les actives, mais 182 USD sur toutes les
  non-archivées en synchrone. La portée `is_archived = 0` ne suffit donc plus seule à
  tenir le plafond global ; Batch ou réduction de portée restent conditionnels.
- **H7 :** un seul appel de voisinage dans la v2, hors relances, remplace les deux
  allers-retours du premier jet. Compatibilité réelle, délais et comptabilité Batch
  restent à mesurer par M-N10 ; le verdict historique n'est pas une preuve de déploiement.

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

## 13. Décisions ouvertes et prochaines mesures

### 13.1 M-N16 : redirection du contenu sur la fixture 4.4

**Hypothèse à départager :** B, servir la remplaçante validée à la place de l'ancienne
sélectionnée, améliore `agent_ok` face à A, filtrer sans redirection. M-N8 écarte le
pointeur nu mais ne mesure pas B. La mesure se fait sur banc et via API, pas sur la base
vive ; elle ne crée aucun retrait réel.

**Deux portes séparées, proposées avant expérience :**

1. **M-N16a, utilité du texte avec oracle contrôlé.** Rejouer les 40 questions et blocs
   de M-N8, avec les mêmes modèle, effort, consigne, plafond de tokens et notation
   aveugle. Conserver V0, V1 et V2 comme références historiques. Construire A en filtrant
   les anciennes explicitement marquées dans les résultats enregistrés, B en résolvant
   ces mêmes résultats vers leur remplaçante. Ne substituer que les anciennes réellement
   présentes ; aucune injection de nouvelle dans les cas hors portée. Publier les ids
   éligibles, les nouvelles déjà présentes, les troncatures et les cas inchangés.
   Dédupliquer avant la limite et appliquer le même backfill déterministe à A et B ;
   faute de réserve de candidats enregistrée, ne pas inventer un backfill dans B seul.
2. **Critère proposé M-N16a.** Au moins **+5/40 `agent_ok` pour B contre A**, sans
   régression sur les cas non éligibles, et `old_cited(B) <= old_cited(A)` ; face à V0,
   viser aussi **au moins 5 anciennes citées en moins**. Rapporter séparément le gain
   sur les seuls cas recevant réellement un texte nouveau. Refaire trois passes
   appariées, publier chaque résultat ; ne pas retenir seulement le meilleur run.
   Porte exploratoire proposée : ces critères tiennent sur les trois passes. Le seuil
   est un choix de conception, pas un résultat ni une preuve statistique générale.
3. **M-N16b, accessibilité réelle après phase 1.** Même fixture, état post-retraits,
   vrai index filtré et projection d'alias candidate : mesurer le nombre de requêtes
   ayant encore un chemin vers la nouvelle, `new_served`, `agent_ok` et latence. Exiger
   **+5/40 `new_served` et +5/40 `agent_ok`** face à A, **0 contenu retiré servi**, **0
   cible hors propriétaire/espace**, **0 doublon de destination** ; ne pas attribuer le
   succès de M-N16a à ce mécanisme avant ce second test. Porte de latence proposée :
   p50 <= 250 ms sur le poste, budget historique référencé par Kleos #18697 ; ce seuil
   reste à confirmer pour le périmètre exact du résolveur, séparément du hook complet.
4. **Cas de sûreté hors score principal.** Cible absente, oubliée, archivée, cycle,
   chaîne à plusieurs remplacements, `partiel`, retrait Patch 60 non confirmé :
   **0 ancienne réinjectée et 0 redirection non autorisée**. Une chaîne tronquée ou
   ambiguë doit échouer fermée. Le coût de projection et son invalidation sont mesurés
   séparément ; un oracle de fixture ne valide pas la provenance en production (N7/N8).

**Réserve du challenge indépendant :** le reviewer rapporte sur
`out-hook-existing-a.jsonl` 40 cas, 8 avec l'ancienne, 5 avec la nouvelle, dont 1 avec
les deux ; seuls 7 reçoivent donc un texte réellement nouveau par substitution simple.
Il rapporte aussi +241 caractères cumulés pour remplacer les 8 anciennes. Ces mesures
communiquées ne sont pas réexécutées ici : le futur protocole doit les reproduire et
figer le plafond de tokens, pas seulement le nombre de lignes. Un succès de M-N16a
valide une utilité de rendu sous oracle, pas la résolution des chaînes en production.

#### Résultat M-N16a et décision opérateur du 2026-10-20

**MESURÉ, lecture des artefacts, sans relancer l'expérience :**
`~/.agent-forge/scratch/m_n16a_report.json` (rapport final, Kleos #30922) et
`m_n16a_audit.json`. Trois passes appariées sur 40 cas, notation Haiku à l'aveugle ;
coût total **0,4835731 USD**, durée **1 239 s** arrondie. Extraction reproductible :
`jq '{passes: [.passes[] | {pass, variants, paired: (.paired_A_B.haiku | {k, inverse_flips, noneligible_regressions, noneligible_agent_ok_divergence})}], verdict, total_cost_usd, total_duration_seconds}' C:/Users/Olivier/.agent-forge/scratch/m_n16a_report.json`.
Le verdict enregistré est `"result": "EFFET LOCAL"`.

| Passe | `agent_ok` V0 / A / B (sur 40) | Gain B contre A | `old_cited` V0 / A / B | k parmi les 7 éligibles | Flips inverses éligibles | Régressions non éligibles |
|---|---|---|---|---|---|---|
| 1 | 4 / 5 / 11 | +6 | 8 / 0 / 0 | 7/7 | 0 | N10 |
| 2 | 4 / 4 / 12 | +8 | 8 / 0 / 0 | 7/7 | 0 | Aucune |
| 3 | 4 / 5 / 12 | +7 | 8 / 0 / 0 | 7/7 | 0 | Aucune |

**MESURÉ, audit intégré au rapport final :** les chiffres du reviewer sont reproduits :
8 cas avec l'ancienne, 5 avec la nouvelle déjà présente (P07, N01, N06, N08, N10),
intersection N08 ; les **7 cas éligibles** sont **P14, P16, P17, P18, P21, P24, P27**.
Le remplacement des huit anciennes représente **+241 caractères** cumulés ; ce chiffre
n'est pas le delta net des blocs après déduplication. Aucune troncature, avec plafonds
de **500 caractères de contexte et 100 tokens de sortie** ; aucun backfill, faute de
réserve enregistrée, identiquement pour A et B. Extraction :
`jq '.audit | {old_present_ids, new_present_ids, both_present_ids, eligible_ids, replacement_delta_all_old, truncations, limits, backfill}' ~/.agent-forge/scratch/m_n16a_report.json`.
Attention à la provenance : l'audit séparé `m_n16a_audit.json` ne liste que N08 dans
`new_present_ids` ; la liste de cinq ci-dessus vient du rapport final.

**MESURÉ, verdict selon la grille fixée avant expérience (Kleos #30917) : EFFET LOCAL.**
La grille ouvrait M-N16b si la porte globale tenait sur les trois passes ; sinon,
k >= 4/7 aux trois passes imposait un arbitrage opérateur, et k <= 3/7 écartait B.
Les gains globaux et les critères `old_cited` tiennent ; seule N10, non éligible,
régresse en passe 1 et fait échouer la porte globale. Son entrée est hash-identique
entre A et B dans l'audit :
`jq '.entries[] | select(.id == "N10") | {A: .A.input_hash, B: .B.input_hash}' ~/.agent-forge/scratch/m_n16a_audit.json`
rend pour les deux `2cfc0369f33a692f79b56690bea5887b9a015ec790d7379ac68112a1baee327b`.
**DÉDUIT :** cette régression isolée sur entrée identique relève du bruit
inter-passes plutôt que de la substitution ; cela ne démontre pas une absence générale
de régression du mécanisme.

**DÉDUIT, non revérifié par une nouvelle notation :** la regex compte 4, 4 puis 5
anciennes citées dans A alors que les anciennes ont été filtrées du contexte ;
ce décalage avec le juge aveugle rend l'instrument peu fiable pour `old_cited`.
La notation officielle ci-dessus est Haiku, pas la regex.

**Décision opérateur consignée dans Kleos #30923** (MESURÉ par
`kleos-cli context "M-N16a résultat décision grille" --limit 5`, extrait :
`M-N16b debloque, avec regle amendee`) : **M-N16a est considérée comme réussie et
M-N16b est débloqué**, malgré le verdict de grille conservé ci-dessus. **Pour M-N16b,
une régression ne compte que si l'entrée diffère entre A et B, ou si elle se reproduit
sur au moins deux passes.** Cet amendement est postérieur à M-N16a ; il ne réécrit ni
le protocole initial ni son verdict. La décision valide l'utilité du rendu sous oracle,
pas la résolution des chaînes en production. M-N16b doit encore établir l'accessibilité
réelle, les gains, la latence et la sûreté ; aucune option d'index n'est choisie ici.

#### M-N16b : chemin réel sans mécanisme, 2026-10-20

**MESURÉ, lecture de `~/.agent-forge/scratch/m_n16b_report.json`, Kleos #30928 :**
`jq '{selection: (.selection | del(.rows)), passes: [.passes[] | del(.rows)], total_cost_usd, total_duration_seconds}' C:/Users/Olivier/.agent-forge/scratch/m_n16b_report.json`
rend notamment `"agent_ok": 7` à chacune des trois passes Haiku. Le chemin testé est
`kleos-cli context --limit 8 --space supersession-bench`, après retraits phase 1,
**sans mécanisme de redirection ni projection candidate**.

| Mesure | Résultat du chemin servi |
|---|---|
| Portée sur les 7 éligibles M-N16a | 0/7 remplaçantes servies ; seule N08 est servie parmi les nouvelles de la fixture |
| `agent_ok`, trois passes | 7/40, 7/40, 7/40 |
| Régressions contre B sous oracle | Les 7 éligibles à chaque passe, avec `input_differs_from_B: true` |
| Anciennes servies / citées Haiku | 0 / 0 |
| Latence de sélection | p50 359,5 ms, p95 407 ms, 40 requêtes |

**DÉDUIT du rapport et de la règle amendée #30923 :** les sept régressions comptent,
puisque leurs entrées diffèrent de B ; elles ne sont pas assimilables au bruit N10.
Ce résultat mesure le chemin sans mécanisme, pas l'échec d'un résolveur implémenté.

**MESURÉ par kleos-5, précision postérieure à #30928, rapportée sans refaire l'audit :**
les 120 cibles étiquetées « hors propriétaire ou espace » sont toutes dans l'espace
`default` #3, avec le même propriétaire. La vue par espace les inclut ; elles prennent
3 des 8 places pour chacune des 40 requêtes. Le libellé brut ne démontre donc pas une
fuite inter-propriétaires. **DÉDUIT :** distinguer cette inclusion de `default` d'un
contrat de recherche strictement limité à l'espace demandé lors de la prochaine mesure.

**DÉDUIT du périmètre chronométré :** la p50 de 359,5 ms est de bout en bout sur le
chemin CLI ; elle n'est pas comparable à la porte de 250 ms proposée pour le résolveur
seul. Ni validation ni invalidation de cette porte de latence par M-N16b.

### 13.2 M-N10 et complément M-N4 avant l'arriéré global

**M-N10 conservée :** 100 requêtes soumises vers 22 h via LiteLLM, cinq soirs, avec
`ended_at - created_at` ; seuil historique < 2 h les cinq soirs pour envisager le
quotidien. La v2 n'attend pas ce résultat pour imposer Batch au courant : elle ne l'y
impose pas. Avant tout arriéré global, établir aussi récupération des résultats,
reprise après redémarrage, facturation et réservations, refus des réponses périmées.
Si la latence dépasse 2 h mais que l'exécution et la comptabilité sont fiables, Batch
reste candidat pour l'arriéré seulement ; si le transport échoue, ne pas soumettre le
lot global.

**Complément M-N4 proposé :** 200 mémoires, contrat élargi `kind/change_marker/title`,
répartition décisions/autres et groupes volumineux publiée ; coût par groupe, p90,
JSON valide avant/après relance, coût des erreurs, nombre de groupes réellement uniques.
Lancer le global seulement si le budget réservé plus la dépense réelle reste dans D5.
La prévision de 182 USD synchrone sur toutes les non-archivées ne respecte pas D5.

### 13.3 Fourches coûteuses : paquet pour le consultant après mesure

| Décision | Option A | Option B | Recommandation et mesure manquante |
|---|---|---|---|
| Résidu déduplication | Garder Patch 60, audit et validation explicite ; faible coût, faux retraits encore possibles | Suspendre avant retrait un ensemble de paires et attendre validation ; plus de doublons servis, nouveau raccordement | A provisoirement, sans promesse de prévention. Mesurer le juge sur les 25 paires étiquetées et une nouvelle cohorte indépendante ; si le besoin est zéro retrait fautif, consultant avant B (N8/M-N12 bis) |
| Recherche des alias | Résoudre des références après la sélection ; changement local mais portée potentiellement nulle après filtrage phase 1 | Projection de recherche dédiée aux alias de retirées confirmées ; plus de maintenance et risque de contenu périmé | M-N16b débloqué par décision opérateur du 2026-10-20 (Kleos #30923, 13.1), avec règle de régression amendée ; aucune option choisie par le succès sous oracle de M-N16a. Ne figer aucune des deux avant M-N16b. Consultant si B est nécessaire, car il modifie le contrat de l'index et sa purge (N2/N7). **Suite du 2026-10-20 :** avis consultant #30930, option 3 sans mécanisme provisoirement ; M-N16c #30940 manque le seuil de 90 % sur les trois chemins. M-N16d en cours, puis retour au consultant ; aucune option figée. Détail ci-dessous |
| Modèle et enveloppe | Sonnet de référence, arriéré actif borné ; coût connu sur 20 groupes | Routage mixte Haiku ou périmètre étendu via Batch ; moindre coût supposé, qualité/transport à valider | A pour le pilote. M-N4 élargi et M-N10 avant extension ; consultant si réduire le coût exige une nouvelle politique de qualité |

#### Recherche des alias : avis consultant, M-N16c et suite ouverte

**Avis consultant consigné, non décision définitive** (`memory_get(30930)`) :
**option 3, aucun mécanisme pour l'instant, confiance 75 %** ; recommandation
« jamais de projection d'alias ». L'option 2 consiste à garder les retirées comme
candidates, puis résoudre au service, extension envisagée du Patch 62 dans
`kleos-lib/src/memory/search.rs`, uniquement si une mesure réelle la justifie.
**DÉDUIT par le consultant :** les remplaçantes lexicalement disjointes de la fixture
4.4 ne représentent pas nécessairement les paires voisines produites par déduplication
et correction ; le 0/7 ne suffit pas à justifier un nouvel index. Les seuils de rappel
proposés sans mécanisme sont **90 % pour la déduplication, 80 % pour les corrections**.
Ce sont ses critères d'arbitrage, pas des performances garanties.

**MESURÉ, lecture du rapport `m_n16c_report.json`, Kleos #30940 :**
`jq '{pair_inventory, paths, tables, path_divergence}' C:/Users/Olivier/.agent-forge/scratch/m_n16c_report.json`
rend `"n": 83`, `"dedup": 83` et `"returned_top10_divergent_pairs": 83`.
Le banc contient 83 paires réelles hors fixture, même propriétaire et espace, gardienne
active et retirée supersédée ; la requête reprend le contenu de la retirée.

| Chemin | Gardienne dans le top 10 | Retirée servie | Seuil dédup 90 % |
|---|---|---|---|
| CLI `search` | 66/83, soit 79,5 % | 0 | Manqué |
| CLI `context` | 55/83, soit 66,3 % | 0 | Manqué |
| MCP `memory_search` | 71/83, soit 85,5 % | 0 | Manqué |

**MESURÉ :** aucune paire de correction dans ce banc, donc seuil de 80 % non évalué.
Les top 10 diffèrent entre chemins sur **83/83 paires** ; la présence de la gardienne
diffère sur 19/83. Les 57 échecs sont **17 + 28 + 12 observations chemin-paire**, pas
57 paires distinctes garanties. **DÉDUIT :** le seuil manqué motive un retour à
l'arbitrage, pas le choix automatique de l'option 2 ni une projection d'alias.

**Statut communiqué par le team-lead le 2026-10-20 : M-N16d en cours**, pour qualifier
la nature des 57 échecs, mesurer le canal kleos-cache et essayer une requête courte.
Retour au consultant avec ces mesures ; **aucune option n'est figée**. Les propositions
historiques de la ligne du tableau sont conservées comme historique, non comme un go.

#### M-N16d F1, avis consultant et décision opérateur du 2026-10-21

**MESURÉ, lecture de `~/.agent-forge/scratch/m_n16d_report.json`, Kleos #31597 :**
le banc comporte 83 paires pour `search`, MCP et `context`, et 81 pour kleos-cache.
La gardienne est présente comme suit, par requête intégrale puis courte :

| Chemin | Intégrale top 3 / top 10 | Courte top 3 / top 10 |
|---|---|---|
| `search` | 61/83 / 72/83 | 51/83 / 69/83 |
| MCP | 61/83 / 72/83 | 51/83 / 69/83 |
| `context` | 0/83 / 62/83 | 0/83 / 51/83 |
| kleos-cache | 81/81 / 81/81 | 60/81 / 69/81 |

**Correction de lecture, Kleos #31625 et #31626 :** le 0/83 du top 3 `context` de M-N16d est un artefact de `--limit 10`, où les tiers always-on occupaient les cinq premiers rangs avant les résultats sémantiques. Le Patch 71 émet le sémantique retenu en tête sans modifier les quotas ni la composition des tiers. L'amélioration attendue du top 3 `context` est une hypothèse tant que le banc n'a pas été rejoué.

**MESURÉ :** 0 retirée servie. Parmi les échecs top 10, le classement est a0, c3,
b4, d4 pour `search` et MCP, et a0, c11, b0, d10 pour `context`. Le banc a dérivé de
24 lignes entre M-N16c et ce matin, avec 57 puis 43 échecs ; la cause n'a pas été lue.

**DÉDUIT par le consultant, Kleos #31604 :** option 1, aucun mécanisme, confiance
70 %. Son argument principal est que l'option 2 impose d'inverser Patch 64 dans
`kleos-cache/src/sync_worker.rs:468`, `is_projectable`. Sur kleos-cache en requête
courte, 69/81, soit 85 %, reste sous le seuil de déduplication de 90 % ; le verdict
repose sur le coût de l'option 2, non sur l'atteinte de ce seuil.

**Décision opérateur du 2026-10-21, Kleos #31605 :** option 1, aucun mécanisme.
La décision bascule seulement si l'une des conditions suivantes est satisfaite :

1. Banc rejoué avec `is_superseded` désactivé : au moins 8/81 paires où la retirée est
   dans le top 10 sans la gardienne, sur kleos-cache en requête courte.
2. Au moins 20 paires de corrections réelles sous 80 % de top 10 : le mécanisme passe
   alors par `parent_memory_id`.
3. `/context` devient le canal dominant.

Le tier important aux rangs 1 à 5 de `/context` est un sujet séparé.

**Autres inconnues explicites :** rappel réel des candidats (M-N11), précision des
confirmations en direct (non couverte par M-N15), fréquence des invalidations et
affamement sous garde d'inactivité (M-N13 ne mesure que les écarts temporels), qualité
et volume des futurs transcripts (M-N2/M-N6), nettoyage des annotations (N7), gain
serveur des liens après N1/N9 (M-N14). Aucune n'est transformée en garantie par la v2.

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
