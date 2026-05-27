// ============================================================================
// Lexicon -- i18n core module (Patch 38 Livrable 1).
//
// Centralises multilingual word lists and (later) regex overrides used by
// the intelligence pipeline (extraction.rs, personality.rs, valence.rs,
// sentiment.rs, etc.). Livrable 1 is purely additive: nothing in the
// codebase consumes this module yet. Livrable 2 will refactor the 15
// hardcoded sites identified in `docs/dev-notes/i18n-audit.md` to read
// from here instead.
//
// Public API:
//   - word_class(lang, class)             -> Vec<String>
//   - word_class_alternation(lang, class) -> String (joined with '|')
//   - supported_languages()               -> Vec<String>
//   - complex_regex(id)                   -> Option<String> (stub for Layer B,
//                                            populated in Livrable 2)
//
// Cascade for the source of truth, in priority order:
//   1. `KLEOS_LEXICON_REPOSITORY` env var (explicit, any directory).
//   2. `KLEOS_DATA_DIR/lexicon` (or `ENGRAM_DATA_DIR/lexicon`) when present.
//   3. Embedded baselines: `lexicon/en.toml`, `lexicon/fr.toml`.
//
// Format of each `<lang>.toml` file (see also `kleos-lib/lexicon/en.toml`
// and `fr.toml` for the embedded reference):
//
//     schema_version = 1
//     language = "en"
//
//     [classes.verb_like]
//     words = ["love", "like", ...]
//
//     [classes.emotion_happy]
//     words = ["happy", "joyful", ...]
//     valence = 0.7
//     intensity = 0.6
// ============================================================================

mod cache;
mod loader;

use std::sync::OnceLock;

use loader::ParsedLexicon;

/// Embedded EN baseline. Parsed once on first access and panics if the file
/// is malformed (a malformed embedded baseline is a build-time bug that must
/// surface immediately, not a runtime warning).
fn embedded_en() -> &'static ParsedLexicon {
    static EN: OnceLock<ParsedLexicon> = OnceLock::new();
    EN.get_or_init(|| {
        loader::parse(include_str!("../../lexicon/en.toml"))
            .expect("embedded en.toml must parse (build-time guarantee)")
    })
}

/// Embedded FR baseline. Same rules as `embedded_en`.
fn embedded_fr() -> &'static ParsedLexicon {
    static FR: OnceLock<ParsedLexicon> = OnceLock::new();
    FR.get_or_init(|| {
        loader::parse(include_str!("../../lexicon/fr.toml"))
            .expect("embedded fr.toml must parse (build-time guarantee)")
    })
}

/// List of language codes that have an embedded baseline. The override repo
/// can add others on top via files named `<lang>.toml`.
const EMBEDDED_LANGS: &[&str] = &["en", "fr"];

fn embedded(lang: &str) -> Option<&'static ParsedLexicon> {
    match lang {
        "en" => Some(embedded_en()),
        "fr" => Some(embedded_fr()),
        _ => None,
    }
}

/// Look up the words for a given semantic class in a given language.
///
/// Returns an empty `Vec` (not an error) when the language or class is
/// unknown. The empty result is a deliberate API choice: callers iterate
/// `supported_languages()` and concatenate matches across all languages,
/// so silently-empty buckets are the common case and must not raise.
pub fn word_class(lang: &str, class: &str) -> Vec<String> {
    // 1. Override repo wins when available.
    if let Some(repo) = cache::repo_root() {
        if let Some(parsed) = cache::resolve_override(repo, lang) {
            if let Some(class_entry) = parsed.classes.get(class) {
                return class_entry.words.clone();
            }
            // Override file exists but does not define this class: fall back
            // to the embedded baseline if any. This lets operators write thin
            // override files that only patch specific classes without having
            // to redeclare everything.
        }
    }
    // 2. Embedded baseline.
    embedded(lang)
        .and_then(|p| p.classes.get(class))
        .map(|c| c.words.clone())
        .unwrap_or_default()
}

/// Convenience: pipe-joined alternation suitable for direct interpolation
/// into a regex template. The values are emitted verbatim -- callers that
/// need regex-escaping should escape themselves (Livrable 2 may add a
/// helper once the consumer call sites are concrete).
pub fn word_class_alternation(lang: &str, class: &str) -> String {
    word_class(lang, class).join("|")
}

/// Enumerate every language that the lexicon module can serve. Includes the
/// embedded baselines plus any `<lang>.toml` file present in the override
/// repo (if configured). Result is sorted and de-duplicated.
pub fn supported_languages() -> Vec<String> {
    let mut langs: Vec<String> = EMBEDDED_LANGS.iter().map(|s| s.to_string()).collect();

    if let Some(repo) = cache::repo_root() {
        if let Ok(entries) = std::fs::read_dir(repo) {
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|e| e.to_str()) != Some("toml") {
                    continue;
                }
                if let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()) {
                    langs.push(stem.to_string());
                }
            }
        }
    }

    langs.sort();
    langs.dedup();
    langs
}

/// Stub for Layer B (Patch 38 Livrable 2): retrieve a complex regex
/// override by its dot-free id (e.g. `"intelligence.extraction.facts.fr.
/// negation_discontinuous"`). Always returns `None` in Livrable 1; the
/// real implementation reads from `<repo>/patterns/<id>.toml`.
///
/// Exposed now so Livrable 2 consumer code can be written against the
/// final signature without churn.
pub fn complex_regex(_id: &str) -> Option<String> {
    None
}

/// Return the optional `(valence, intensity)` metadata for an emotion class.
/// Used by Livrable 2 personality.rs / valence.rs refactor. Returns `None`
/// when the class has no metadata or the language is unknown.
pub fn class_emotion_metadata(lang: &str, class: &str) -> Option<(f64, f64)> {
    let from_override = cache::repo_root()
        .and_then(|repo| cache::resolve_override(repo, lang))
        .and_then(|p| {
            p.classes.get(class).and_then(|c| match (c.valence, c.intensity) {
                (Some(v), Some(i)) => Some((v, i)),
                _ => None,
            })
        });
    if from_override.is_some() {
        return from_override;
    }
    embedded(lang).and_then(|p| {
        p.classes.get(class).and_then(|c| match (c.valence, c.intensity) {
            (Some(v), Some(i)) => Some((v, i)),
            _ => None,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_en_loads_verb_like() {
        let words = word_class("en", "verb_like");
        assert!(words.contains(&"love".to_string()));
        assert!(words.contains(&"like".to_string()));
        assert!(words.contains(&"enjoy".to_string()));
        assert!(words.contains(&"adore".to_string()));
        assert!(words.contains(&"prefer".to_string()));
    }

    #[test]
    fn embedded_fr_loads_verb_like() {
        let words = word_class("fr", "verb_like");
        assert!(words.contains(&"aimer".to_string()));
        assert!(words.contains(&"adorer".to_string()));
        assert!(words.contains(&"apprecier".to_string()));
        assert!(words.contains(&"preferer".to_string()));
    }

    #[test]
    fn unknown_language_returns_empty() {
        let words = word_class("xx", "verb_like");
        assert!(words.is_empty());
    }

    #[test]
    fn unknown_class_returns_empty() {
        let words = word_class("en", "this_class_does_not_exist");
        assert!(words.is_empty());
    }

    #[test]
    fn supported_languages_includes_embedded_baselines() {
        let langs = supported_languages();
        assert!(langs.contains(&"en".to_string()));
        assert!(langs.contains(&"fr".to_string()));
    }

    #[test]
    fn supported_languages_is_sorted_and_deduplicated() {
        let langs = supported_languages();
        let mut sorted = langs.clone();
        sorted.sort();
        assert_eq!(langs, sorted, "supported_languages must be sorted");
        let mut deduped = langs.clone();
        deduped.dedup();
        assert_eq!(langs, deduped, "supported_languages must be deduplicated");
    }

    #[test]
    fn word_class_alternation_pipes_words() {
        let alt = word_class_alternation("en", "verb_like");
        // Order is whatever the embedded TOML defined; check we see at
        // least the canonical entries separated by '|'.
        assert!(alt.contains("love"));
        assert!(alt.contains("|"));
    }

    #[test]
    fn complex_regex_returns_none_in_livrable_1() {
        assert!(complex_regex("any.id").is_none());
    }

    #[test]
    fn class_emotion_metadata_returns_some_for_emotion_classes() {
        let meta = class_emotion_metadata("en", "emotion_happy");
        assert!(meta.is_some(), "emotion_happy must expose (valence, intensity)");
        let (valence, intensity) = meta.unwrap();
        assert!(valence > 0.0, "happy is positive valence");
        assert!(intensity > 0.0 && intensity <= 1.0);
    }

    #[test]
    fn class_emotion_metadata_returns_none_for_layer_a_classes() {
        // articles / stopwords have no valence/intensity, so metadata must
        // be None even though the class exists.
        assert!(class_emotion_metadata("en", "articles").is_none());
        assert!(class_emotion_metadata("en", "stopwords").is_none());
    }

    #[test]
    fn embedded_en_has_full_class_set() {
        // Smoke test that the embedded EN baseline declares every class the
        // Livrable 2 refactor will need. Any class missing here will cause
        // an empty result downstream, so we check upfront.
        for class in [
            "verb_like",
            "verb_dislike",
            "verb_buy",
            "state_verbs",
            "articles",
            "stopwords",
            "first_person_pronoun",
            "emotion_happy",
            "emotion_sad",
            "intensifier_strong",
            "negation_marker",
            "causal_strong",
            "causal_context",
            "causal_weak",
            "filler_prefixes",
            "meta_stoplist",
            "credential_keywords",
            "prohibition_marker",
        ] {
            assert!(
                !word_class("en", class).is_empty(),
                "embedded en.toml is missing class {class}",
            );
        }
    }

    #[test]
    fn embedded_fr_has_full_class_set() {
        // Same coverage check for the FR baseline.
        for class in [
            "verb_like",
            "verb_dislike",
            "verb_buy",
            "state_verbs",
            "articles",
            "stopwords",
            "first_person_pronoun",
            "emotion_happy",
            "emotion_sad",
            "intensifier_strong",
            "negation_marker",
            "causal_strong",
            "causal_context",
            "causal_weak",
            "filler_prefixes",
            "meta_stoplist",
            "credential_keywords",
            "prohibition_marker",
        ] {
            assert!(
                !word_class("fr", class).is_empty(),
                "embedded fr.toml is missing class {class}",
            );
        }
    }
}
