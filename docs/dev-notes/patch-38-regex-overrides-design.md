# Patch 38 -- Regex extraction overrides (i18n + tech) -- Design

**Status :** draft (uncommitted), pending operator review.
**Auteurs :** peers Claude Code (desktop-7b2civn-kleos + kleos-2), session 2026-05-26.
**Branche cible :** `local/patch-38-regex-overrides` depuis `d6157f5d` (sur `local/patch-33-spaces-integration`).
**Reference upstream :** candidat PR (additif pur, zero touche extraction.rs upstream).
**Patch precondition recommande :** Patch 37.1 (sibling chirurgical de 37, applique le filtre space sur `detect_contradictions(memory)` single path -- 30min, separe, deploye AVANT Patch 38).

---

## 1. Probleme

`kleos-lib/src/intelligence/extraction.rs` (`fast_extract_facts`) ne couvre que 12 regex anglais consumer/lifestyle (`bought N X`, `spent $X on Y`, `like Y`, etc.). Sur un corpus tech francais reel (LXC 121 prod) : ~2 structured_facts pour 2282 memories (taux <0.1%). Tout le pipeline `/intelligence/contradictions` et `/intelligence/temporal/detect` est dead-end fonctionnel sur ce corpus parce qu'il consomme exclusivement la table `structured_facts`.

Voir `docs/dev-notes/intelligence-pipeline-overview.md` (commit d6157f5d) pour l'analyse complete.

## 2. Approche -- pattern overlay miroir Patch 15/16 (prompts-overrides)

i18n des regex via cascade de chargement, comportement upstream strictement preserve quand env var unset.

### 2.1. Architecture

- **Niveau delta upstream :** additif pur (cf. `~/.claude/rules/rust-upstream-fork.md`).
- **Nouveau module :** `kleos-lib/src/intelligence/extraction_overrides.rs`.
- **API :** `pub(crate) fn extract(content: &str, date_ref: Option<&str>) -> (Vec<FactInsert>, Vec<PrefUpsert>, Vec<StateUpsert>)`.
- **Touche `extraction.rs` :** 3 lignes additives entre les boucles regex et le `db.write(...)` block (extend + dedup HashSet par tuple `(subject, predicate, object)`).
- **Types deplaces :** `FactInsert` / `PrefUpsert` / `StateUpsert` vers `intelligence/types.rs` (alignement avec `ExtractionStats`).

### 2.2. Cascade resolution du repo overrides

1. `KLEOS_REGEX_OVERRIDES_REPOSITORY` env explicite si set.
2. `${KLEOS_DATA_DIR}/regex` si le dossier existe (default LXC 121 `/var/lib/kleos/regex`).
3. Aucun -> embedded only, comportement upstream strict.

### 2.3. Layout du repo

```
regex-overrides/
  intelligence/
    extraction/
      facts.fr.toml
      facts.tech-deploy.toml
      facts.tech-network.toml
      prefs.fr.toml
      states.fr.toml
      README.md
```

Repo **in-repo** (pas submodule), miroir style `gate-rules/`. Decouplage path-only : `env > KLEOS_DATA_DIR > embedded` permet migration ulterieure vers submodule sans changement code.

## 3. Format TOML

Triple-string raw `'''...'''` pour les regex (preserve backslashes), named captures `(?P<name>...)` requises.

```toml
schema_version = 1

[[fact]]
name = "fr.tech-deploy.deployed"               # convention <lang>.<domain>.<verb>
regex = '''(?i)\bj['e]ai\s+(?P<verb>deploye|deploye|lance)\s+(?P<object>.+?)(?:\.|,|$)'''
subject = "user"                                # litteral preferentiel pour actions operateur
predicate = "deployed"                          # litteral OU predicate_capture = "verb"
captures = { object = "object" }                # mapping group_name -> structured_fact_field
confidence = 0.7                                # optionnel, par-pattern. Defaut unique 0.8 si omis.

[[fact]]
name = "fr.tech-deploy.not-deployed"            # voie A negation : pattern separe
regex = '''(?i)\bj['e]ai\s+(?:pas\s+|jamais\s+)(deploye|deploye)\s+(?P<object>.+?)(?:\.|$)'''
subject = "user"
predicate = "deployed"
captures = { object = "object" }
object_prefix = "NOT_"                          # prefix automatique sur la valeur capturee
# confidence omis -> default 0.8
```

### 3.1. Convention subject normalisation (critique pour dedup)

- `subject = "user"` litteral pour actions de l'operateur (defaut FR + EN doivent matcher).
- Entites nommees pour le reste (`subject = "service"`, ou capture group `subject_capture = "subject"` quand l'entite est dans le texte).
- **Sans cette convention, dedup foire silencieusement** quand un fait equivalent est exprime en FR et EN dans la meme memory.

### 3.2. Negation

**Voie A par defaut** : patterns positive + negative separes, `object_prefix = "NOT_"` automatique sur le pattern negatif. `objects_match` cote `contradiction.rs` voit `deployed` != `NOT_deployed` -> contradiction detectee mecaniquement. Zero touche cote consumer.

Voie B (negation_capture syntaxique sugar dans un seul regex bidirectionnel) reportee a Patch 38.1 si volume de patterns justifie.

## 4. Composition + dedup

- Embedded TOUJOURS evalues d'abord (preserve upstream strict).
- Override iteres ensuite, accumulation pas first-match-wins.
- **Dedup au write** : HashSet<(subject, predicate, object)> avant `INSERT INTO structured_facts`. Evite doublons quand le meme fait matche par 2 regex (EN + FR) sur le meme content.
- Le dedup HashSet est **local a une extraction (1 memory = 1 passe)**, pas cross-memory. Le UNIQUE INDEX v66 sur `(memory_id, subject, predicate, object)` complete cross-memory mais c'est une protection serveur (INSERT OR IGNORE), pas de la dedup applicative.
- Pas de scoring/ordering, simple ordre de declaration TOML.

## 5. Cache + compilation

Pattern hybride :
- `RegexSet` pour pre-filter O(1) any-match check.
- `Vec<Regex>` pour capture sur les patterns identifies matched par `set.matches()`.
- Threshold : `RegexSet` si `patterns.len() > 50`, sinon loop simple.
- Reference : Patch 25 gate matcher utilise deja le meme idiom.

Cache TTL 5s, Arc<CompiledOverrides>. Rebuild si mtime du repo a change (walk shallow sur les *.toml). Miroir `kleos-lib/src/llm/prompts.rs::TTL_SECS`.

## 6. Robustesse (ReDoS)

- `Regex::with_size_limit(<defaut crate>)` : borne implicite contre catastrophic backtracking.
- Per-memory timeout defensif : `tokio::time::timeout(Duration::from_millis(500), extract_overrides(content))` cote `fast_extract_facts`. Si timeout, `warn!` + skip ce content + continue le pipeline. Localise le degat a 1 memory.
- Endpoint admin `POST /admin/regex-overrides/validate` (MVP+1) : pre-flight pour l'operateur, execute chaque pattern sur 10 strings synthetiques, mesure wall-clock, reject si >50ms. **Pas automatique au load**, c'est un outil de test avant `git push`.
- Pattern TOML invalide ou `Regex::new` qui plante -> `warn!` + skip ce pattern, le reste continue. Ne casse jamais le pipeline.

## 7. Migrations DB

- **v66 (idempotent)** : `CREATE UNIQUE INDEX IF NOT EXISTS idx_structured_facts_subj_pred_obj ON structured_facts(memory_id, subject, predicate, object)`. Precondition : dedup repair des duplicats existants si besoin (sub-routine au moment de la migration, sinon `CREATE UNIQUE INDEX` plante).
- **v67 (idempotent, append-only)** : `ALTER TABLE structured_facts ADD COLUMN extraction_source TEXT NOT NULL DEFAULT 'embedded'`. Guard via `table_has_column`. Rows existants -> default `'embedded'` litteral, pas NULL. AC (7).
- INSERT side : `INSERT OR IGNORE INTO structured_facts (..., extraction_source) VALUES (...)`. Idempotent.
- Manifests `migrations.manifest` + `tenant_migrations.manifest` mis a jour. Test CI append-only protect contre renumerotation (cf. `~/.claude/rules/rust-upstream-fork.md` lesson migrations).

## 8. Admin endpoints

- `POST /admin/reextract-facts?dry_run=true&since=DATE&space=X&memory_ids=[...]` : opt-in migration legacy. `dry_run` retourne JSON `{by_memory: [{memory_id, current_facts, new_facts, diff}], totals: {...}}` sans muter. `since`/`space`/`memory_ids` filtrent pour eviter de retraiter 2282 mems sur un test. Pas auto au boot.
- `POST /admin/regex-overrides/validate` (MVP+1) : charge le repo, compile tout, retourne `{loaded: N, ok: [...], errors: [{file, pattern_name, error}], slow: [{pattern_name, ms}]}`.

## 9. Tests

### 9.1. Unitaires (extraction_overrides::tests)

Matrix 5 axes : langue x domaine x positive/negative x quantite-presente/absente x dedup-needed/pas-besoin. ~20 tests minimum.

Inclut :
- Pattern TOML invalide -> warn + skip (le reste compile).
- Regex compile fail -> warn + skip.
- Repo inexistant -> fallback embedded only (zero erreur).
- Hot-reload : mtime change apres TTL -> reload, comportement nouveau.
- Dedup : "I deployed kleos" + "j'ai deploye kleos" dans la meme memory -> 1 fact apres dedup.
- Negation Voie A : "j'ai pas deploye kleos" -> 1 fact `(user, deployed, NOT_kleos)`.
- ReDoS : pattern catastrophic dans le set + content adversarial -> per-memory timeout 500ms declenche, warn + skip, autres memories OK.

### 9.2. Integration e2e (kleos-server)

Smoke test FR contradiction :
1. Seed repo regex temp avec 3 patterns FR (`fr.tech-deploy.deployed`, `fr.tech-deploy.not-deployed`, etc.).
2. Seed 2 memories FR contradictoires sur le meme subject+predicate.
3. Trigger `fast_extract_facts` via `POST /memories`.
4. Assert que `structured_facts` contient les bonnes rows (subject, predicate, object, extraction_source=`fr.tech-deploy.deployed`).
5. Trigger `POST /intelligence/contradictions`.
6. Assert que la contradiction est detectee.

## 10. Acceptance criteria

1. 0 fact extrait sur corpus FR tech avant ; >=10 patterns FR chargeables apres ; >=10 facts emis sur un seed de 50 memories FR.
2. Embedded patterns intacts en behavior (test contre fixtures upstream).
3. Dedup au write fonctionne (HashSet par tuple).
4. Admin endpoint `dry_run` + `since` + `space` operationnels.
5. Migration v66 + v67 idempotente, manifests append-only, test CI present.
6. Colonne `extraction_source` populated pour chaque fact insere (jamais NULL).
7. Embedded rows existants ont `extraction_source = 'embedded'` litteral (pas NULL), via default migration v67.
8. >=1 contradiction FR detectable en e2e test.
9. ReDoS protection : per-memory timeout 500ms + size_limit, validation endpoint MVP+1.

## 11. Edge cases declares

1. TOML invalide -> skip + warn.
2. Regex compile fail -> skip + warn.
3. Repo inexistant -> fallback embedded only.
4. Hot-reload TTL 5s.
5. Dedup sur tuple meme apres N regex matchent.
6. Negation discontinue FR (`ne ... jamais sans X`) limitee, documente comme chantier ulterieur path LLM enrichi.
7. ReDoS catastrophic pattern -> per-memory timeout + skip + warn.
8. `disable_embedded` non supporte au MVP (YAGNI -> Patch 38.1 si necessaire).
9. Cas hypothetique d'overmatch embedded EN sur FR (peu probable vu que les regex embedded demandent des mots anglais explicites comme `like|love|enjoy|bought|spent`) -> accepte tel quel au MVP, bruit residue acceptable si observe. `disable_embedded` reporte a Patch 38.1 si necessaire.

## 12. Bonus -- benefice transitif sur LLM extract_facts path

`kleos-lib/src/ingestion/processors/extract.rs::extract_facts` produit du texte qui repasse par regex. Avec les nouveaux patterns FR/tech, ce path s'enrichit automatiquement -- pas de touche `extract.rs`. Benefice partiel : depend de la similarite stylistique entre l'output LLM et les patterns regex declares. **Document dans le README de `regex-overrides/`.**

## 13. Rejected alternatives

Alternatives pesees et explicitement rejetees pendant la convergence design. Documentees pour eviter qu'une session future les repropose sans connaitre les raisons.

| Alternative | Raison du rejet |
|---|---|
| **YAML pour le format d'override** | Indentation fragile + escaping cauchemardesque pour les regex (`'(?i)\\\\b...'` au lieu de raw string). Disqualifie. |
| **JSON pour le format d'override** | Pas de commentaires natifs, escaping double backslash (`\\\\b` au lieu de `\b`), pas de multiline natif. TOML triple-string raw `'''...'''` est strictement superieur pour les regex. |
| **Refactor embedded patterns vers TOML** | Niveau "refactor local" voire "reecriture" cf. `~/.claude/rules/rust-upstream-fork.md` table. Tout future evolution upstream sur `extraction.rs` deviendrait un merge manuel painful. Benefice d'uniformisation cosmetique seulement. Asymetrie embedded (Rust `&'static`) + overrides (TOML) est exactement le pattern de `prompts-overrides`. |
| **Submodule git pour `regex-overrides/`** | Surplus de plumbing operationnel + risque "operateur oublie de `git pull` cote LXC". Pattern in-repo style `gate-rules/` est plus simple pour le MVP. Migration submodule ulterieure triviale (path resolu via `env > KLEOS_DATA_DIR`, pas de changement code). |
| **Voie B negation -- single regex bidirectionnel avec `negation_capture`** | Sucre syntaxique. Voie A (patterns positive/negative separes avec `object_prefix = "NOT_"`) suffit pour le MVP. Voie B reportee a Patch 38.1 si volume de patterns le justifie. |
| **Bundle Patch 37.1 dans Patch 38** | Dilue 2 patches : 37.1 (sibling chirurgical de 37) + 38 (i18n regex) scope mixed = candidat PR upstream plus difficile a evaluer. Sequential reduit blast radius. Deploy 37.1 d'abord (5min, faible risque), puis 38 (gros lot avec tests). |
| **`disable_embedded = [...]` manifest pour neutraliser un pattern embedded** | YAGNI. Si un pattern embedded overmatche sur FR (ex `like_regex` sur "j'aime bien que..."), accepter le bruit residue au MVP. Mesurer en pratique. Si necessaire, Patch 38.1 ajoute 12 if-guards niveau "patch chirurgical". |
| **Veto patterns post-hoc cote module overrides** (filtrer FactInsert generes par embedded a posteriori) | Complexite forte pour un cas hypothetique. Rejected. |
| **First-match-wins par fact_type** (1 fact max par predicate par memory) | Regression vs comportement upstream (un memory peut produire N facts via plusieurs regex differents). Accumulation + dedup `(subject, predicate, object)` preserve la semantique upstream. |
| **`domain` colonne dans `structured_facts`** | Changement de comportement non neutre (ouvre dimension scoping pour contradictions intra-domain). Pas necessaire. `extraction_source` colonne (v67) couvre le besoin debug/rollback sans ouvrir nouvelle dimension. |
| **Auto-rejoue extraction sur toutes les memories au boot apres deploy Patch 38** | Risque dataloss + cout 10s blocage scheduler + duplicats si pas d'index unique. Migration opt-in via `POST /admin/reextract-facts?dry_run&since&space`. |
| **Validation endpoint automatique au load runtime** | Mauvais coupe-circuit (un seul pattern lent fait planter le boot). Mieux : `Regex::with_size_limit` au load (structurel) + `tokio::time::timeout(500ms)` per-memory (defensif runtime) + endpoint validate manuel pour pre-flight operateur. |

---

## 14. Resume executif

Format TOML hierarchique (schema_version=1, named captures, convention subject="user"), cascade `env > KLEOS_DATA_DIR/regex > embedded`, accumulation + dedup au write, additif pur (module separe + 3 lignes appel), cache TTL 5s avec RegexSet>50 threshold, ReDoS protection (size_limit + per-memory timeout 500ms), migrations v66 UNIQUE INDEX + v67 extraction_source append-only, admin endpoints reextract dry_run + validate MVP+1, ~20 tests unitaires + smoke e2e FR. Cout rebase upstream = quasi nul.

**Patch 37.1 livre AVANT, separe.**

---

## 15. Open items pour l'operateur

1. Bundle Patch 37.1 dans Patch 38 ou separe ? Penchants peers : separe (consensus apres iteration design). Operator confirme.
2. Submodule git vs in-repo dossier ? Default proposed : in-repo (`regex-overrides/`). Migration submodule ulterieure triviale.
3. Endpoint `/admin/regex-overrides/validate` au MVP ou MVP+1 ? Default proposed : MVP+1 (le validate est confort, pas bloquant pour la fonctionnalite).
4. Convention `confidence` : default unique 0.8 (pas de distinction embedded vs overrides). Par-pattern overrideable dans TOML si l'operateur veut signaler une regex experimentale (ex `confidence = 0.5`). Plus juste qu'un default different par origine. Revise apres iteration peer.
5. Threshold de declenchement spec_task : qui drive (desktop-7b2civn-kleos sur la branche, kleos-2 en peer review) ?
