# Error Tracked -- known failing tests / issues to fix later

Suivi des erreurs connues (tests rouges, bugs identifies mais non bloquants) a
corriger ulterieurement. Chaque entree note la cause racine confirmee et si
c'est une regression ou un bug pre-existant, pour ne pas re-investiguer.

---

## 5 tests rouges -- Patch 38 i18n (lexicon / handoff atoms) -- PRE-EXISTANTS

**Detecte :** 2026-06-01, post-merge aa6a0bec, via `cargo test --lib -p kleos-lib --features bundled-sqlite` (947 passed / 5 failed).

**Statut :** PRE-EXISTANTS (pas une regression du merge aa6a0bec). Preuve : `kleos-lib/src/lexicon/mod.rs`, le loader, et les baselines `kleos-lib/lexicon/{fr,en}.toml` sont byte-identiques au tag `pre-merge-aa6a0bec-local-patches` ; les deps de stemming (`rust-stemmers` 1.2.0, `unicode-normalization`) sont inchangees ; `handoffs/atoms.rs` n'a qu'un diff cosmetique (bannieres de commentaires raccourcies par upstream) plus un fix securite upstream (`encode_untrusted_content`) dans le RENDU de sortie, pas dans `extract_heuristic`. Le code teste est donc identique au pre-merge -> ces tests etaient deja rouges avant.

**Non bloquant :** ces tests sont `#[cfg(test)]`, ils n'affectent pas le binaire release. Le merge a ete commite malgre eux (decision operateur 2026-06-01).

### Liste

| Test | Fichier | Symptome |
|---|---|---|
| `lexicon::tests::fold_stems_french_conjugations` | `kleos-lib/src/lexicon/mod.rs:507` | `fold_for_matching("aimer","fr",true)` et les formes conjuguees (aimee/aimees/aimait) ne foldent PAS au meme stem. Observe : `left: "aim"`, `right: "aime"`. Le stemmer Snowball FR ne reduit pas uniformement. |
| `lexicon::tests::embedded_fr_loads_verb_like` | `kleos-lib/src/lexicon/mod.rs:437` | `assertion failed: words.contains(&"apprecier")` -- le mot attendu est absent de la classe verb_like chargee depuis `fr.toml`. |
| `handoffs::atoms::tests::heuristic_extracts_task` | `kleos-lib/src/handoffs/atoms.rs:762` | `extract_heuristic` retourne 0 atome de type task ("should find at least one task"). |
| `handoffs::atoms::tests::heuristic_extracts_decision` | `kleos-lib/src/handoffs/atoms.rs` | idem, 0 atome decision. |
| `handoffs::atoms::tests::heuristic_extracts_constraint` | `kleos-lib/src/handoffs/atoms.rs` | idem, 0 atome constraint. |

### Pistes de fix (a confirmer en session dediee)

- **lexicon fold/stem** : verifier la logique `fold_for_matching` cote VOCSAP Patch 38 -- soit le test attend un fold custom (pre-stem) que le code n'applique pas, soit le stemmer Snowball est applique alors que le test attendait une normalisation sans stem agressif. Aligner test et implementation.
- **atoms heuristic** : `extract_heuristic` s'appuie sur les classes lexicon `atom_<kind>_markers` (Patch 38 L2.B). Si ces classes ne sont pas peuplees dans `fr.toml`/`en.toml` (ou si le test fournit un input qui ne matche aucun marker), l'extraction retourne 0. Verifier que les classes `atom_task_markers` / `atom_decision_markers` / `atom_constraint_markers` existent et matchent l'input des tests.

### References

- Kleos memoire #7739 (constat pre-existant).
- Patch 38 i18n : voir `docs/dev-notes/local-patches.md` et `CLAUDE.md` projet section lexicons.
