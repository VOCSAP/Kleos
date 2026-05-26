# Intelligence Pipeline -- Overview (2026-05-26)

Document synthese du fonctionnement de Kleos pour la detection des
contradictions et patterns temporels, redige a partir d'une investigation
post-Patch 37. Permet de comprendre pourquoi les endpoints
`/intelligence/contradictions` et `/intelligence/temporal/detect`
retournent souvent vide sur un corpus reel, et comment orienter la prise
de note si on veut effectivement profiter de ces capacites.

---

## 1. Extraction des structured_facts

Les contradictions detectees par Kleos reposent **entierement** sur la
table `structured_facts` (subject, predicate, object, confidence,
memory_id). Sans facts stockes, aucune contradiction n'est detectable
mecaniquement.

### 1.1. Deux paths d'extraction

| Path | Fichier | Methode | Quand declenche |
|---|---|---|---|
| **fast_extract_facts** | `kleos-lib/src/intelligence/extraction.rs:123` | **Regex pure** (no LLM) | Apres chaque INSERT memory (background tokio spawn), via `/ingest` job, via admin reprocess |
| **extract_facts** | `kleos-lib/src/ingestion/processors/extract.rs:161` | **LLM** (`LocalModelClient` -> Ollama) | Uniquement `/ingest` avec mode `extract` |

### 1.2. fast_extract_facts -- regex pure, anglais consumer/lifestyle

Patterns hardcodes dans `extraction.rs` :

| Regex | Cible | Exemple |
|---|---|---|
| `buy_regex` | `bought 5 widgets` | "I bought 3 books" -> subject=user, predicate=bought, object=books, quantity=3 |
| `spent_regex` | `spent $X on Y` | "I spent $50 on dinner" |
| `have_regex` | `have N X` | "I have 3 cats" |
| `exercise_regex` | "did 30 push-ups", "ran 5 km" | |
| `made_regex` | "made N X" | "I made 200 dollars" |
| `earned_regex` | "earned $X" | |
| `like_regex` / `dislike_regex` / `favorite_regex` | Preferences | "I like jazz" |
| `location_regex` | "live in X", "moved to X" | State-type |
| `role_regex` | "work as X" | State-type |

**Limite critique** : couverture **anglais uniquement**, focus
**consumer/lifestyle** (achats, depenses, exercice, preferences, lieu,
role). **Zero couverture** du tech francais (commandes, IPs, deploys,
patches, comportement logiciel, etat de services, etc.).

Consequence empirique observee sur LXC 121 prod (2026-05-26) :
**~2 structured_facts pour ~2282 memories** (taux <0.1%). Les 2 facts
sont des artefacts marginaux de matchs partiels.

### 1.3. extract_facts -- LLM (path /ingest extract mode)

`kleos-lib/src/ingestion/processors/extract.rs:161` appelle un LLM avec
un system prompt qui demande de retourner un JSON array de "facts" en
texte libre. **MAIS** : ces "facts" sont stockes comme **nouvelles
memories separees** (pas comme rows `structured_facts`). Le pipeline
enqueue ensuite `ingestion.fact_extract` qui execute
`fast_extract_facts` sur chaque memory ainsi cree, donc on retombe sur
les memes regex anglais.

Cette voie LLM **n'aide pas** pour augmenter le taux de structured_facts
sur du tech francais : le LLM produit des phrases qui sont ensuite
re-soumises au regex anglais, qui ne matche pas.

### 1.4. Verdict structured_facts

- **A la prise de note** : rien de special a faire cote
  operateur. Le pipeline tourne automatiquement en background. Mais
  sur du contenu tech francais, **0 fact extrait dans 99% des cas**.
- **A long terme** : pour rendre la detection contradictions reellement
  utile sur ce corpus, il faudrait soit (a) etendre les regex (lourd,
  fragile, multilangue), soit (b) ajouter un path LLM qui produit
  directement des rows structured_facts (subject+predicate+object), pas
  juste du texte libre. C'est un chantier non commit a date.

---

## 2. Detection des contradictions

### 2.1. Deux fonctions, deux semantiques

| Fonction | Fichier | Quand appellee | Patch 37 filter |
|---|---|---|---|
| `detect_contradictions(memory)` | `intelligence/contradiction.rs:22` | Apres store memory (single memory, check contre les facts existants) | **PAS FILTRE PAR SPACE** (deferred) |
| `scan_all_contradictions(user_id)` | `intelligence/contradiction.rs:149` | Endpoint `POST /intelligence/contradictions` | **FILTRE Patch 37** (JOIN m1+m2 + space equality) |
| `detect_fact_contradictions(...)` | `intelligence/temporal.rs:560` | Sous-routine de `post_process_new_facts` | **FILTRE Patch 37** (JOIN m_cand+m_new) |

### 2.2. Algorithme commun

Pour les 3 fonctions :

1. Loader les `structured_facts` candidats (filtre subject + predicate
   identique, memory_id different).
2. Comparer les `object` via `objects_match` (string match flexible).
3. Si objects differents -> contradiction (memory_a vs memory_b).
4. Confidence = `min(conf_a, conf_b) * 0.8`.

**Aucun LLM, aucune analyse semantique, aucune analyse temporelle de
type "X etait vrai puis devenu faux".** C'est strictement structure
relationnel (subject + predicate = even key, objects different = same
question with conflicting answers).

### 2.3. Pourquoi le "agent-forge ko le 10/04 / ok le 25/05" n'est PAS detecte

Si l'operateur stocke :
- mem A 10/04 : "agent-forge ne propose pas d'aide via --help"
- mem B 25/05 : "agent-forge expose une aide complete via --help"

Le pipeline regex ne genere aucun structured_fact pour ces 2 memories
(aucune regex consumer ne matche). Resultat : 0 row structured_fact,
0 candidate pour `detect_contradictions`, 0 contradiction detectee.

**Meme avec extraction LLM via /ingest**, le path ne produirait pas
les bons structured_facts -- il decomposerait peut-etre en "agent-forge
expose --help" et "agent-forge ne propose pas --help" comme phrases
separees, mais sans structurer `subject=agent-forge predicate=expose
object=help`. Donc 0 row structured_fact pour ce cas.

### 2.4. Ce qu'il faudrait pour detecter ces contradictions semantiques

Option 1 -- enrichir l'extraction LLM pour structurer subject + predicate
+ object + negation + date :
```json
[
  {"subject": "agent-forge", "predicate": "expose_help", "object": "false", "valid_at": "2026-04-10"},
  {"subject": "agent-forge", "predicate": "expose_help", "object": "true",  "valid_at": "2026-05-25"}
]
```
Puis le pipeline contradiction matche subject+predicate, voit
object=false vs true, leve une contradiction. La logique temporal
existante (`detect_fact_contradictions` avec `is_state_verb`) saurait
classer ca comme transition d'etat, pas comme contradiction si
`valid_at` est ordonne.

Option 2 -- ajouter un nouveau path LLM "semantic contradiction sweep"
qui prend 2 memories sur le meme topic (par embedding similarity) et
demande au LLM "ces 2 memoires se contredisent-elles ? si oui,
laquelle est la plus recente ?". Plus generique mais lourd en CPU
LLM.

Aucune des deux options n'est implementee. C'est un chantier ouvert.

---

## 3. Detection des temporal patterns

### 3.1. Algorithme

`detect_patterns()` dans `intelligence/temporal.rs:65` :

1. SELECT memories WHERE is_forgotten=0, LIMIT 5_000 (`DETECT_SCAN_LIMIT`).
2. Group by `(space_id, category)` (Patch 37, avant : par category seul).
3. Pour chaque bucket :
   - Si `entries.len() < MIN_SAMPLE_SIZE` (= 5) -> skip.
   - Calculer les inter-arrival times en secondes.
   - Si `stddev / mean >= STDDEV_RATIO_THRESHOLD` -> skip (trop noisy).
   - Si mean matche `daily` (24h +/- 2h), `weekly` (7j +/- ?), ou
     `monthly` (30j +/- ?) -> creer un TemporalPattern.

### 3.2. Pourquoi ca retourne souvent vide

Sur LXC 121 (2026-05-26) :
- Categories denses (`discovery`, `decision`, `state`) en theorie
  > MIN_SAMPLE_SIZE.
- MAIS les sessions Claude sont irregulieres dans le temps (pics
  pendant les sessions de travail, vide la nuit/weekend).
- Le ratio `stddev/mean` est tres eleve sur des inter-arrival mixant
  des intervalles de 10 secondes et de 12 heures.
- Donc bucket noisy -> skip -> 0 pattern.

Pour avoir des patterns naturels, il faudrait des memories cree avec
**cadence regulee** : par exemple un cron qui stocke un "daily standup
note" chaque jour a 09:00. Ce n'est pas le profil naturel d'une session
de dev.

### 3.3. Verdict temporal_patterns

- Mecanique correcte mais **profil d'usage Claude Code = noisy
  inter-arrival** -> peu de patterns detectes en pratique.
- Patch 37 a corrige le bug cross-space mais n'augmente pas le taux de
  detection (au contraire : split en sous-buckets, chaque bucket plus
  petit).
- Pour exploiter cette feature : faudrait un cas d'usage avec emission
  reguliere (suivi habitude, monitoring journalier, etc.).

---

## 4. Ce qui declenche le pipeline

| Trigger | Path code | Quoi est lance |
|---|---|---|
| `POST /memories` (kleos-cli store) | `routes/memory/mod.rs:8` import + tokio::spawn ligne 182 | `fast_extract_facts` sur la nouvelle memory, en background |
| `POST /ingest` raw mode | `ingestion/processors/raw.rs` puis enqueue `ingestion.fact_extract` job | `fast_extract_facts` via job worker (main.rs:570 dispatcher) |
| `POST /ingest` extract mode | `ingestion/processors/extract.rs` | LLM extract -> N memories -> chacune enqueue `fact_extract` -> regex |
| Admin reprocess | `routes/admin/mod.rs:479` | `fast_extract_facts` rejoue sur un set de memories |
| Background dreamer | `kleos-server/src/dreamer.rs` | `scan_all_contradictions`, `detect_patterns`, `consolidation`, growth reflect, etc. par tick (5min) si serveur idle >60s |

### 4.1. Stockage via `kleos-cli store` -- ce qui se passe vraiment

1. `POST /memories` arrive cote serveur
2. INSERT memory (+ embedding chunks si embedder available)
3. Background spawn : `fast_extract_facts(content)` regex (anglais
   consumer)
4. Si match -> INSERT INTO structured_facts
5. Si pas de match -> rien (cas dominant pour tech francais)
6. Aucun contradiction check inline (`detect_contradictions` n'est PAS
   appele a chaque store -- verifier dans le code si on veut etre sur,
   mais grep ne montre pas de call site dans routes/memory/mod.rs)

### 4.2. Conclusion : faut-il orienter la prise de note ?

- **Si l'objectif est juste de stocker pour recherche** : non, prendre
  des notes naturelles est suffisant. La recherche memory_search et
  context_build fonctionnent independamment des structured_facts.
- **Si l'objectif est de declencher la detection de contradictions ou
  patterns temporels** : oui, il faudrait soit :
  - Rediger en anglais avec phrases regex-friendly (`I bought X`, `I
    spent $Y on Z`) -- artificiel et inutile.
  - **Ou attendre l'enrichissement LLM-based** (chantier ouvert, non
    livre a date).
- **Recommandation operateur 2026-05-26** : **ne pas modifier la prise
  de note** pour servir Kleos. Les structured_facts sont un dead-end
  fonctionnel sur ce corpus. Continuer a stocker des faits atomiques
  pertinents via `kleos-cli store -c <cat>`, et utiliser la recherche
  via `context` / `search` pour retrouver les contradictions
  semantiques manuellement (l'humain reste meilleur que Kleos sur ce
  point precis).

---

## 5. Gaps et chantiers ouverts (a date 2026-05-26)

| Gap | Severite | Effort estime |
|---|---|---|
| `detect_contradictions(memory)` (single, ligne 22 contradiction.rs) n'a pas le filtre Patch 37 par space | Moyenne | 30min (memes 3 lignes JOIN + clause egalite) |
| Regex extraction anglais consumer-only -> 0 structured_facts pour tech francais | Haute (bloque tout le pipeline contradiction sur ce corpus) | 3-6h pour ajouter regex techniques + french, ou voie LLM enrichie |
| Pas de detection contradictions semantiques (`X faux puis vrai`) | Moyenne | 1-2j pour design + impl path LLM dedie ou enrichissement extract_facts |
| Temporal patterns peu utiles sur profil session Claude (noisy inter-arrival) | Basse | Hors scope (changement profil d'usage, pas du code) |
| Web GUI ne reflete pas space_id (cf. memoire #4850) | Moyenne | 2-4h (separe, GUI surface) |

---

## 6. References croisees

- `docs/dev-notes/local-patches.md` Patch 37 : filtre cross-space sur
  scan_all_contradictions + detect_fact_contradictions + detect_patterns
- `docs/dev-notes/dreamer-llm-contract.md` : prompts LLM Ollama utilises
  par le dreamer (growth/reflect)
- `docs/dev-notes/dreamer-tuning-todo.md` : seuils dreamer (idle_threshold,
  GROWTH_REFLECT_CHANCE)
- `docs/dev-notes/dreamer-brouillons-review-2026-05-20.md` : etat des
  reflexions dreamer
- `kleos-lib/src/intelligence/{extraction,contradiction,temporal,growth}.rs`
  : code source des composants
- `KLEOS.md` (global VOCSAP) section 9 "Familles MCP" : reflexes par
  famille intelligence

---

## 7. Glossaire rapide

- **structured_facts** : table relationnelle subject + predicate + object,
  remplie par regex au store ou par admin reprocess.
- **memory_links** : table edge (source_id, target_id, type, similarity).
  Type `contradicts` pose par `detect_contradictions` quand une
  contradiction est trouvee.
- **TemporalPattern** : pattern recurrent (daily/weekly/monthly) detecte
  par `detect_patterns` sur les inter-arrival times d'une (space,
  category).
- **fast_extract_facts** : passe regex synchrone sur le texte d'une
  memory.
- **extract_facts** : passe LLM dans le pipeline /ingest mode extract,
  decompose le texte en facts texte, stocke chacun comme nouvelle
  memory.
- **dreamer** : scheduler background dans kleos-server qui execute les
  passes intelligence (consolidate, contradictions, temporal, growth)
  toutes les 5 min si serveur idle.
