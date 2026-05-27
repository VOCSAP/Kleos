# Patch 38 -- Reste a faire (post 2026-05-27)

Etat du tronc : branche `local/patch-38-i18n-core` HEAD `f0b556a5`.
Couvre L1 + L2.A 12/12 + normalize + L2.B 3/4. Detail : voir
`docs/dev-notes/local-patches.md` section Patch 38, et
`docs/dev-notes/i18n-audit.md` section 7 (status post-Patch 38).

## Roadmap restante (ordre suggere)

### 1. valence.rs 22 EMOTION_PATTERNS

Site 4 de l'audit. Chaque pattern porte `valence` (`f64` dans [-1, 1]) et
`arousal` (`f64` dans [0, 1]). Decoupage propose :

- **Strategy A** : 22 classes lexicon distinctes `valence_<emotion>_<tier>` (ex:
  `valence_anger_intense`, `valence_anger_mild`, `valence_fear_intense`,
  `valence_fear_mild`, ...). Chaque classe declare `valence` + `arousal` dans le
  TOML metadata. Avantage : symetrique aux 17 classes `emotion_*` du L2.A
  (qui portent deja `valence` + `intensity`). Inconvenient : 22 nouvelles
  classes EN + 22 FR a remplir.
- **Strategy B** : 1 classe lexicon globale `valence_emotions` avec une map
  `{word -> (emotion, valence, arousal)}`. Necessite d'etendre le format TOML
  pour porter un dictionnaire word -> tuple. Avantage : 1 seule classe.
  Inconvenient : extension format TOML non triviale, perd la lisibilite.
- **Strategy C** : Mutualiser avec les 17 classes `emotion_*` existantes en
  ajoutant juste `arousal` comme metadata supplementaire et un `tier` interne
  (intensite : intense / mild). Avantage : zero duplication avec L2.A.
  Inconvenient : il faut etendre `LexiconClass` avec `arousal: Option<f64>` et
  un `tier: Option<String>` pour preserver les groupes intense/mild.

**Recommandation** : Strategy C. Le code consommateur devient un mapping
`emotion -> regex assemble a partir des classes emotion_<X>_intense +
emotion_<X>_mild`. Volume TOML modeste, design coherent avec L2.A.

### 2. extraction.rs 7 patterns unit-specific

`buy_regex`, `spent_regex`, `have_regex`, `exercise_regex`, `made_regex`,
`earned_regex` (+ un legacy). Ces patterns encodent :

- Quantite numerique (`\s+(\d+)\s+`) : preservable cross-lang.
- Devise (`\$([\d,.]+)`) : EN-only ; FR utilise `\d+(?:,\d+)?\s*€` ou
  `\d+\s+euros`. Necessite un template par convention monetaire.
- Unites de temps / distance (`hours?|minutes?|mins?|miles?|km`) : EN
  metric et imperial melanges ; FR aurait `heures?|minutes?|mins?|kilometres?|km`.

**Recommandation** : nouvelle classe lexicon `time_units` + `distance_units`
+ `currency_symbols` (un set par langue). Le helper `_regex_for(lang)`
interpole ces classes. Layer A+B avec metadata par classe.

### 3. Livrable 3 -- migrations + admin endpoints

Reference plan original `~/.claude/plans/je-pr-f-re-b-complet-robust-kazoo.md`
section Livrable 3.

- **Migration v66** : `CREATE UNIQUE INDEX IF NOT EXISTS
  idx_structured_facts_subj_pred_obj ON structured_facts(memory_id, subject,
  predicate, object)`. Precondition : sub-routine `repair_legacy_duplicates`
  pour purger les doublons existants avant la creation de l'index unique
  (sinon SQLite refuse l'index sur table avec violations).

- **Migration v67** : `ALTER TABLE structured_facts ADD COLUMN extraction_source
  TEXT NOT NULL DEFAULT 'embedded'`. Guard via `table_has_column` pour
  idempotence. Permet `DELETE WHERE extraction_source = 'fr.bad_pattern'`
  pour rollback selectif si un override TOML fr.toml produit du bruit.

- **Manifests append-only** : `migrations.manifest` + `tenant_migrations.manifest`
  mis a jour. Test CI append-only protect contre renumerotation (cf.
  `~/.claude/rules/rust-upstream-fork.md` lesson).

- **Endpoint `POST /admin/reextract-facts?dry_run=true&since=DATE&space=X&memory_ids=[...]`** :
  opt-in migration legacy. `dry_run=true` retourne le JSON
  `{by_memory: [...], totals: {...}}` sans muter. Pas auto au boot.

- **Endpoint `POST /admin/lexicon/validate`** : charge le repo lexicon,
  compile tout, retourne `{loaded: N, ok: [...], errors: [{file, class,
  error}]}`. Pre-flight pour l'operateur avant `git push`.

- **Endpoint `POST /admin/lexicon/reload`** (optionnel) : force purge cache
  lexicon avant TTL si urgence.

### 4. Sync submodule lexicon-overrides

Le repo `VOCSAP/Kleos.lexicon` est encore au commit initial `7a29dd8` avec
les anciennes versions des TOMLs (sans accents corrects, sans les nouvelles
classes ajoutees pendant L2.A et L2.B).

A faire :

```bash
cd lexicon-overrides
git fetch origin
git pull origin main
cp ../kleos-lib/lexicon/en.toml ./en.toml
cp ../kleos-lib/lexicon/fr.toml ./fr.toml
git add en.toml fr.toml
git commit -m "Sync from kleos-lib/lexicon at Patch 38 final"
git push origin main
cd ..
git add lexicon-overrides
git commit -m "chore: bump lexicon-overrides pointer to <hash>"
```

### 5. Build WSL + deploy LXC 121

```bash
# Cote WSL
cd /mnt/c/Users/Olivier/workspace/claude-experiment/Kleos
cargo build --release -p kleos-server --target x86_64-unknown-linux-gnu --features version-tag
```

### 6. Smoke E2E FR cross-space contradiction

```bash
# Cote operateur, avec lexicon FR deploye
kleos-cli store "agent-forge n'expose pas d'aide via --help" -c discovery --space patch-38-test
kleos-cli store "agent-forge expose une aide complete via --help" -c discovery --space patch-38-test
# Trigger pipeline + scan
curl -s -X POST http://192.168.10.21:4200/intelligence/contradictions \
  -H "Authorization: Bearer $KLEOS_API_KEY" | jq '.count, .contradictions[].confidence'
# Assert : count >= 1, confidence > 0.5
```

Egalement : creer 2 memoires meme subject+predicate mais objects differents
dans 2 spaces differents, verifier qu'aucune contradiction cross-space
n'est detectee (Patch 37.1 + Patch 38 ensemble).

## Decisions deja prises

| Decision | Justification |
|---|---|
| Pas de LLM embedded pour le matching | Overkill, 50-500 ms par lookup vs nanoseconds pour strip-accents + stem |
| Pas de lemmatiseur Rust pure | Ecosysteme Rust manque d'options production pour FR. Stemmer Snowball + variantes TOML manuelles couvrent 80% |
| Stemming opt-out par classe via `stem = false` | Eviter over-stemming sur grammar / tokens techniques |
| `lexicon::class_emotion_metadata` separe de `word_class` | API minimaliste, metadata seulement pour classes qui en ont besoin |
| Submodule `lexicon-overrides/` separe de l'embedded baseline | Embedded = source-of-truth garanti, override = customization operateur |
| README public sans nom des repos prives | Decouplage editorial : forks publics ne sont pas exposes aux conventions VOCSAP |

## Decisions a prendre

- valence.rs Strategy A vs B vs C (cf. section 1).
- extraction.rs unit-specific patterns : 1 classe par unite ou 1 classe par dimension (time / distance / currency) ?
- Submodule lexicon-overrides : auto-sync via CI (post-push sur main) ou manuel ?
- Smoke E2E : tester avec memoires FR-only ou bilingues ?
