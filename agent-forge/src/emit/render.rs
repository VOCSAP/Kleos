//! Markdown renderers. These are pure string functions so the emitted shape is
//! testable without touching the filesystem or the database.

use crate::emit::model::{ApproachRow, SpecRecord};
use crate::emit::trust::Trust;

/// The irreducible prose a model supplies for one slice. Everything else in a
/// slice document is assembled from stored rows.
pub struct SliceContent {
    /// One-line statement of what this slice set out to do.
    pub intent: String,
    /// One entry per component touched: what it does and under what conditions.
    pub components: Vec<String>,
    /// Non-obvious conditions: root causes, gotchas, and documented limitations.
    pub conditions: Vec<String>,
}

/// Render a bullet list, or a placeholder line when the list is empty, so a
/// section never renders as a bare heading.
fn bullets(items: &[String]) -> String {
    if items.is_empty() {
        return "_None recorded._\n".to_string();
    }
    items
        .iter()
        .map(|i| format!("- {}\n", i))
        .collect::<Vec<_>>()
        .join("")
}

/// Choose a code-fence long enough that the content cannot close it early.
/// CommonMark ends a fenced block at the first line whose backtick run is at
/// least as long as the opening fence, so an interface contract containing its
/// own fence would otherwise terminate the block and swallow everything after
/// it into a code span. The fence is therefore one backtick longer than the
/// longest run the content contains, with three as the floor.
fn fence_for(content: &str) -> String {
    let mut longest = 0usize;
    let mut current = 0usize;
    for ch in content.chars() {
        if ch == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    "`".repeat(longest.max(2) + 1)
}

/// Render the chosen approach as a decision block naming the strongest rejected
/// alternative and the reason it lost. Returns an empty string when no approach
/// was marked chosen, because a decision with no chosen path is not a decision.
fn render_decision(approaches: &[ApproachRow], trust: Trust) -> String {
    // Known gap against the design's decision-block schema: `where:` (a file or
    // area anchoring the decision) is specified as non-optional, but nothing in
    // the data model can source it -- `approaches` has no such column. The field
    // is therefore absent rather than fabricated. Phase 2 computes the changed
    // file set per slice, which is where an anchor would come from.
    let Some(chosen) = approaches.iter().find(|a| a.chosen) else {
        return String::new();
    };

    // The strongest rejected alternative is the highest-scoring unchosen
    // approach. Unscored approaches compare as f64::MIN so they sort last, and
    // max_by yields the LAST of several equal maxima, so a tie (including a tie
    // between two unscored approaches) resolves to the most recently recorded.
    let alternative = approaches.iter().filter(|a| !a.chosen).max_by(|a, b| {
        a.score
            .unwrap_or(f64::MIN)
            .partial_cmp(&b.score.unwrap_or(f64::MIN))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut out = format!("\n## Decision: {}\n\n", chosen.name);
    out.push_str(&format!("- **why:** {}\n", chosen.description));
    if let Some(alt) = alternative {
        let reason = if alt.cons.is_empty() {
            "no reason recorded".to_string()
        } else {
            alt.cons.join("; ")
        };
        out.push_str(&format!(
            "- **alternative:** {} -- rejected: {}\n",
            alt.name, reason
        ));
    }
    out.push_str(&format!("- **trust:** {}\n", trust.label()));
    out
}

/// Render a spec's top-level record document.
pub fn render_record(record: &SpecRecord, trust: Trust) -> String {
    let mut out = format!("# Record: {}\n\n", record.task_description);
    out.push_str(&format!(
        "- **spec:** `{}`\n- **type:** {}\n\n",
        record.id, record.task_type
    ));

    out.push_str("## Acceptance criteria\n\n");
    out.push_str(&bullets(&record.acceptance_criteria));

    out.push_str("\n## Edge cases\n\n");
    out.push_str(&bullets(&record.edge_cases));

    if let Some(contract) = &record.interface_contract {
        out.push_str("\n## Interface contract\n\n");
        let fence = fence_for(contract);
        out.push_str(&format!("{}text\n{}\n{}\n", fence, contract, fence));
    }

    out.push_str(&render_decision(&record.approaches, trust));

    out.push_str("\n## Verification evidence\n\n");
    if record.verifications.is_empty() {
        out.push_str("_No verification runs recorded._\n");
    } else {
        for v in &record.verifications {
            out.push_str(&format!(
                "- `{}` -- {}\n",
                v.command,
                if v.success { "passed" } else { "failed" }
            ));
        }
    }

    out
}

/// Render one slice document: the model's knowledge-transfer prose plus the
/// decisions and discoveries stored for the spec.
pub fn render_slice(
    index: i64,
    content: &SliceContent,
    record: &SpecRecord,
    trust: Trust,
) -> String {
    let mut out = format!("# Slice {:03}: {}\n\n", index, content.intent);
    out.push_str(&format!("- **spec:** `{}`\n\n", record.id));

    out.push_str("## Components\n\n");
    out.push_str(&bullets(&content.components));

    out.push_str("\n## Hard-won conditions\n\n");
    let mut conditions = content.conditions.clone();
    // Phase 1 has no time window, so every slice repeats the spec's entire
    // discovery history rather than only what is new since the previous
    // checkpoint. Slice 003 therefore restates what 001 and 002 already showed.
    // Phase 2's slice diffing supplies the scoping that fixes this.
    for learn in &record.learns {
        conditions.push(match &learn.context {
            Some(ctx) => format!("{} ({})", learn.discovery, ctx),
            None => learn.discovery.clone(),
        });
    }
    out.push_str(&bullets(&conditions));

    out.push_str(&render_decision(&record.approaches, trust));

    out
}

#[cfg(test)]
/// Tests for the record and slice renderers.
mod tests {
    use super::*;
    use crate::emit::model::{ApproachRow, LearnRow, SpecRecord, VerificationRow};
    use crate::emit::trust::Trust;

    /// Build a record with one chosen and one rejected approach plus a learning.
    fn record() -> SpecRecord {
        SpecRecord {
            id: "spec_1".into(),
            task_description: "Add a thing".into(),
            task_type: "feature".into(),
            acceptance_criteria: vec!["it works".into()],
            edge_cases: vec!["empty input".into()],
            interface_contract: Some("fn thing() -> u8".into()),
            approaches: vec![
                ApproachRow {
                    name: "Direct".into(),
                    description: "Do it directly".into(),
                    pros: vec!["simple".into()],
                    cons: vec![],
                    score: Some(8.0),
                    chosen: true,
                },
                ApproachRow {
                    name: "Indirect".into(),
                    description: "Add a layer".into(),
                    pros: vec![],
                    cons: vec!["slower".into()],
                    score: Some(5.0),
                    chosen: false,
                },
            ],
            learns: vec![LearnRow {
                discovery: "The cache lies on cold start".into(),
                context: Some("found while tracing".into()),
                tags: vec!["cache".into()],
            }],
            verifications: vec![VerificationRow {
                command: "cargo test".into(),
                success: true,
                criteria_index: Some(0),
            }],
        }
    }

    /// The record renders its intent, criteria, edge cases, and contract.
    #[test]
    fn record_renders_spec_fields() {
        let md = render_record(&record(), Trust::SpecVerified);
        assert!(md.contains("# Record: Add a thing"));
        assert!(md.contains("it works"));
        assert!(md.contains("empty input"));
        assert!(md.contains("fn thing() -> u8"));
    }

    /// The record renders the chosen approach as a decision naming the rejected
    /// alternative and its rejection reason.
    #[test]
    fn record_renders_decision_with_alternative() {
        let md = render_record(&record(), Trust::SpecVerified);
        assert!(md.contains("## Decision: Direct"));
        assert!(md.contains("**alternative:**"));
        assert!(md.contains("Indirect"));
        assert!(md.contains("slower"));
    }

    /// The trust label appears verbatim so its caveat cannot be lost.
    #[test]
    fn record_renders_trust_label() {
        let md = render_record(&record(), Trust::SpecVerified);
        assert!(md.contains("not separately proved"));
    }

    /// A record with no approaches renders without a decisions section rather
    /// than emitting an empty heading.
    #[test]
    fn record_without_approaches_omits_decisions() {
        let mut r = record();
        r.approaches.clear();
        let md = render_record(&r, Trust::Unverified);
        assert!(!md.contains("## Decision:"));
    }

    /// A slice renders its components and its hard-won conditions under distinct
    /// headings, and carries the learnings captured for the spec.
    #[test]
    fn slice_renders_knowledge_transfer() {
        let content = SliceContent {
            intent: "wire it up".into(),
            components: vec!["Renderer -- turns rows into markdown".into()],
            conditions: vec!["Empty specs still render".into()],
        };
        let md = render_slice(2, &content, &record(), Trust::SpecVerified);
        assert!(md.contains("# Slice 002: wire it up"));
        assert!(md.contains("## Components"));
        assert!(md.contains("Renderer -- turns rows into markdown"));
        assert!(md.contains("## Hard-won conditions"));
        assert!(md.contains("Empty specs still render"));
        assert!(md.contains("The cache lies on cold start"));
    }

    /// An interface contract containing its own code fence does not terminate the
    /// block early. Without a longer outer fence the contract's closing backticks
    /// would end the section and swallow the Decision and Verification headings
    /// into a code span -- silent corruption of a committed document.
    #[test]
    fn interface_contract_containing_a_fence_stays_fenced() {
        let mut r = record();
        r.interface_contract = Some("example:\n```\nfn thing() {}\n```".into());
        let md = render_record(&r, Trust::SpecVerified);
        assert!(md.contains("````text"));
        // The opening and closing fences must PAIR. Asserting that the later
        // headings are merely present proves nothing, because they are emitted
        // unconditionally and `contains` does not parse markdown -- a mismatched
        // closing fence would leave the block open and still satisfy that check.
        // Counting the four-backtick runs catches exactly that regression: the
        // contract's own three-backtick fences cannot match, so a correct render
        // has precisely two.
        assert_eq!(md.matches("````").count(), 2);
    }

    /// Content with no backticks gets the minimum three-backtick fence, not a
    /// wider one. Without this, an off-by-one in `fence_for` would go unnoticed
    /// because every other test only checks that the content survives.
    #[test]
    fn fence_floor_is_three_backticks() {
        let mut r = record();
        r.interface_contract = Some("fn thing() -> u8".into());
        let md = render_record(&r, Trust::SpecVerified);
        assert!(md.contains("```text"));
        assert!(!md.contains("````"));
    }

    /// The rendered alternative is the highest-SCORING rejected approach, and the
    /// score is what drives the choice -- not the position in the list.
    ///
    /// The ordering here is deliberate. `Strong` is neither the first nor the
    /// last rejected approach, so this arrangement discriminates between all
    /// three plausible selection rules at once: "first unchosen" would surface
    /// `Weak`, "last unchosen" would surface `Mid`, and only a score-driven rule
    /// surfaces `Strong`. Putting the highest scorer at either end, as an earlier
    /// version of this test did, cannot tell those rules apart.
    #[test]
    fn alternative_is_the_highest_scoring_rejection() {
        let mut r = record();
        r.approaches = vec![
            ApproachRow {
                name: "Chosen".into(),
                description: "the taken path".into(),
                pros: vec![],
                cons: vec![],
                score: Some(9.0),
                chosen: true,
            },
            ApproachRow {
                name: "Weak".into(),
                description: "a poor option".into(),
                pros: vec![],
                cons: vec!["barely works".into()],
                score: Some(2.0),
                chosen: false,
            },
            ApproachRow {
                name: "Strong".into(),
                description: "the real contender".into(),
                pros: vec![],
                cons: vec!["slower".into()],
                score: Some(7.0),
                chosen: false,
            },
            ApproachRow {
                name: "Mid".into(),
                description: "a middling option".into(),
                pros: vec![],
                cons: vec!["awkward".into()],
                score: Some(5.0),
                chosen: false,
            },
        ];
        let md = render_record(&r, Trust::SpecVerified);
        assert!(md.contains("**alternative:** Strong -- rejected: slower"));
        assert!(!md.contains("Weak"));
        assert!(!md.contains("Mid"));
    }
}
