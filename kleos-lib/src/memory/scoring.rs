use crate::memory::types::{QuestionType, SearchStrategy};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::LazyLock;

pub use crate::validation::RERANKER_TOP_K;
pub const SEARCH_FACT_VECTOR_FLOOR: f64 = 0.22;
pub const SEARCH_PREFERENCE_VECTOR_FLOOR: f64 = 0.12;
pub const SEARCH_REASONING_VECTOR_FLOOR: f64 = 0.10;
pub const SEARCH_GENERALIZATION_VECTOR_FLOOR: f64 = 0.12;
pub const SEARCH_PERSONALITY_MIN_SCORE: f64 = 0.30;
/// Lower bound on the FSRS decay multiplier applied per candidate inside
/// `hybrid_search`. At retrievability=0 the multiplier is `DECAY_FLOOR`;
/// at retrievability=1 it is 1.0. Lowered from 0.3 (R8 P-021) so the
/// dynamic range now spans an order of magnitude per memory rather than
/// a third, which keeps relevant-but-old memories rankable while still
/// letting fresh memories outscore them.
const DEFAULT_DECAY_FLOOR: f64 = 0.1;
static DECAY_FLOOR_OVERRIDE: LazyLock<f64> = LazyLock::new(|| {
    std::env::var("KLEOS_DECAY_FLOOR")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_DECAY_FLOOR)
});
pub fn decay_floor() -> f64 {
    *DECAY_FLOOR_OVERRIDE
}
pub const PAGERANK_WEIGHT: f64 = 0.15;
pub const DEFAULT_VECTOR_FLOOR: f64 = 0.15;
pub const RRF_K: f64 = 60.0;
pub const RECENCY_WEIGHT: f64 = 0.15;

pub fn question_strategy(qt: QuestionType) -> SearchStrategy {
    match qt {
        QuestionType::FactRecall => SearchStrategy {
            vector_floor: SEARCH_FACT_VECTOR_FLOOR,
            vector_weight: 0.62,
            fts_weight: 0.32,
            candidate_multiplier: 2,
            fts_limit_multiplier: 2,
            expand_relationships: false,
            relationship_seed_limit: 3,
            hop1_limit: 4,
            hop2_limit: 0,
            relationship_multiplier: 0.75,
            include_personality_signals: false,
            personality_limit: 0,
            personality_weight: 0.0,
        },
        QuestionType::Preference => SearchStrategy {
            vector_floor: SEARCH_PREFERENCE_VECTOR_FLOOR,
            vector_weight: 0.52,
            fts_weight: 0.30,
            candidate_multiplier: 3,
            fts_limit_multiplier: 4,
            expand_relationships: true,
            relationship_seed_limit: 5,
            hop1_limit: 6,
            hop2_limit: 2,
            relationship_multiplier: 1.0,
            include_personality_signals: true,
            personality_limit: 24,
            personality_weight: 0.22,
        },
        QuestionType::Reasoning => SearchStrategy {
            vector_floor: SEARCH_REASONING_VECTOR_FLOOR,
            vector_weight: 0.5,
            fts_weight: 0.26,
            candidate_multiplier: 4,
            fts_limit_multiplier: 5,
            expand_relationships: true,
            relationship_seed_limit: 5,
            hop1_limit: 8,
            hop2_limit: 2,
            relationship_multiplier: 1.2,
            include_personality_signals: true,
            personality_limit: 30,
            personality_weight: 0.14,
        },
        QuestionType::Generalization => SearchStrategy {
            vector_floor: SEARCH_GENERALIZATION_VECTOR_FLOOR,
            vector_weight: 0.48,
            fts_weight: 0.24,
            candidate_multiplier: 4,
            fts_limit_multiplier: 5,
            expand_relationships: true,
            relationship_seed_limit: 6,
            hop1_limit: 8,
            hop2_limit: 2,
            relationship_multiplier: 1.2,
            include_personality_signals: true,
            personality_limit: 36,
            personality_weight: 0.24,
        },
        QuestionType::Temporal => SearchStrategy {
            vector_floor: 0.10,
            vector_weight: 0.35,
            fts_weight: 0.35,
            candidate_multiplier: 4,
            fts_limit_multiplier: 5,
            expand_relationships: true,
            relationship_seed_limit: 5,
            hop1_limit: 8,
            hop2_limit: 2,
            relationship_multiplier: 1.2,
            include_personality_signals: false,
            personality_limit: 0,
            personality_weight: 0.0,
        },
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

pub fn classify_question_mixed(query: &str) -> HashMap<QuestionType, f64> {
    let q = query.to_lowercase();
    let mut scores: HashMap<QuestionType, f64> = HashMap::new();
    if contains_any(&q, &["when did", "when was", "timeline", "history of"]) {
        *scores.entry(QuestionType::Temporal).or_default() += 0.6;
    }
    if contains_any(&q, &["over the past", "how long ago", "since when"]) {
        *scores.entry(QuestionType::Temporal).or_default() += 0.4;
    }
    if contains_any(
        &q,
        &[
            "used to",
            "originally",
            "evolution of",
            "over time",
            "progression",
        ],
    ) {
        *scores.entry(QuestionType::Temporal).or_default() += 0.5;
    }
    if extract_query_date(query).is_some()
        && contains_any(&q, &["what", "who", "how", "which", "did"])
    {
        *scores.entry(QuestionType::Temporal).or_default() += 0.3;
    }
    if contains_any(
        &q,
        &[
            "recently",
            "attended",
            "joined",
            "last time",
            "went to",
            "visited",
        ],
    ) {
        *scores.entry(QuestionType::FactRecall).or_default() += 0.5;
    }
    if contains_any(
        &q,
        &[
            "what is my",
            "what are my",
            "tell me about",
            "do i have",
            "do i own",
        ],
    ) {
        *scores.entry(QuestionType::FactRecall).or_default() += 0.5;
    }
    if contains_any(
        &q,
        &["what did i", "where do", "where did", "who is", "who was"],
    ) {
        *scores.entry(QuestionType::FactRecall).or_default() += 0.4;
    }
    if contains_any(
        &q,
        &[
            "why did",
            "what made",
            "decided",
            "reason",
            "because",
            "why do",
        ],
    ) {
        *scores.entry(QuestionType::Reasoning).or_default() += 0.6;
    }
    if contains_any(&q, &["motivation", "what led", "tradeoff", "trade-off"]) {
        *scores.entry(QuestionType::Reasoning).or_default() += 0.4;
    }
    if contains_any(
        &q,
        &[
            "should i",
            "do you think",
            "considering",
            "would i",
            "good fit",
        ],
    ) {
        *scores.entry(QuestionType::Generalization).or_default() += 0.6;
    }
    if contains_any(&q, &["does it make sense", "aligned with"]) {
        *scores.entry(QuestionType::Generalization).or_default() += 0.4;
    }
    if contains_any(
        &q,
        &["suggest", "recommend", "what would", "ideas", "weekend"],
    ) {
        *scores.entry(QuestionType::Preference).or_default() += 0.5;
    }
    if contains_any(
        &q,
        &[
            "favorite",
            "prefer",
            "like most",
            "enjoy",
            "love",
            "hate",
            "dislike",
            "interested in",
            "passionate about",
        ],
    ) {
        *scores.entry(QuestionType::Preference).or_default() += 0.6;
    }
    if contains_any(
        &q,
        &["what kind of", "what type of", "taste in", "style of"],
    ) {
        *scores.entry(QuestionType::Preference).or_default() += 0.4;
    }

    let total: f64 = scores.values().sum();
    if total == 0.0 {
        let mut m = HashMap::new();
        m.insert(QuestionType::FactRecall, 1.0);
        return m;
    }
    for v in scores.values_mut() {
        *v = (*v / total * 100.0).round() / 100.0;
    }
    scores
}

pub fn blend_strategies(weights: &HashMap<QuestionType, f64>) -> SearchStrategy {
    if weights.len() == 1 {
        return question_strategy(*weights.keys().next().unwrap());
    }
    let mut r = SearchStrategy {
        vector_floor: 0.0,
        vector_weight: 0.0,
        fts_weight: 0.0,
        candidate_multiplier: 0,
        fts_limit_multiplier: 0,
        expand_relationships: false,
        relationship_seed_limit: 0,
        hop1_limit: 0,
        hop2_limit: 0,
        relationship_multiplier: 0.0,
        include_personality_signals: false,
        personality_limit: 0,
        personality_weight: 0.0,
    };
    let (mut ew, mut pw) = (0.0_f64, 0.0_f64);
    let (mut cm, mut flm, mut rsl, mut h1, mut h2, mut pl) =
        (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    for (&qt, &w) in weights {
        let s = question_strategy(qt);
        r.vector_floor += s.vector_floor * w;
        r.vector_weight += s.vector_weight * w;
        r.fts_weight += s.fts_weight * w;
        r.relationship_multiplier += s.relationship_multiplier * w;
        r.personality_weight += s.personality_weight * w;
        cm += s.candidate_multiplier as f64 * w;
        flm += s.fts_limit_multiplier as f64 * w;
        rsl += s.relationship_seed_limit as f64 * w;
        h1 += s.hop1_limit as f64 * w;
        h2 += s.hop2_limit as f64 * w;
        pl += s.personality_limit as f64 * w;
        if s.expand_relationships {
            ew += w;
        }
        if s.include_personality_signals {
            pw += w;
        }
    }
    r.expand_relationships = ew > 0.4;
    r.include_personality_signals = pw > 0.3;
    r.candidate_multiplier = cm.round() as usize;
    r.fts_limit_multiplier = flm.round() as usize;
    r.relationship_seed_limit = rsl.round() as usize;
    r.hop1_limit = h1.round() as usize;
    r.hop2_limit = h2.round() as usize;
    r.personality_limit = pl.round() as usize;
    r
}

pub fn extract_query_date(query: &str) -> Option<String> {
    let q = query.to_lowercase();
    // ISO date
    if let Some(pos) = q.find(|c: char| c.is_ascii_digit()) {
        let rest = &q[pos..];
        if rest.len() >= 10 {
            let c = &rest[..10];
            if c.as_bytes()[4] == b'-'
                && c.as_bytes()[7] == b'-'
                && c[..4].chars().all(|x| x.is_ascii_digit())
                && c[5..7].chars().all(|x| x.is_ascii_digit())
                && c[8..10].chars().all(|x| x.is_ascii_digit())
            {
                return Some(c.to_string());
            }
        }
    }
    // Month day
    let months = [
        ("january", "01"),
        ("february", "02"),
        ("march", "03"),
        ("april", "04"),
        ("may", "05"),
        ("june", "06"),
        ("july", "07"),
        ("august", "08"),
        ("september", "09"),
        ("october", "10"),
        ("november", "11"),
        ("december", "12"),
    ];
    for &(name, num) in &months {
        if let Some(mpos) = q.find(name) {
            let after = q[mpos + name.len()..].trim_start();
            let ds: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(day) = ds.parse::<u32>() {
                if (1..=31).contains(&day) {
                    let ad = after[ds.len()..]
                        .trim_start()
                        .trim_start_matches(',')
                        .trim_start();
                    let ys: String = ad.chars().take_while(|c| c.is_ascii_digit()).collect();
                    let yr = if ys.len() == 4 {
                        ys
                    } else {
                        Utc::now().format("%Y").to_string()
                    };
                    return Some(format!("{}-{}-{:02}", yr, num, day));
                }
            }
        }
    }
    // Relative
    let now = Utc::now();
    if q.contains("yesterday") {
        return Some(
            (now - chrono::Duration::days(1))
                .format("%Y-%m-%d")
                .to_string(),
        );
    }
    if q.contains("last week") {
        return Some(
            (now - chrono::Duration::days(7))
                .format("%Y-%m-%d")
                .to_string(),
        );
    }
    if q.contains("last month") {
        return Some(
            (now - chrono::Duration::days(30))
                .format("%Y-%m-%d")
                .to_string(),
        );
    }
    if q.contains("today") {
        return Some(now.format("%Y-%m-%d").to_string());
    }
    // N units ago
    if let Some(ap) = q.find(" ago") {
        let before = &q[..ap];
        let parts: Vec<&str> = before.split_whitespace().collect();
        if parts.len() >= 2 {
            let unit = parts[parts.len() - 1];
            if let Ok(n) = parts[parts.len() - 2].parse::<i64>() {
                let days = if unit.starts_with("day") {
                    n
                } else if unit.starts_with("week") {
                    n * 7
                } else if unit.starts_with("month") {
                    n * 30
                } else {
                    0
                };
                if days > 0 {
                    return Some(
                        (now - chrono::Duration::days(days))
                            .format("%Y-%m-%d")
                            .to_string(),
                    );
                }
            }
        }
    }
    None
}

pub fn rrf_score(rank: usize) -> f64 {
    1.0 / (RRF_K + rank as f64 + 1.0)
}

pub fn temporal_proximity_boost(query_date: &str, memory_created_at: &str) -> f64 {
    match (parse_date_ms(query_date), parse_date_ms(memory_created_at)) {
        (Some(q), Some(m)) => {
            let dd = ((q - m) as f64).abs() / 86_400_000.0;
            1.0 + 0.5 * (-(dd * dd) / (2.0 * 7.0 * 7.0)).exp()
        }
        _ => 1.0,
    }
}

pub fn contradiction_penalty(content: &str, is_latest: bool) -> f64 {
    if is_latest {
        return 1.0;
    }
    let lc = content.to_lowercase();
    if lc.contains("no longer")
        || lc.contains("changed to")
        || lc.contains("used to")
        || lc.contains("instead now")
        || lc.contains("but now")
        || lc.contains("previously")
        || lc.contains("was replaced")
        || lc.contains("switched from")
    {
        0.65
    } else {
        1.0
    }
}

pub fn source_count_boost(source_count: i32, is_consolidated: bool) -> f64 {
    if is_consolidated {
        return 1.0;
    }
    1.0 + (source_count as f64 / 10.0).min(1.0) * 0.05
}

pub fn static_boost(is_static: bool, is_consolidated: bool) -> f64 {
    if is_static && !is_consolidated {
        1.03
    } else {
        1.0
    }
}

pub fn pagerank_boost(pagerank_score: f64) -> f64 {
    1.0 + pagerank_score * PAGERANK_WEIGHT
}

pub fn recency_score(created_at: &str) -> f64 {
    let age_days = match parse_date_ms(created_at) {
        Some(ms) => {
            let now_ms = Utc::now().timestamp_millis();
            ((now_ms - ms) as f64 / 86_400_000.0).max(0.0)
        }
        None => 30.0,
    };
    (-age_days / 30.0_f64).exp()
}

pub fn link_type_weight(link_type: &str) -> f64 {
    match link_type {
        "caused_by" | "causes" => 2.0,
        "updates" | "corrects" => 1.5,
        "extends" | "contradicts" => 1.3,
        _ => 1.0,
    }
}

/// Parse a date string into epoch milliseconds, handling ISO8601 with or
/// without timezone, space-separated datetime, and bare dates.
pub(crate) fn parse_date_ms(s: &str) -> Option<i64> {
    let n = if s.contains('Z') || s.contains('+') {
        s.replace(' ', "T")
    } else if s.contains('T') || s.contains(' ') {
        format!("{}Z", s.replace(' ', "T"))
    } else {
        format!("{}T00:00:00Z", s)
    };
    n.parse::<chrono::DateTime<chrono::Utc>>()
        .ok()
        .map(|dt| dt.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_iso() {
        assert_eq!(
            extract_query_date("on 2026-03-16"),
            Some("2026-03-16".to_string())
        );
    }

    #[test]
    fn extract_relative() {
        assert!(extract_query_date("yesterday").is_some());
        assert!(extract_query_date("last week").is_some());
        assert!(extract_query_date("3 days ago").is_some());
    }

    #[test]
    fn extract_month() {
        let r = extract_query_date("on march 15");
        assert!(r.is_some());
        assert!(r.unwrap().contains("-03-15"));
    }

    #[test]
    fn extract_none() {
        assert_eq!(extract_query_date("what is my name"), None);
    }

    #[test]
    fn temporal_boost_close() {
        let b = temporal_proximity_boost("2026-03-15", "2026-03-15T12:00:00Z");
        assert!(b > 1.4, "got {}", b);
    }

    #[test]
    fn temporal_boost_far() {
        let b = temporal_proximity_boost("2026-03-15", "2025-01-01T00:00:00Z");
        assert!(b < 1.01, "got {}", b);
    }

    #[test]
    fn contradiction_test() {
        assert_eq!(contradiction_penalty("current", true), 1.0);
        assert_eq!(contradiction_penalty("no longer valid", false), 0.65);
    }

    #[test]
    fn link_weight_test() {
        assert_eq!(link_type_weight("caused_by"), 2.0);
        assert_eq!(link_type_weight("updates"), 1.5);
        assert_eq!(link_type_weight("similarity"), 1.0);
    }

    #[test]
    fn blend_single() {
        let mut w = HashMap::new();
        w.insert(QuestionType::FactRecall, 1.0);
        let s = blend_strategies(&w);
        assert!((s.vector_weight - 0.62).abs() < 0.001);
    }

    #[test]
    fn blend_mixed() {
        let mut w = HashMap::new();
        w.insert(QuestionType::FactRecall, 0.5);
        w.insert(QuestionType::Preference, 0.5);
        let s = blend_strategies(&w);
        assert!(
            (s.vector_weight - 0.57).abs() < 0.01,
            "got {}",
            s.vector_weight
        );
    }
}
