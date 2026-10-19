# Spec d'implementation : phase 2, conflits de decisions (detection, indicateur, juge)

Statut : **projet, a valider par l'operateur**. Aucun code avant son go. Le lot A
peut partir sur go ; le lot B (retrait) attend que la precision des retraits soit
mesuree (plan 6.1 ter, correction du 2026-10-19 : le controle operateur n'a porte sur
aucun retrait).
Source des decisions : plan `2026-10-17-supersession-muninn-plan.md`, sections 6.0 a
6.1 ter et journal (section 14). Mesures : `~/.agent-forge/scratch/phase2_*`, Kleos
#30245, #30251, #30256, #30257, #30268, decision #30276.

Principe : la spec reproduit **la configuration mesuree**, pas une variante voisine.
Tout ecart (seuils, listes de mots, prompts, modele, effort, dates dans le prompt)
invalide la porte et se remesure avant d'etre livre.

---

## 1. Perimetre

Trois lots, livres et deployes dans cet ordre. Chacun est utile seul et se coupe par
un interrupteur.

| Lot | Contenu | Depots | Interrupteur |
|---|---|---|---|
| A | table des conflits, detecteur, champ `conflicts` et ligne d'indicateur sur les surfaces serveur | Kleos | `KLEOS_DECISION_CONFLICTS` (defaut `off`) |
| B | job du dreamer : juge par chaine, juge par paire, validateur, consensus, application | Kleos | `KLEOS_CONFLICT_JUDGE_URL` absent = pas de juge |
| C | indicateur sur le chemin dominant du hook | kleos-cache, claude-config (`hooks-rs`), kleos-sidecar | champ absent = rendu inchange |

Pourquoi le lot C existe : le hook par prompt passe par kleos-cache `POST /v1/retrieve`
dans 206 cas sur 219 (journal, 2026-10-18), rendu par
`hooks-rs/src/hooks/user_prompt_lean.rs:253-272` (`#id [cat] [local] texte`) ; le
repli sidecar (`kleos-sidecar/src/routes.rs:788-824`) rend `[cat] texte`. Aucun des deux
ne verrait un champ ajoute cote serveur. Sans le lot C, l'indicateur n'atteint l'agent
que par `/context`, `/prompt/generate` et `kleos-cli context`.

Hors perimetre : l'API Batch (moitie prix, a mesurer plus tard sur LiteLLM), un
rendu dans l'interface GUI, la detection sur d'autres categories que `decision`.

Niveau d'ecart upstream vise : **additif** (nouveaux modules, overlay de schema), plus
quelques **patchs chirurgicaux** d'accroche, un patch numerote par lot (Patch 67, 68,
69 dans `docs/dev-notes/local-patches.md`).

---

## 2. Lot A : conflits ouverts et indicateur

### 2.1 Schema (overlay VOCSAP, jamais une migration numerotee)

Nouveau fichier `kleos-lib/src/db/vocsap/decision_conflicts.sql`, overlay createur
inscrit dans `VOCSAP_MONOLITH_OVERLAYS` **et** `VOCSAP_TENANT_OVERLAYS`
(`kleos-lib/src/db/vocsap/mod.rs`), nom `decision_conflicts`, ajoute a
`CREATOR_OVERLAYS` dans `mod tests`.

```sql
CREATE TABLE IF NOT EXISTS decision_conflicts (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL,
  space_id INTEGER,                 -- NULL est un groupe a part entiere
  old_id INTEGER NOT NULL,          -- la plus ancienne
  new_id INTEGER NOT NULL,          -- la plus recente, celle qui a ouvert le conflit
  score REAL NOT NULL,              -- recouvrement mesure
  marker INTEGER NOT NULL,          -- 1 si la nouvelle porte un marqueur de changement
  status TEXT NOT NULL DEFAULT 'open',
     -- open | distinct | partial | disputed | superseded | stale
  partial_point TEXT,               -- point perime, verdict partiel seulement
  component_sig TEXT,               -- signature de la composante au dernier jugement
  judged_at TEXT,
  attempts INTEGER NOT NULL DEFAULT 0,
  last_error TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE (old_id, new_id)
);
CREATE INDEX IF NOT EXISTS idx_decision_conflicts_old ON decision_conflicts(old_id, status);
CREATE INDEX IF NOT EXISTS idx_decision_conflicts_new ON decision_conflicts(new_id, status);
```

Statuts servis avec un indicateur : `open`, `disputed`, `partial`. Les autres ferment
l'indicateur. `stale` : une des deux memoires est sortie de la lecture (oubliee,
archivee, retiree par un autre chemin) ; pose au moment du rendu ou du passage du
dreamer, jamais reactive.

### 2.2 Detecteur (deterministe, reproduction exacte de M1)

Nouveau module `kleos-lib/src/intelligence/decision_conflicts.rs`.

- **Normalisation** : minuscules, NFKD, suppression des marques combinantes
  (`unicode-normalization`, deja dependance de kleos-lib), decoupage sur `[^0-9a-z]+`,
  mots de 2 caracteres ou plus, hors liste de mots vides.
- **Listes** : les listes mesurees sont embarquees telles quelles dans le module
  (`STOP`, `MARKERS_FR`, `MARKERS_EN` de `phase2_overlap_sim.py:16-27`). Une classe
  lexicon `decision_conflict_markers` (canal `lexicon-overrides`, Patch 38) peut les
  etendre ; classe vide ou absente = listes embarquees.
- **Marqueur** : un marqueur est present si ` <marqueur> ` apparait dans le texte
  normalise, entoure d'espaces, mots separes par une espace.
- **Score** : `|A inter B| / min(|A|, |B|)`, 0 si moins de 2 mots communs.
- **Seuil** : 0,34 si la nouvelle memoire porte un marqueur, 0,5 sinon.
- **Regle** : a l'arrivee d'une decision servable `new`, parcourir les decisions
  servables du meme `user_id` et du meme `space_id` (NULL compris), **strictement plus
  anciennes** que `new` par `(created_at, id)` et d'id different, de la plus recente a
  la plus ancienne ; la premiere qui passe le seuil ouvre **un** conflit `(old, new)` ;
  arret. Rien n'est retire.
- **Servable** : `is_latest = 1`, `is_superseded = 0`, `is_forgotten = 0`,
  `is_archived = 0`, `status <> 'pending'`, `category = 'decision'`.

Accroches :

1. `kleos-server/src/routes/memory/mod.rs`, dans le bloc `if !result.pending`
   (l.230-242), une ligne `spawn_detect_conflicts(...)` a cote de
   `spawn_post_store_derivation`. Patch chirurgical.
2. Meme appel dans `routes/inbox/mod.rs` (approbation d'une memoire `pending`), seul
   autre appelant de `spawn_post_store_derivation`.
3. Rattrapage dans le dreamer (avec ou sans juge) : balayage **complet** de toutes les
   decisions servables du user a chaque passage, dans l'ordre `(created_at, id)`, qui
   rejoue la regle. Il couvre les chemins qui ne passent pas par la route (`batch`,
   `conversations`, `ingestion`, `onboard`, `sync`, `space`), l'arriere, et les
   decisions qui redeviennent servables (desarchivage, fin d'oubli). Pas de
   marque-page par id : il raterait ces dernieres. Cout : calcul local seul, environ
   1 400 decisions sur la copie.
4. Idempotence : `INSERT OR IGNORE` sur `UNIQUE (old_id, new_id)` ; un second passage
   sur le meme etat n'ecrit rien.

Ecart assume avec la mesure : la simulation voyait l'etat fige de la copie. En regime,
une memoire retiree sort du parcours ; si c'etait la premiere ancienne a passer le
seuil, le parcours continue et peut ouvrir une paire **que la mesure n'a jamais vue**
(pas seulement moins de paires). La parite 194 / 194 ne vaut que sur l'etat fige ; en
regime, ces paires passent par le meme juge et le meme consensus.

### 2.3 Indicateur

Rendu commun, une fonction pure dans le module :

- conflit ouvert ou dispute, sur chacune des deux memoires :
  `conflit avec #<autre> (<date AAAA-MM-JJ>), arbitrage en attente`
- partiel, sur l'ancienne seulement :
  `point perime selon #<nouvelle> (<date>) : <partial_point>`

Le texte de l'autre memoire n'est **pas** cite (meme regle que la phase Evolution,
Patch 63 : ne jamais mettre l'ancien texte a cote du courant ; l'agent a l'id pour le
lire). `partial_point` vient du LLM : il passe par `escape_tag_delimiters` et reste a
l'interieur de l'enveloppe `<user_memory>` de la memoire concernee, borne a 200
caracteres. Cout mesure en M3 avec l'extrait : environ 34 tokens par ligne ; sans
extrait, moins.

Surfaces, chacune chargee par une seule requete groupee sur les ids de la page (pas de
N+1) :

| Surface | Site | Forme |
|---|---|---|
| JSON d'une memoire (`/list`, `/memory/{id}`) | `memory_to_json` (`routes/memory/mod.rs:85`) | champ `conflicts: [{with, date, status, point?}]`, **absent** quand vide |
| `/search` | `search_memories` (l.465-522), motif `item["x"] = ...` | idem |
| `/recall` | quatre niveaux (l.785, 798, 811, 839) | idem |
| `/context` | apres coup sur les blocs, avant `assemble_context_string` (`context/mod.rs:1415`), sans nouveau champ dans `ContextBlock` | ligne d'indicateur sous la memoire |
| `/prompt/generate` | `memory_line` (`routes/prompts/mod.rs:123`) | ligne d'indicateur sous `- [mem id]` |

Interrupteur `off` : aucune requete supplementaire, JSON et texte identiques a
aujourd'hui (garantie testee).

### 2.4 Tests du lot A

- Overlay : apply puis no-op (x2), no-op sur schema complet, cas fresh, monolith et
  tenant ; `overlays_are_noop_when_target_tables_absent` vert avec l'exemption.
- Detecteur, unitaire : tokenizer (accents, ponctuation, mots de 1 caractere), marqueur
  fr et en, seuil 0,34 / 0,5, moins de 2 mots communs, NULL space, une seule ouverture
  par nouvelle memoire.
- **Parite avec la mesure** : sur la fixture 4.4, 112 candidats, 18 / 40 remplacements,
  7 / 30 controles signales ; sur l'export `phase2-decisions.json` (copie figee), les
  **194 memes paires** que `real-candidates.jsonl`. Le second est un test `#[ignore]`
  qui lit le chemin dans une variable d'environnement (les donnees reelles ne vont pas
  dans le depot), lance a la main avant la porte.
- Surfaces : une paire de decisions en conflit est servie avec son indicateur sur
  chaque surface du tableau ; interrupteur off = sortie identique octet pour octet ;
  une memoire `stale` ne porte plus d'indicateur.
- Garde de non-regression : `retrieval_eval` 9 / 9 et la ligne de base kleos-cache de
  la phase 1, inchanges.

---

## 3. Lot B : juge dans le dreamer

### 3.1 Place dans le dreamer

Hors DAG de `default_pipeline` : une fonction `run_conflict_stage(db, user_id, cfg)`
appelee depuis `run_cycle` (`dreamer.rs:259`) et `run_cycle_tenants` (`dreamer.rs:754`),
sur le modele de `run_skill_evolution`, avec son propre intervalle
(`KLEOS_CONFLICT_JUDGE_INTERVAL_SECS`, defaut 21 600). Raison : `IntelligenceTask::run`
ne recoit ni config ni client, et changer la signature de `default_pipeline` touche
trois appelants et les tests du scheduler. Le rapport de l'etape ne contient jamais
la cle, l'URL complete ni les textes des memoires (`last_pipeline_report` est expose).

Ordre dans un passage : rattrapage du detecteur (2.2), puis jugement.

### 3.2 Client du juge

Nouveau module `kleos-lib/src/intelligence/conflict_judge.rs`, calque sur
`services/broca.rs::call_llm_endpoint` sans le reutiliser (corps propre, pas
d'injection `KLEOS_LLM_THINK`) :

- variables : `KLEOS_CONFLICT_JUDGE_URL`, `KLEOS_CONFLICT_JUDGE_API_KEY`,
  `KLEOS_CONFLICT_JUDGE_MODEL` (defaut `claude-sonnet-5-5`),
  `KLEOS_CONFLICT_JUDGE_CHAIN_EFFORT` (defaut `medium`),
  `KLEOS_CONFLICT_JUDGE_PAIR_EFFORT` (defaut : champ absent, soit l'effort par defaut
  du modele, `high` pour Sonnet 5.5, **comme mesure**) ;
- `reqwest::Client` via `net::safe_client_builder()`, URL passee par
  `net::validate_outbound_url` (un LiteLLM en RFC1918 exige `KLEOS_NET_ALLOW_PRIVATE=1`,
  a documenter dans `CLAUDE.local.md`) ;
- corps : `model`, `temperature: 0`, `max_tokens` 16 000 (chaine) et 4 000 (paire),
  `messages` system + user, `output_config: {effort}` seulement si un effort est fixe ;
- timeout 300 s (chaine) et 120 s (paire), 3 nouvelles tentatives espacees ;
- reponse : `choices[0].message.content` ; vide, `finish_reason = length` ou erreur =
  echec, jamais un verdict.

Les prompts sont de nouveaux ids du canal prompts (Patch 15) :
`decision_conflicts/chain/system` et `decision_conflicts/pair/system`, versions
embarquees dans `kleos-lib/prompts/` = **texte exact mesure** (`CHAIN_SYSTEM` de
`phase2_chain_judge.py:19-35`, `SYSTEM` de `phase2_judge.py:21-35`). Une surcharge par
`prompts-overrides` change le comportement mesure : elle se remesure.

### 3.3 Composantes et entrees

- Composante : fermeture connexe des conflits du user dont le statut est `open`,
  `disputed` ou `partial`, restreinte aux memoires encore servables. Les conflits
  `stale` en sortent.
- Signature : hash des `(id, sha256(content))` des membres tries et des ids de conflits
  de la composante. **Jamais `updated_at`** : Kleos le reecrit a la lecture
  (`context/deps.rs:381`, `memory/mod.rs:1005-1007`) et au balayage de
  reconsolidation (`reconsolidation.rs:212`), la signature deriverait a chaque cycle.
- Un jugement par signature : une composante deja jugee n'est **pas** renvoyee tant
  que ses membres, leur contenu ou ses conflits ne changent pas, y compris quand des
  conflits y restent `disputed` ou `partial`. Raison : chaque nouveau jugement est un
  nouveau tirage (entre Sonnet `medium` et `high`, la chaine change de verdict sur 7 des
  41 paires que la paire juge `supersedes`), et un retrait a pu sortir d'un tirage
  favorable. La mesure porte sur un tirage par paire ; la production aussi.
- Prompt chaine (mesure) : membres tries par `(created_at, id)`, chacun
  `[<id>] <created_at>\n<texte coupe a 6 000 caracteres + " [...]">`, separes par une
  ligne vide.
- Prompt paire (mesure) : `A :\n<ancienne>\n\nB :\n<nouvelle>`, **sans dates**, memes
  coupes.
- Plafond par passage : `KLEOS_CONFLICT_JUDGE_MAX_COMPONENTS` (defaut 20) ; le reste
  attend le passage suivant, les plus anciennes composantes d'abord.

### 3.4 Validateur deterministe (reproduction de `validate_edges` et `validate`)

Chaine :

1. Sortie (comme `phase2_judge.py:73-81`) : clotures markdown retirees, texte pris du
   premier `{` au dernier `}`, parse JSON ; echec du parse (dont plusieurs objets),
   cles manquantes = aucune decision.
2. Un lien `remplace [n, o]` n'est garde que si `n` et `o` sont membres et que
   `(created_at, id)` de `n` est strictement superieur a celui de `o`.
3. Si tous les membres seraient remplaces, le plus recent est retire de l'ensemble et
   ses liens tombent : au moins une memoire reste active.
4. Une memoire deja retiree n'est jamais reactivee.
5. `partial [n, o, point]` : garde aux memes conditions d'ordre ; `point` non vide.

Paire : verdict dans `supersedes | distinct | conflict` ; `supersedes` exige
`quote_a` et `quote_b` non vides, sous-chaines exactes de A et de B, sinon il devient
`conflict`.

### 3.5 Consensus et application

Pour chaque conflit `(old, new)` de la composante :

| Chaine | Paire Sonnet | Statut | Effet |
|---|---|---|---|
| `remplace (new, old)` | `supersedes` valide | `superseded` | retrait de `old` |
| `remplace (new, old)` | autre | `disputed` | indicateur servi, pas de retrait |
| `partial (new, old)` | non appele | `partial` | indicateur partiel sur `old` |
| aucun lien | non appele | `distinct` | indicateur ferme |
| echec du juge | non appele | inchange, `attempts + 1`, `last_error` | indicateur servi |

Le juge par paire n'est appele que sur les conflits que la chaine propose de retirer
(c'est la configuration mesuree : consensus sur les paires candidates). Un lien
`remplace` de la chaine entre deux membres qui ne forment pas un conflit detecte n'est
**pas** applique et n'ouvre rien (il n'a pas ete mesure).

Retrait, dans une transaction courte ouverte **apres** les appels LLM (aucun appel
reseau sous ecriture) :

1. relire `old` et `new` : tous deux servables, meme user, meme espace ; sinon `stale` ;
2. `memory::insert_link(new, old, 1.0, "supersedes", user)` (source = gardienne,
   cible = retiree, comme `correct_memory`) ;
3. `UPDATE memories SET is_superseded = 1, updated_at = datetime('now') WHERE id = old`
   (comme `correction.rs:66` et `duplicates.rs:95`) ;
4. statut `superseded`, `component_sig`, `judged_at` ;
5. `invalidate_search_cache(user)` apres le commit.

Le trigger du Patch 61 (rendre la retiree si la gardienne est archivee) s'applique sans
changement.

Echecs : au-dela de 5 tentatives, la composante n'est plus renvoyee avant un changement
de signature ; le compteur apparait dans le rapport de l'etape. Une lecture ratee n'est
jamais comptee comme `distinct`.

### 3.6 Tests du lot B

- Validateur : lien ancien vers recent rejete, membre inconnu rejete, tous remplaces,
  plusieurs objets JSON, citation absente du texte, memoire deja retiree.
- Consensus : les cinq lignes du tableau 3.5, avec un faux client injecte (trait),
  sans reseau.
- Idempotence : deux passages sur le meme etat = zero appel au second, zero ecriture.
- Application : relecture qui trouve `old` archivee entre l'appel et l'ecriture = `stale`,
  aucun lien ; lien et drapeau poses ensemble ou pas du tout.
- Banc (a la main, avant la porte de deploiement) : passage complet sur la copie figee
  avec le vrai juge ; comparaison aux 15 retraits du consensus mesure (Sonnet
  `medium`), 0 retrait sur les 30 controles, et chaque retrait propose confronte aux
  etiquettes operateur des retraits (section 5, point 2).
  Ecart de quelques unites attendu (non-determinisme), a reporter dans le journal.

---

## 4. Lot C : chemin du hook

**Lot conditionnel** : il ne demarre qu'apres une mesure, sur le serveur vif avec les
lots A et B, du nombre de memoires servies qui portent un indicateur (M3 sur 99
requetes : 1 sur 792 avant juge). Si le volume reste de cet ordre, le lot C ne vaut
pas trois depots touches.

- **kleos-cache** (`workspace/kleos-cache`) : `sync_worker.rs` relit tout `/list` a
  chaque passe mais ne reecrit un document que si son `content_hash` change
  (l.237-270 ; le hash couvre id, contenu, categorie, tags, importance, `is_static`,
  `created_at`, espace, l.484-494 ; `updated_at` n'est pas lu). Il faut donc un
  `conflicts_hash` **separe**, qui met a jour les metadonnees sans recalculer le
  vecteur. Mettre `conflicts` dans `content_hash` reencoderait un texte inchange.
  `POST /v1/retrieve` renvoie `conflicts` dans chaque resultat, absent quand vide.
  Sans ce mecanisme, un conflit ouvert apres la premiere sync n'atteint jamais le
  cache, et un conflit ferme y reste affiche.
- **claude-hooks** (`claude-config/hooks-rs/src/hooks/user_prompt_lean.rs:253-272`) :
  sous chaque ligne `#id [cat] [local] texte`, une ligne d'indicateur par conflit, meme
  texte qu'en 2.3, aplati par `flat`.
- **kleos-sidecar** (`routes.rs:788-824`) : idem sous `[cat] texte`, compte dans le
  budget de caracteres.

Tests : un resultat avec et sans `conflicts`, rendu identique sans le champ (garde de
non-regression du hook), budget respecte.

---

## 5. Deploiement et porte

1. Lot A sur le banc (`:4290`, copie figee), parite 194 / 194, surfaces vertes ; puis
   serveur vif avec `KLEOS_DECISION_CONFLICTS=on`, sauvegarde a froid avant.
2. **Prealable au lot B** : etiquetage par l'operateur d'un echantillon **parmi les
   retraits proposes** par le consensus (15 sur la copie), en commencant par les paires
   6, 20 et 23 de `phase2-labels-operator.md`, deja dans sa feuille. Une seule reponse
   `distinct` fait tomber le consensus tel quel.
3. Lot B sur le banc avec le vrai juge (cout attendu environ 2 USD pour l'arriere) ;
   porte : 0 controle retire, precision des retraits acceptee par l'operateur sur
   l'echantillon du point 2, idempotence prouvee (second passage : 0 appel) ; puis
   serveur vif.
4. Lot C, conditionnel (section 4).

Secret : la cle LiteLLM du serveur vit dans `/etc/kleos/kleos.env` (en clair sur le
LXC, comme les autres cles du serveur) ; jamais dans un depot, jamais dans un rapport.

---

## 6. Points ouverts a trancher avant le lot concerne

1. **Precision des retraits** : non mesuree (section 5, point 2). C'est le point qui
   decide du lot B.
2. **Pair judge a l'effort par defaut** : c'est ce qui a ete mesure ; passer la paire en
   `medium` reduirait le cout du rattrapage mais demande une remesure du consensus.
3. **Langues** : listes fr + en seulement, comme mesure ; une memoire allemande n'est
   detectee que par les mots communs, sans marqueur.
