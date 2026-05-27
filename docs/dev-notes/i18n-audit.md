# i18n Audit -- Hardcoded English vocabulary across Kleos crates (2026-05-26)

**Status :** draft (uncommitted), audit read-only, working tree intact.
**Scope :** tous les crates du workspace Kleos (17 crates).
**Methode :** grep transverse sur patterns `Regex::new`, `const X: &[&str]`, `LazyLock<HashMap>`, `strip_prefix`, plus inspection ciblee des modules NLP.
**Objectif :** cartographier les sites avant decision de design Patch 38 (overrides standalone vs i18n core lexicon).

---

## 1. Synthese

- **15 sites distincts** identifies avec vocabulaire anglais hardcode.
- **14 sites concentres dans `kleos-lib/`**, 1 site dans `kleos-server/`.
- **Zero site** dans `eidolon-supervisor`, `agent-forge`, `forge`, `kleos-sidecar`, `kleos-sh`, `kleos-cli`, `kleos-mcp`, `kleos-cred(d)`, `kleos-fs`, `kleos-ingest`, `kleos-migrate`, `kleos-approval-tui`, `kleos-client`, `kleos-cleanup` (ces crates sont infra/admin/CLI, pas NLP).
- **Volume total** : ~400+ entrees lexicales + ~50 regex patterns avec vocab anglais.
- **Patterns paralleles** : `extraction.rs::like_regex` et `personality.rs::LIKE_PATTERN` font la meme chose avec presque les memes mots, sans partager le lexique. Premiere preuve concrete du gain DRY d'une i18n core.

## 2. Table par site

Colonnes :
- **Site** : `crate/path:line` du symbole.
- **Nature** : `regex` (regex avec mots litteraux), `wordlist` (const `&[&str]`), `lexicon` (HashMap mot->score), `markers` (discourse markers / stopwords), `articles` (strip articles).
- **Volume** : nombre approximatif de mots anglais embarques.
- **Layer** :
  - `A` = word-class substituable (le lexique est un set de mots reutilisables qu'on peut multilingualiser via dictionnaire).
  - `B` = pattern complexe sans mots reutilisables (i18n requiert override de regex entier).
  - `A+B` = mix (regex template + word slots).
- **Reuse** : nombre approximatif d'autres sites qui dupliquent semantiquement le meme vocab (estimation).

| # | Site | Nature | Volume | Layer | Reuse |
|---|---|---|---|---|---|
| 1 | `kleos-lib/src/intelligence/extraction.rs:23-102` (12 regex `buy/spent/have/exercise/made/earned/like/dislike/favorite/location/role`) | regex | ~50 verbes + structures | A+B | 1 (personality.rs duplique like/dislike/favorite) |
| 2 | `kleos-lib/src/intelligence/extraction.rs:387-431` `infer_domain` | wordlist | 7 domaines x ~5 mots (food/entertainment/reading/music/gaming/fitness/travel) | A | 0 |
| 3 | `kleos-lib/src/intelligence/temporal.rs:559` `STATE_VERBS` | wordlist | 10 verbes (is/has/lives/works/became/started/moved/lives in/works at/works as) | A | 1 (extraction.rs role_regex) |
| 4 | `kleos-lib/src/intelligence/valence.rs:21-125` `EMOTION_PATTERNS` | regex | 22 regex emotions ~120 mots (anger/fear/sadness/joy/excitement/pride/satisfaction/calm/gratitude/curiosity/accomplishment/admiration/surprise/confusion/fatigue/frustration/disgust) | A | 2 (sentiment.rs, personality.rs EMOTION_KEYWORDS) |
| 5 | `kleos-lib/src/intelligence/sentiment.rs:9-230` `SENTIMENT_LEXICON` | lexicon | 230+ entries AFINN-style (-5..+5) | A | 2 (valence.rs, personality.rs) |
| 6 | `kleos-lib/src/intelligence/decomposition.rs:33-43` `FILLER_PREFIXES` | markers | 9 phrases (so/well/basically/actually/honestly/like/i mean/you know/anyway) | A | 0 |
| 7 | `kleos-lib/src/intelligence/decomposition.rs:46-59` `META_STOPLIST` | markers | 12 phrases (let me explain/as i mentioned/in summary/...) | A | 0 |
| 8 | `kleos-lib/src/personality.rs:145-267` `EMOTION_KEYWORDS` | lexicon | 17 emotions (happy/excited/grateful/proud/sad/angry/frustrated/anxious/...) | A | 2 (valence.rs, sentiment.rs) |
| 9 | `kleos-lib/src/personality.rs:269-290` `INTENSIFIERS` | lexicon | 18 intensifiers (very/really/absolutely/extremely/...) | A | 0 |
| 10 | `kleos-lib/src/personality.rs:304-331` (7 lazy_regex `LIKE/DISLIKE/FAV/DECISION/IDENTITY/VALUE/MOTIVATION`) | regex | ~40 verbes/phrases | A+B | 1 (extraction.rs like_regex/dislike_regex/favorite_regex) |
| 11 | `kleos-lib/src/personality.rs:341-346` `clean_subject` strip articles | articles | 5 articles (a/an/the/my/our) | A | 0 |
| 12 | `kleos-lib/src/prompts.rs:253-264` `SCRUB_PATTERNS` | wordlist | 10 credential keywords (password/passwd/secret/token/api_key/...) | A | 0 (mots techniques majoritairement cross-lang sauf "password"->"mot de passe") |
| 13 | `kleos-lib/src/brain/hopfield/recall.rs:18-44` `STRONG_CAUSAL/CONTEXT_CAUSAL/WEAK_CAUSAL/NEGATION` | markers | 7 + 5 + 2 + 11 mots (caused by/because/never/not/...) | A | 0 |
| 14 | `kleos-lib/src/handoffs/atoms.rs:188-208` `RE_DECISION/RE_CONSTRAINT/RE_TASK/RE_QUESTION` | regex | ~30 verbes/phrases (we will/we should/must not/never/TODO:/need to/...) | A+B | 1 (personality.rs DECISION_PATTERN partially) |
| 15 | `kleos-lib/src/services/brain.rs:1482-1486` `stopwords` (inline HashSet dans une fn) | markers | 21 stopwords anglais (this/that/with/from/have/...) | A | 0 |
| 16 | `kleos-server/src/routes/gate/mod.rs:651-660` `PROHIBITIONS` (fn-local) | wordlist | 8 prohibition keywords (never/do not/don't/must not/prohibited/forbidden/blocked/banned) | A | 1 (recall.rs NEGATION) |

## 3. Patterns de duplication observes

Trois groupes thematiques apparaissent dans plusieurs sites, **sans partage de lexique** :

### Groupe A -- Preferences (love/like/hate)

- `extraction.rs::like_regex` + `extraction.rs::dislike_regex` + `extraction.rs::favorite_regex` (12 verbs)
- `personality.rs::LIKE_PATTERN` + `personality.rs::DISLIKE_PATTERN` + `personality.rs::FAV_PATTERN` (12 verbs, mostly overlapping)
- `valence.rs::EMOTION_PATTERNS` emotion "admiration" et "disgust" (love/hate words)
- `sentiment.rs::SENTIMENT_LEXICON` (love=5, hate=-5)
- `personality.rs::EMOTION_KEYWORDS` (n'a pas exactement love/hate mais happy/sad)

**Total dans le groupe** : ~50 verbes preferenciels en anglais repetes a 5 endroits sans partage.

### Groupe B -- Emotions (happy/sad/angry/etc.)

- `valence.rs::EMOTION_PATTERNS` 22 regex contenant ~120 emotion words
- `sentiment.rs::SENTIMENT_LEXICON` 230 entries dont nombreuses emotions
- `personality.rs::EMOTION_KEYWORDS` 17 emotion -> meta

**Triple duplication** : 3 lexiques d'emotions paralleles avec recouvrement significatif.

### Groupe C -- Negation / Prohibition / Causal

- `recall.rs::NEGATION` + `recall.rs::STRONG_CAUSAL/CONTEXT_CAUSAL/WEAK_CAUSAL`
- `gate/mod.rs::PROHIBITIONS` (overlap `never`, `do not`, `don't`)
- `handoffs/atoms.rs::RE_CONSTRAINT` (must not, cannot, never, always, required, forbidden, do not)

**Triple duplication** : negation/prohibition lexique disperse a 3 endroits.

## 4. Decoupage Layer A vs Layer B

### 4.1. Layer A pur (lexique reutilisable) -- candidates principaux

Ces sites consomment uniquement des word lists qu'on peut multilingualiser via dictionnaire central :

- `temporal.rs::STATE_VERBS` -- direct mapping
- `extraction.rs::infer_domain` -- 7 domaines x N keywords
- `personality.rs::EMOTION_KEYWORDS` -- emotion -> (valence, intensity)
- `personality.rs::INTENSIFIERS` -- intensifier -> multiplier
- `sentiment.rs::SENTIMENT_LEXICON` -- word -> AFINN score
- `decomposition.rs::FILLER_PREFIXES/META_STOPLIST` -- markers
- `recall.rs::STRONG_CAUSAL/CONTEXT_CAUSAL/WEAK_CAUSAL/NEGATION` -- markers
- `gate/mod.rs::PROHIBITIONS` -- markers
- `prompts.rs::SCRUB_PATTERNS` -- keywords
- `services/brain.rs::stopwords` -- markers
- `personality.rs::clean_subject` articles -- markers

**Volume Layer A pur :** ~500 mots, ~80% du total.

### 4.2. Layer A+B (regex avec slots de mots)

Ces sites construisent des regex qui melangent **mots reutilisables** et **structure syntaxique** (`(?:I\s+)?(verbs)\s+(...)`). i18n requiert :
- Layer A : substituer les listes de verbs
- Layer B : adapter la structure (`(?:I\s+)?` devient `(?:je\s+|j['e]\s+)?` en FR)

Sites concernes :
- `extraction.rs` 12 regex
- `personality.rs` 7 lazy_regex
- `valence.rs` 22 EMOTION_PATTERNS
- `handoffs/atoms.rs` 4 RE_* atoms

**Volume Layer A+B :** ~45 regex, structure parfois reutilisable entre langues.

### 4.3. Layer B pur (regex sans mots simples)

Aucun site identifie est strictement Layer B pur. Toutes les regex examinees ont au moins une liste de mots substituable.

## 5. Implications design

### 5.1. Pure pattern overrides (Patch 38 design actuel)

**Couvre** : sites Layer A+B mais **chaque pattern est dupplique** dans TOML par langue. 4 sites (extraction.rs / personality.rs / valence.rs / handoffs/atoms.rs) avec ~45 regex par langue = ~135 regex en TOML pour 3 langues (EN + FR + ES, par exemple). Maintenance lineaire dans le nombre de langues.

**Ne couvre pas** (ou couvre mal) : sites Layer A pur (wordlists, lexicon, markers). On peut ecrire un override TOML pour `STATE_VERBS` mais c'est un detournement de l'API "pattern override".

**Conclusion** : Patch 38 design actuel resoud ~30% du probleme audit. Le reste reste hardcode et duplique.

### 5.2. i18n core lexicon (vision operateur)

**Couvre** : sites Layer A pur via dictionnaire central `lexicon/<lang>.toml` declarant des word-classes (`verb_like`, `verb_buy`, `state_verbs`, `negation_markers`, `causal_strong`, `intensifier`, `articles`, `stopwords`, `emotion_happy`, `emotion_sad`, etc.).

**Couvre aussi** : sites Layer A+B via :
1. Substitution des word slots dans regex templates (`r"\b({verb_like})\s+..."` avec interpolation au load).
2. Override total du regex via mechanism Layer B fallback pour structures non decomposables.

**Sites refactores** : 15 sites au lieu de 4. Toucher :
- `extraction.rs` (12 regex + infer_domain)
- `temporal.rs` (STATE_VERBS)
- `valence.rs` (22 patterns)
- `sentiment.rs` (lexicon entier)
- `decomposition.rs` (markers)
- `personality.rs` (7 regex + 2 lexicons + clean_subject)
- `prompts.rs` (SCRUB_PATTERNS)
- `recall.rs` (4 markers)
- `handoffs/atoms.rs` (4 regex)
- `services/brain.rs` (stopwords inline -> extraction const)
- `gate/mod.rs` (PROHIBITIONS)

**Niveau delta upstream :** "patch chirurgical multi-sites" (cf. `~/.claude/rules/rust-upstream-fork.md`). Toucher ~12 fichiers cote kleos-lib + 1 cote kleos-server. Significatif mais reste sous "refactor local" parce que les signatures publiques restent inchangees (les fns conservent leur API, c'est juste le contenu qui consomme un lexicon).

**Conclusion** : i18n core resoud ~95% du probleme audit. Premier deploy paraitra cher (12 fichiers touches) mais l'effort marginal d'ajouter une nouvelle langue est ensuite ~0 (juste un nouveau fichier `lexicon/<lang>.toml`).

### 5.3. Hybride recommande

Choisir **i18n core** pour les sites Layer A pur (le gain DRY justifie le delta), **pattern overrides** pour les sites Layer A+B ou la structure regex elle-meme varie linguistiquement (ex : negation FR discontinue `ne ... pas`).

Layout cible :
```
i18n-overrides/
  lexicon/
    en.toml                          # baseline (peut copier embedded ou rester vide)
    fr.toml                          # word-classes FR
  patterns/
    intelligence/
      extraction/
        facts.fr.toml                # patterns regex complets pour la negation FR
        facts.tech.toml              # patterns regex tech (lang-agnostic souvent)
```

Les regex code-side deviennent templates :
```rust
fn like_regex_for(lang: &str) -> Regex {
    let verbs = lexicon::word_class(lang, "verb_like").join("|");  // "love|like|enjoy|adore|prefer"
    Regex::new(&format!(r"(?i)\b(?:{})\s+(?:{})\s+(.+?)(?:\.|,|$)",
        lexicon::word_class(lang, "first_person_pronoun").join("|"),
        verbs
    )).unwrap()
}
```

Loop sur les langues supportees :
```rust
for lang in lexicon::supported_languages() {
    for cap in like_regex_for(lang).captures_iter(content) {
        facts.push(...)
    }
}
```

Ou cache statique par langue :
```rust
static LIKE_REGEX_BY_LANG: LazyLock<HashMap<String, Regex>> = LazyLock::new(|| {
    lexicon::supported_languages().iter()
        .map(|lang| (lang.clone(), like_regex_for(lang)))
        .collect()
});
```

## 6. Sites non-i18n (false positives ecartes pendant l'audit)

Pour reference, les const wordlists suivantes ont ete examinees et **ne sont pas** des cibles i18n (vocabulaire technique cross-langue ou enum protocol) :

- `HANDOFFS_COLUMNS`, `SKIP_COLUMNS`, `FK_ORDERED_TABLES`, `SKIP_TABLES`, `AUXILIARY_SCHEMA_STATEMENTS` (SQL)
- `VALID_TASK_TYPES`, `VALID_STATUSES` (agent-forge enums)
- `KNOWN_SUBCOMMANDS`, `CODE_EXTENSIONS`, `SUPPORTED_EXTENSIONS` (CLI)
- `READ_ONLY_TOOLS`, `TOOLS_REQUIRING_APPROVAL` (Anthropic Claude Code tool names)
- `OPEN_PATHS`, `SPA_ROUTES`, `ALLOWED_CATEGORIES` (HTTP paths)
- `CONTENT_FIELDS`, `TITLE_COLUMN_NAMES` (CSV/JSONL field names)
- `SKIP_TAGS` (HTML tag names)
- `VALID_PROJECT_STATUSES`, `VALID_STEP_TYPES`, `VALID_DRIFT_TYPES`, `VALID_SEVERITIES`, `VALID_RATINGS`, `ALLOWED_LLM_ACTIONS` (status/type enums)
- `DEFAULT_ALIASES` (space aliases)
- `INDEXABLE_APP_TYPES` (app type enum)
- `PKCS11_LIB_PATHS` (filesystem paths)
- `CODE_DEV_KEYWORDS` (intentionnel : tags techniques anglais conventionnels)

## 7. Recommandation operateur

**Option A** : Patch 38 etroit (TOML pattern overrides comme design redige). Resoud ~30% du probleme. Effort initial 1-2 jours. Maintenance lineaire en langues * sites Layer A+B.

**Option B** : Patch 38 i18n core complet (lexicon central + pattern overrides hybride). Resoud ~95% du probleme. Effort initial 3-5 jours. Maintenance constante en nombre de langues (chaque nouveau lang = 1 fichier).

**Option C** : Patch 38 i18n core MVP (lexicon central uniquement, sans pattern overrides). Resoud ~70% du probleme (les sites Layer A pur). Effort initial 2-3 jours. Pattern overrides reportes a Patch 39 si necessaire pour negation FR discontinue ou structures regex specifiques.

**Penchant peer kleos-2 :** Option C (MVP lexicon central). Justification :
- Le gros gain DRY est sur les sites Layer A (80% du volume).
- Les sites Layer A+B peuvent etre traites via templating cote code (`format!()` avec word-class) sans introduire un mecanisme de pattern overrides distinct.
- Pattern overrides etait justifie quand on pensait au seul site extraction.rs. Avec 15 sites identifies, le lexique partage est l'unite naturelle de DRY.
- On peut toujours ajouter Patch 39 pattern overrides ulterieurement pour les cas regex structurel.

## 8. References

- `kleos-lib/src/intelligence/extraction.rs` (12 regex anglais)
- `kleos-lib/src/intelligence/temporal.rs:559` STATE_VERBS
- `kleos-lib/src/intelligence/valence.rs:21-125` EMOTION_PATTERNS
- `kleos-lib/src/intelligence/sentiment.rs:9-230` SENTIMENT_LEXICON
- `kleos-lib/src/intelligence/decomposition.rs:33-59` markers
- `kleos-lib/src/personality.rs:145-346` EMOTION_KEYWORDS / INTENSIFIERS / 7 lazy_regex / clean_subject
- `kleos-lib/src/prompts.rs:253-264` SCRUB_PATTERNS
- `kleos-lib/src/brain/hopfield/recall.rs:18-44` causal markers
- `kleos-lib/src/handoffs/atoms.rs:188-208` RE_DECISION/RE_CONSTRAINT/RE_TASK/RE_QUESTION
- `kleos-lib/src/services/brain.rs:1482-1486` inline stopwords
- `kleos-server/src/routes/gate/mod.rs:651-660` PROHIBITIONS
- `docs/dev-notes/patch-38-regex-overrides-design.md` (design Patch 38 actuel, a re-evaluer a la lumiere de cet audit)
- `docs/dev-notes/intelligence-pipeline-overview.md` (analyse extraction.rs originale)
- `~/.claude/rules/rust-upstream-fork.md` (table de delta upstream)

---

## 7. Status post-Patch 38 (2026-05-27)

Patch 38 livre sur branche `local/patch-38-i18n-core` HEAD `f0b556a5`. Statut par site :

| # | Site | Livre | Differable |
|---|---|---|---|
| 1 | extraction.rs 12 regex | 5/12 (like, dislike, favorite, location, role) | 7/12 (buy, spent, have, exercise, made, earned -- unit/currency EN-only) |
| 2 | extraction.rs::infer_domain | oui | -- |
| 3 | temporal.rs::STATE_VERBS | oui | -- |
| 4 | valence.rs EMOTION_PATTERNS | 0/22 | 22 patterns avec metadata valence+arousal, TOML lourd |
| 5 | sentiment.rs SENTIMENT_LEXICON | oui (decoupage 10 buckets par score) | -- |
| 6 | decomposition.rs FILLER_PREFIXES | oui | -- |
| 7 | decomposition.rs META_STOPLIST | oui | -- |
| 8 | personality.rs EMOTION_KEYWORDS | oui (17 emotion classes lexicon) | -- |
| 9 | personality.rs INTENSIFIERS | oui (5 tier classes lexicon) | -- |
| 10 | personality.rs 7 lazy_regex | 7/7 | -- |
| 11 | personality.rs clean_subject articles | oui (NB: pas folde, position-based) | -- |
| 12 | prompts.rs SCRUB_PATTERNS | oui | -- |
| 13 | recall.rs causal + NEGATION | oui (NB: hopfield causal compute_causal_score position-based, pas folde) | -- |
| 14 | atoms.rs 4 RE_* | oui | -- |
| 15 | services/brain.rs stopwords | oui | -- |
| 16 | gate/mod.rs PROHIBITIONS | oui | -- |

### Couche normalisation ajoutee

- `lexicon::fold_for_matching(s, lang, with_stem)` -- lowercase + Unicode NFD strip diacritics + Snowball stemming optional.
- `lexicon::fold_word_for_class(word, lang, class)` -- consulte le metadata `stem` de la classe.
- Crates ajoutees au workspace : `unicode-normalization` 0.1, `rust-stemmers` 1.2.
- 9 classes "mots-grammaire" marquees `stem = false` (state_verbs, articles, stopwords, first_person_pronoun, negation_marker, intensifier_*, credential_keywords).
- 8 / 11 sites L2.A patches utilisent le folding ; 3 sites position-based (clean_subject, hopfield causal, decomposition strip_filler) restent en comparaison surface.

### Reste a faire (cf. docs/dev-notes/patch-38-remaining-work.md)

- valence.rs 22 EMOTION_PATTERNS : 22 classes lexicon avec valence + arousal metadata par classe.
- extraction.rs 7 patterns unit-specific (buy/spent/have/exercise/made/earned) : design dedie pour EN/FR currency + units.
- Livrable 3 : migrations v66 (UNIQUE INDEX structured_facts) + v67 (extraction_source column) + 3 admin endpoints (reextract-facts, lexicon/validate, lexicon/reload).
- Sync submodule lexicon-overrides avec TOMLs maj.
- Build WSL + deploy LXC 121 + smoke E2E contradiction FR cross-space.
