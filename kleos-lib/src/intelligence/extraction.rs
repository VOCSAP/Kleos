//! Fact extraction -- regex-based extraction of structured facts, preferences, and state.
//!
//! Ported from intelligence/extraction.ts. Pure regex, no LLM needed.

use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, OnceLock};

use crate::db::Database;
use crate::intelligence::types::ExtractionStats;
use crate::{EngError, Result};
use regex::Regex;
use tracing::{debug, warn};

/// Build a per-language regex from a template by interpolating
/// `lexicon::word_class_alternation` for each placeholder.
///
/// Patch 38 L2.B helper. Used by the like/dislike/favorite/location/role
/// patterns whose verbs vary across languages. The remaining patterns
/// (buy / spent / have / exercise / made / earned) keep their English
/// surface form because they encode unit-specific syntax (currency `$`,
/// quantity prefix, time units) that does not port symmetrically to
/// French and is left as future work.
fn compile_lang_regex(pattern: &str) -> Option<Regex> {
    Regex::new(pattern).ok()
}

fn like_regex_for(lang: &str) -> Option<Regex> {
    // Patch 38 L2.B wildcard-after-stem: TOML lists infinitives
    // (`aimer`, `adorer`) but the source is raw user text with
    // conjugated forms. Stem the alternation and add `\w*` so the
    // root matches every inflection (`aime`, `aimait`, `aimerions`).
    // The capture group is preserved (cap[1] = verb, cap[2] = object).
    let verbs = crate::lexicon::word_class_alternation_stemmed(lang, "verb_like");
    if verbs.is_empty() {
        return None;
    }
    let pronouns = crate::lexicon::word_class_alternation_stemmed(lang, "first_person_pronoun");
    let pronoun_clause = if pronouns.is_empty() {
        String::new()
    } else {
        format!(r"(?:(?:{pronouns})\w*\s+)")
    };
    // Patch 38 L2.B fix: wrap the alternation in `(?:...)` BEFORE
    // applying the `\w*` wildcard. Without the inner group, regex
    // priority makes `aim|ador|appreci|prefer|kiff\w*` parse as
    // `(aim) OR (ador) OR ... OR (kiff\w*)` and only the last
    // alternative gets the suffix. With `(?:...)\w*` the wildcard
    // applies to every alternative.
    let pattern = format!(r"(?i)\b{pronoun_clause}?((?:{verbs})\w*)\s+(.+?)(?:\.|,|$)");
    compile_lang_regex(&pattern)
}

fn dislike_regex_for(lang: &str) -> Option<Regex> {
    let verbs = crate::lexicon::word_class_alternation_stemmed(lang, "verb_dislike");
    if verbs.is_empty() {
        return None;
    }
    let pronouns = crate::lexicon::word_class_alternation_stemmed(lang, "first_person_pronoun");
    let pronoun_clause = if pronouns.is_empty() {
        String::new()
    } else {
        format!(r"(?:(?:{pronouns})\w*\s+)")
    };
    let pattern = format!(r"(?i)\b{pronoun_clause}?((?:{verbs})\w*)\s+(.+?)(?:\.|,|$)");
    compile_lang_regex(&pattern)
}

fn favorite_regex_for(lang: &str) -> Option<Regex> {
    let markers = crate::lexicon::word_class_alternation_stemmed(lang, "favorite_marker");
    let categories = crate::lexicon::word_class_alternation_stemmed(lang, "favorite_category");
    let copula = crate::lexicon::word_class_alternation_stemmed(lang, "is_or_are");
    if markers.is_empty() || categories.is_empty() || copula.is_empty() {
        return None;
    }
    // English form: "my favorite food is X" (marker before category).
    // French form: "mon plat préféré est X" (marker after category).
    // The template accepts either order so the same regex covers both
    // languages. Marker groups stay non-capturing so the caller still
    // reads cap[1] = category, cap[2] = value (signature preserved).
    // Patch 38 L2.B fix: each alternation is wrapped in `(?:...)` so
    // the `\w*` wildcard applies to every alternative (regex priority
    // would otherwise attach the wildcard only to the last word).
    let pattern = format!(
        r"(?i)\b(?:my|mon|ma)\s+(?:(?:{markers})\w*\s+)?((?:{categories})\w*)\s+(?:(?:{markers})\w*\s+)?(?:(?:{copula})\w*)\s+(.+?)(?:\.|,|$)"
    );
    compile_lang_regex(&pattern)
}

fn location_regex_for(lang: &str) -> Option<Regex> {
    let verbs = crate::lexicon::word_class_alternation_stemmed(lang, "location_verbs");
    if verbs.is_empty() {
        return None;
    }
    let pronouns = crate::lexicon::word_class_alternation_stemmed(lang, "first_person_pronoun");
    let pronoun_clause = if pronouns.is_empty() {
        String::new()
    } else {
        format!(r"(?:{pronouns})\w*\s+")
    };
    let pattern = format!(r"(?i)\b(?:{pronoun_clause})?(?:{verbs})\w*\s+(.+?)(?:\.|,|$)");
    compile_lang_regex(&pattern)
}

fn role_regex_for(lang: &str) -> Option<Regex> {
    let verbs = crate::lexicon::word_class_alternation_stemmed(lang, "role_verbs");
    if verbs.is_empty() {
        return None;
    }
    let pronouns = crate::lexicon::word_class_alternation_stemmed(lang, "first_person_pronoun");
    let pronoun_clause = if pronouns.is_empty() {
        String::new()
    } else {
        format!(r"(?:{pronouns})\w*\s+")
    };
    let pattern =
        format!(r"(?i)\b(?:{pronoun_clause})?(?:{verbs})\w*\s+(?:a\s+|an\s+|my\s+|un\s+|une\s+)?(.+?)(?:\.|,|$)");
    compile_lang_regex(&pattern)
}

/// Cache of compiled per-language regexes for the 5 i18n-portable patterns,
/// plus the cross-language copula set used by the Patch 38.1 collision skip.
/// Compiled once on first access from the current state of the lexicon.
struct LangRegexCache {
    like: HashMap<String, Regex>,
    dislike: HashMap<String, Regex>,
    favorite: HashMap<String, Regex>,
    location: HashMap<String, Regex>,
    role: HashMap<String, Regex>,
    /// Patch 38.1 (v2 -- cross-lang): union of copula tokens (`is_or_are`
    /// class) folded across ALL supported languages, with stem=false
    /// projection so the runtime check can fold its candidate token the
    /// same way regardless of which lang's pattern produced the match.
    ///
    /// Rationale: a `verb_like` stem (`prefer` from EN `prefer` or FR
    /// `preferer`) often matches across languages thanks to `\w*`. The
    /// match's lang is therefore not a reliable hint for which copula
    /// vocabulary to consult. The union set absorbs this: if the first
    /// token of the captured object is a copula in ANY supported lang,
    /// the match is suspect regardless of which lang's regex produced it.
    ///
    /// False-positive risk is minimal: copules are short, distinctive
    /// grammar words (`is`/`est`/`ist`/`es`...) and rarely appear as
    /// leading tokens of legitimate LIKE/DISLIKE objects.
    all_copulas: HashSet<String>,
}

static LANG_REGEX: LazyLock<LangRegexCache> = LazyLock::new(|| {
    let mut like = HashMap::new();
    let mut dislike = HashMap::new();
    let mut favorite = HashMap::new();
    let mut location = HashMap::new();
    let mut role = HashMap::new();
    let mut all_copulas: HashSet<String> = HashSet::new();
    for lang in crate::lexicon::supported_languages() {
        if let Some(re) = like_regex_for(&lang) {
            like.insert(lang.clone(), re);
        }
        if let Some(re) = dislike_regex_for(&lang) {
            dislike.insert(lang.clone(), re);
        }
        if let Some(re) = favorite_regex_for(&lang) {
            favorite.insert(lang.clone(), re);
        }
        if let Some(re) = location_regex_for(&lang) {
            location.insert(lang.clone(), re);
        }
        if let Some(re) = role_regex_for(&lang) {
            role.insert(lang.clone(), re);
        }
        // Patch 38.1 v2: merge every lang's copula set into the global
        // union. Fold each word with the class's stem policy of THIS
        // lang (typically stem=false for the grammar class), so the
        // stored form matches what the runtime check produces.
        let copula_words = crate::lexicon::word_class(&lang, "is_or_are");
        if !copula_words.is_empty() {
            let with_stem = crate::lexicon::class_stem_enabled(&lang, "is_or_are");
            for w in copula_words {
                all_copulas.insert(crate::lexicon::fold_for_matching(&w, &lang, with_stem));
            }
        }
    }
    LangRegexCache {
        like,
        dislike,
        favorite,
        location,
        role,
        all_copulas,
    }
});

/// Patch 38.1 (v2): returns true when a LIKE / DISLIKE capture is suspect
/// because its object starts with a copula in any supported language.
/// Cross-lang on purpose: `verb_like` stems often match cross-lang via
/// `\w*` (EN `prefer` matches FR `prefere`), so the match's lang does
/// not reliably indicate which copula vocabulary to consult. We fold the
/// candidate token under each supported lang and check membership in the
/// global union -- a hit in any lang means skip.
///
/// Languages without an `is_or_are` class contribute nothing to the
/// union, so they cannot trigger a false-positive skip.
fn object_starts_with_copula(object: &str) -> bool {
    let Some(first_token) = object.split_whitespace().next() else {
        return false;
    };
    // Try every supported lang's fold projection. The union set was
    // built from per-lang folds, so the candidate must be folded the
    // same way to compare apples to apples.
    for lang in crate::lexicon::supported_languages() {
        let with_stem = crate::lexicon::class_stem_enabled(&lang, "is_or_are");
        let folded = crate::lexicon::fold_for_matching(first_token, &lang, with_stem);
        if LANG_REGEX.all_copulas.contains(&folded) {
            return true;
        }
    }
    false
}

/// Retained for symmetry with other modules. Extraction writes errors are
/// logged inline via `warn!` and do not propagate, so `?` + this helper is
/// not used on the hot path. Kept so new write paths have a consistent
/// conversion available without redefining it.
#[allow(dead_code)]
fn rusqlite_to_eng_error(err: rusqlite::Error) -> EngError {
    EngError::DatabaseMessage(err.to_string())
}

// Static regex patterns compiled once via OnceLock (DOS-H4 fix).
fn buy_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(bought|purchased|got|acquired|received|ordered|picked up)\s+(\d+)\s+(.+?)(?:\.|,|$)").unwrap()
    })
}

fn spent_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\bspent\s+\$([\d,.]+)\s+(?:on|for)\s+(.+?)(?:\.|,|$)").unwrap()
    })
}

fn have_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:I\s+)?(?:have|has|own|got)\s+(\d+)\s+(.+?)(?:\.|,|\s+(?:and|but|so|now))",
        )
        .unwrap()
    })
}

fn exercise_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(ran|jogged|walked|hiked|swam|cycled|biked|exercised)\s+(?:for\s+)?(\d+(?:\.\d+)?)\s+(hours?|minutes?|mins?|miles?|km)").unwrap()
    })
}

fn made_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(made|baked|cooked|prepared)\s+(?:a\s+|some\s+)?(.+?)(?:\.|,|\s+(?:and|but|for|from|yesterday|today|last))").unwrap()
    })
}

fn earned_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(earned|made|received|got)\s+\$([\d,.]+)(?:\s+(?:from|for|in)\s+(.+?))?(?:\.|,|$)").unwrap()
    })
}

// Patch 38 L2.B -- the prior English-only static like_regex(),
// dislike_regex(), favorite_regex(), location_regex() and role_regex()
// functions are superseded by the per-language helpers above
// (like_regex_for, dislike_regex_for, etc.) and the LANG_REGEX cache.
//
// The other 7 patterns (buy, spent, have, exercise, made, earned) keep
// their English surface form below because they encode unit-specific
// syntax (currency `$`, numeric quantity prefix, time/distance units)
// that does not translate symmetrically to French. They are tracked as
// future work in docs/dev-notes/i18n-audit.md.

// Collected operations to execute in a single transaction
struct FactInsert {
    subject: String,
    verb: String,
    object: String,
}

struct PrefUpsert {
    key: String,
    value: String,
}

struct StateUpsert {
    key: String,
    value: String,
}

/// Extract structured facts, preferences, and state updates from memory content.
#[tracing::instrument(skip(db, content), fields(content_len = content.len()))]
pub async fn fast_extract_facts(
    db: &Database,
    content: &str,
    memory_id: i64,
    user_id: i64,
    episode_id: Option<i64>,
) -> Result<ExtractionStats> {
    // Collect all operations first
    let mut facts: Vec<FactInsert> = Vec::new();
    let mut prefs: Vec<PrefUpsert> = Vec::new();
    let mut states: Vec<StateUpsert> = Vec::new();

    // Extract date context from content
    let _date_approx = extract_date_approx(content);
    let date_ref = extract_date_ref(content);

    // -- Pattern 1: bought/purchased N items --
    for cap in buy_regex().captures_iter(content) {
        let verb = cap[1].to_lowercase();
        let quantity: i64 = cap[2].parse().unwrap_or(0);
        let object = cap[3].trim();
        if object.len() > 200 {
            continue;
        }
        facts.push(FactInsert {
            subject: "user".to_string(),
            verb,
            object: format_fact_object(object, Some(quantity), None, date_ref.as_deref()),
        });
    }

    // -- Pattern 2: spent $N on X --
    for cap in spent_regex().captures_iter(content) {
        let amount: f64 = cap[1].replace(',', "").parse().unwrap_or(0.0);
        let object = cap[2].trim();
        facts.push(FactInsert {
            subject: "user".to_string(),
            verb: "spent".to_string(),
            object: format_fact_object(
                object,
                Some(amount as i64),
                Some("dollars"),
                date_ref.as_deref(),
            ),
        });
    }

    // -- Pattern 3: have/own N X --
    for cap in have_regex().captures_iter(content) {
        let quantity: i64 = cap[1].parse().unwrap_or(0);
        let object = cap[2].trim();
        facts.push(FactInsert {
            subject: "user".to_string(),
            verb: "has".to_string(),
            object: format_fact_object(object, Some(quantity), None, date_ref.as_deref()),
        });
    }

    // -- Pattern 4: exercised for N time --
    for cap in exercise_regex().captures_iter(content) {
        let verb = cap[1].to_lowercase();
        let quantity: f64 = cap[2].parse().unwrap_or(0.0);
        let unit = cap[3].to_lowercase();
        facts.push(FactInsert {
            subject: "user".to_string(),
            verb,
            object: format_fact_object("", Some(quantity as i64), Some(&unit), date_ref.as_deref()),
        });
    }

    // -- Pattern 5: made/baked/cooked X --
    for cap in made_regex().captures_iter(content) {
        let verb = cap[1].to_lowercase();
        let object = cap[2].trim();
        facts.push(FactInsert {
            subject: "user".to_string(),
            verb,
            object: format_fact_object(object, Some(1), None, date_ref.as_deref()),
        });
    }

    // -- Pattern 6: earned/made $N --
    for cap in earned_regex().captures_iter(content) {
        let amount: f64 = cap[1].replace(',', "").parse().unwrap_or(0.0);
        let object = cap.get(2).map(|m| m.as_str().trim()).unwrap_or("");
        facts.push(FactInsert {
            subject: "user".to_string(),
            verb: "earned".to_string(),
            object: format_fact_object(
                object,
                Some(amount as i64),
                Some("dollars"),
                date_ref.as_deref(),
            ),
        });
    }

    // -- Preferences: likes/enjoys --
    // Patch 38 L2.B -- the 5 i18n-portable patterns (like, dislike,
    // favorite, location, role) iterate over every supported language
    // and apply that language's compiled regex. A small HashSet
    // dedup-guards against the same pref / state appearing twice when
    // a bilingual sentence matches both languages' patterns.
    let mut seen_prefs: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut seen_states: std::collections::HashSet<String> = std::collections::HashSet::new();

    for lang in crate::lexicon::supported_languages() {
        // -- Preferences: likes --
        if let Some(re) = LANG_REGEX.like.get(&lang) {
            for cap in re.captures_iter(content) {
                let object = cap[2].trim();
                // Patch 38.1: skip captures whose object starts with a
                // copula -- strong signal the source is actually a
                // FAVORITE structure ("mon plat prefere est X") that
                // the verb stem overlap mis-classified as LIKE.
                if object_starts_with_copula(object) {
                    continue;
                }
                if object.len() > 3 && object.len() < 100 {
                    let domain = infer_domain(object);
                    let key = format!("{domain}:likes {object}");
                    if seen_prefs.insert(key.clone()) {
                        prefs.push(PrefUpsert {
                            key,
                            value: format!("evidence_memory_id:{memory_id}"),
                        });
                    }
                }
            }
        }

        // -- Preferences: dislikes --
        if let Some(re) = LANG_REGEX.dislike.get(&lang) {
            for cap in re.captures_iter(content) {
                let object = cap[2].trim();
                // Patch 38.1: same cross-pattern collision skip as LIKE.
                if object_starts_with_copula(object) {
                    continue;
                }
                if object.len() > 3 && object.len() < 100 {
                    let domain = infer_domain(object);
                    let key = format!("{domain}:dislikes {object}");
                    if seen_prefs.insert(key.clone()) {
                        prefs.push(PrefUpsert {
                            key,
                            value: format!("evidence_memory_id:{memory_id}"),
                        });
                    }
                }
            }
        }

        // -- Preferences: favorites --
        if let Some(re) = LANG_REGEX.favorite.get(&lang) {
            for cap in re.captures_iter(content) {
                let category = cap[1].trim().to_lowercase();
                let value = cap[2].trim();
                let key = format!("{category}:favorite: {value}");
                if seen_prefs.insert(key.clone()) {
                    prefs.push(PrefUpsert {
                        key,
                        value: format!("evidence_memory_id:{memory_id}"),
                    });
                }
            }
        }

        // -- State updates: location changes --
        if let Some(re) = LANG_REGEX.location.get(&lang) {
            for cap in re.captures_iter(content) {
                let location = cap[1].trim();
                let key = format!("current_location|{location}");
                if seen_states.insert(key) {
                    states.push(StateUpsert {
                        key: "current_location".to_string(),
                        value: format!("{location} (memory:{memory_id})"),
                    });
                }
            }
        }

        // -- State updates: role changes --
        if let Some(re) = LANG_REGEX.role.get(&lang) {
            for cap in re.captures_iter(content) {
                let role = cap[1].trim();
                if role.len() > 3 && role.len() < 100 {
                    let key = format!("current_role|{role}");
                    if seen_states.insert(key) {
                        states.push(StateUpsert {
                            key: "current_role".to_string(),
                            value: format!("{role} (memory:{memory_id})"),
                        });
                    }
                }
            }
        }
    }

    // Execute all operations in a single write
    let fact_count = facts.len();
    let pref_count = prefs.len();
    let state_count = states.len();

    if fact_count + pref_count + state_count > 0 {
        db.write(move |conn| {
            // Insert facts
            for fact in &facts {
                if let Err(e) = conn.execute(
                    "INSERT INTO structured_facts (memory_id, subject, predicate, object, confidence) \
                     VALUES (?1, ?2, ?3, ?4, 1.0)",
                    rusqlite::params![memory_id, fact.subject, fact.verb, fact.object],
                ) {
                    warn!(error = %e, "fact_insert_failed");
                }
            }

            // Upsert preferences
            for pref in &prefs {
                if let Err(e) = conn.execute(
                    "INSERT INTO user_preferences (key, value, created_at, updated_at) \
                     VALUES (?1, ?2, datetime('now'), datetime('now')) \
                     ON CONFLICT(key) DO UPDATE SET \
                       value = excluded.value, \
                       updated_at = datetime('now')",
                    rusqlite::params![pref.key, pref.value],
                ) {
                    warn!(error = %e, "preference_upsert_failed");
                }
            }

            // Upsert state
            for state in &states {
                if let Err(e) = conn.execute(
                    "INSERT INTO current_state (agent, key, value, created_at, updated_at) \
                     VALUES ('system', ?1, ?2, datetime('now'), datetime('now')) \
                     ON CONFLICT(agent, key) DO UPDATE SET \
                       value = excluded.value, \
                       updated_at = datetime('now')",
                    rusqlite::params![state.key, state.value],
                ) {
                    warn!(error = %e, "state_upsert_failed");
                }
            }

            // Stamp episode provenance on newly extracted facts
            if !facts.is_empty() {
                if let Some(ep_id) = episode_id {
                    if let Err(e) = conn.execute(
                        "UPDATE structured_facts SET episode_id = ?1
                         WHERE memory_id = ?2 AND episode_id IS NULL",
                        rusqlite::params![ep_id, memory_id],
                    ) {
                        warn!(memory_id, user_id, error = %e, "extraction: failed to stamp episode_id on structured_facts");
                    }
                }
            }

            Ok(())
        })
        .await?;
    }

    let stats = ExtractionStats {
        facts: fact_count as i32,
        preferences: pref_count as i32,
        state_updates: state_count as i32,
    };

    if stats.facts + stats.preferences + stats.state_updates > 0 {
        debug!(
            memory_id,
            facts = stats.facts,
            prefs = stats.preferences,
            state = stats.state_updates,
            "fast_extract"
        );
    }

    Ok(stats)
}

fn format_fact_object(
    object: &str,
    quantity: Option<i64>,
    unit: Option<&str>,
    date_ref: Option<&str>,
) -> String {
    format!(
        "{}{}{}{}",
        object,
        quantity
            .map(|q| format!(" [qty:{}]", q))
            .unwrap_or_default(),
        unit.map(|u| format!(" [unit:{}]", u)).unwrap_or_default(),
        date_ref
            .map(|d| format!(" [date:{}]", d))
            .unwrap_or_default(),
    )
}

fn extract_date_approx(content: &str) -> Option<String> {
    let re = Regex::new(r"\[Conversation date:\s*([\d/]+)\]").ok()?;
    re.captures(content).map(|c| c[1].to_string())
}

fn extract_date_ref(content: &str) -> Option<String> {
    // Look for explicit date references like "yesterday", "last week", specific dates
    let re = Regex::new(r"(?i)\b(yesterday|today|last\s+(?:week|month|year)|(?:on\s+)?(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2}(?:st|nd|rd|th)?(?:\s*,?\s*\d{4})?)\b").ok()?;
    re.find(content).map(|m| m.as_str().to_string())
}

/// Infer the high-level domain of an object string by scanning the i18n
/// lexicon for every supported language.
///
/// Patch 38 L2 site 11 + normalize -- compare folded versions of both
/// sides so an object like "petit-déjeuner" matches the lexicon entry
/// "petit-déjeuner" (or its bare ASCII equivalent if the user dropped
/// the accent and the hyphen). DOMAINS preserves the original priority
/// (food > entertainment > reading > music > gaming > fitness > travel)
/// so overlapping matches resolve deterministically.
fn infer_domain(object: &str) -> String {
    const DOMAINS: &[(&str, &str)] = &[
        ("domain_food", "food"),
        ("domain_entertainment", "entertainment"),
        ("domain_reading", "reading"),
        ("domain_music", "music"),
        ("domain_gaming", "gaming"),
        ("domain_fitness", "fitness"),
        ("domain_travel", "travel"),
    ];
    for (class, label) in DOMAINS {
        for lang in crate::lexicon::supported_languages() {
            let folded_obj = crate::lexicon::fold_for_matching(object, &lang, true);
            let words = crate::lexicon::word_class(&lang, class);
            if words
                .iter()
                .any(|w| folded_obj.contains(&crate::lexicon::fold_word_for_class(w, &lang, class)))
            {
                return (*label).to_string();
            }
        }
    }
    "general".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buy_regex_captures() {
        let content = "I bought 3 apples yesterday.";
        let caps: Vec<_> = buy_regex().captures_iter(content).collect();
        assert_eq!(caps.len(), 1);
        assert_eq!(&caps[0][1], "bought");
        assert_eq!(&caps[0][2], "3");
        assert_eq!(&caps[0][3], "apples yesterday");
    }

    #[test]
    fn test_spent_regex_captures() {
        let content = "I spent $50.00 on groceries.";
        let caps: Vec<_> = spent_regex().captures_iter(content).collect();
        assert_eq!(caps.len(), 1);
        assert_eq!(&caps[0][1], "50.00");
        assert_eq!(&caps[0][2], "groceries");
    }

    #[test]
    fn test_infer_domain() {
        assert_eq!(infer_domain("pizza"), "food");
        assert_eq!(infer_domain("movie night"), "entertainment");
        assert_eq!(infer_domain("running shoes"), "fitness");
        assert_eq!(infer_domain("random thing"), "general");
    }

    #[test]
    fn test_format_fact_object() {
        assert_eq!(
            format_fact_object("apples", Some(3), None, None),
            "apples [qty:3]"
        );
        assert_eq!(
            format_fact_object("", Some(5), Some("miles"), Some("yesterday")),
            " [qty:5] [unit:miles] [date:yesterday]"
        );
    }
}
