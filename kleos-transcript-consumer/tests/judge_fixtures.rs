use std::fs;
use std::path::PathBuf;

use kleos_transcript_consumer::judge::{
    effect, DropReason, Effect, Fidelity, Relation, Verdict, FIDELITY_PROMPT,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    case: String,
    expected_verdict: Verdict,
    expected_effect: Effect,
    fact: serde_json::Value,
    source_turns: Vec<serde_json::Value>,
    neighbours: Vec<serde_json::Value>,
}

fn cases() -> Vec<Case> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/judge");
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|p| {
            serde_json::from_str(&fs::read_to_string(p).unwrap())
                .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        })
        .collect()
}

fn verdict(fidelity: Fidelity, relation: Relation, neighbour_id: Option<i64>) -> Verdict {
    Verdict {
        fidelity,
        relation,
        neighbour_id,
        reason: String::new(),
    }
}

// Mutation: change the reason of the Infidele or Doublon arm, the HoldConflict arm, or the publish tag prefix in effect().
#[test]
fn effect_matches_expected_effect_of_every_case() {
    let cases = cases();
    for c in &cases {
        assert_eq!(effect(&c.expected_verdict), c.expected_effect, "{}", c.case);
    }
}

// Mutation: delete a fixture file.
#[test]
fn the_five_judgement_cases_are_present() {
    let mut names: Vec<String> = cases().into_iter().map(|c| c.case).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "agent_contradiction",
            "agent_duplicate",
            "distorted_fact",
            "faithful_distinct",
            "invented_fact"
        ]
    );
}

// Mutation: empty a fixture's source_turns or fact, or the neighbours of a faithful case.
#[test]
fn every_case_carries_fact_source_turns_and_neighbours() {
    for c in cases() {
        assert!(c.fact.get("content").is_some(), "{}", c.case);
        assert!(!c.source_turns.is_empty(), "{}", c.case);
        let judged_against_neighbours = c.expected_verdict.neighbour_id.is_some()
            || c.expected_verdict.fidelity == Fidelity::Fidele;
        assert!(
            !judged_against_neighbours || !c.neighbours.is_empty(),
            "{}: a faithful or neighbour-referencing case needs neighbours",
            c.case
        );
    }
}

// Mutation: test relation before fidelity in effect(), or swap the drop reason of Infidele and Inverifiable.
#[test]
fn fidelity_wins_over_relation() {
    for relation in [
        Relation::Distinct,
        Relation::Doublon,
        Relation::Contredit,
        Relation::Prolonge,
        Relation::MetAJour,
        Relation::Partiel,
    ] {
        for (fidelity, reason) in [
            (Fidelity::Infidele, DropReason::Unfaithful),
            (Fidelity::Inverifiable, DropReason::Unverifiable),
        ] {
            assert_eq!(
                effect(&verdict(fidelity, relation, Some(7))),
                Effect::Drop { reason },
                "{fidelity:?} {relation:?}"
            );
        }
    }
}

// Mutation: map Contredit without neighbour to HoldConflict or Publish.
#[test]
fn contradiction_without_neighbour_is_dropped_fail_closed() {
    let e = effect(&verdict(Fidelity::Fidele, Relation::Contredit, None));
    assert_eq!(
        e,
        Effect::Drop {
            reason: DropReason::ConflictWithoutNeighbour
        }
    );
}

// Mutation: publish a doublon, or change the tag prefix.
#[test]
fn publishing_relations_carry_the_relation_tag_and_doublon_is_dropped() {
    for (relation, tag) in [
        (Relation::Distinct, "relation:distinct"),
        (Relation::Prolonge, "relation:prolonge"),
        (Relation::MetAJour, "relation:met_a_jour"),
        (Relation::Partiel, "relation:partiel"),
    ] {
        assert_eq!(
            effect(&verdict(Fidelity::Fidele, relation, None)),
            Effect::Publish {
                relation_tag: tag.to_string()
            }
        );
    }
    assert_eq!(
        effect(&verdict(Fidelity::Fidele, Relation::Doublon, Some(1))),
        Effect::Drop {
            reason: DropReason::Duplicate
        }
    );
}

// Mutation: rename a variant without serde rename_all, or empty the prompt file.
#[test]
fn verdict_wire_format_is_snake_case_and_prompt_states_the_schema() {
    let v: Verdict = serde_json::from_str(
        r#"{"fidelity":"fidele","relation":"met_a_jour","neighbour_id":3,"reason":"r"}"#,
    )
    .unwrap();
    assert_eq!(v.relation, Relation::MetAJour);
    for needle in [
        "fidelity",
        "relation",
        "neighbour_id",
        "reason",
        "met_a_jour",
        "JSON",
    ] {
        assert!(FIDELITY_PROMPT.contains(needle), "prompt lacks {needle}");
    }
}
