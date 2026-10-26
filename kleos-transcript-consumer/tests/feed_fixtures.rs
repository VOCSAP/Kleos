use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use kleos_transcript_consumer::feed::{decode, FeedEvent, FeedItem, RemapMotif};

const ALL_KINDS: [&str; 9] = [
    "session_meta",
    "epoch_opened",
    "epoch_superseded",
    "facts_updated",
    "lineage_stabilized",
    "lineage_merged",
    "lineage_reopened",
    "branch_root_changed",
    "remap",
];

fn fixture_files() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/feed");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    files.sort();
    files
}

fn items_of(path: &PathBuf) -> Vec<FeedItem> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .enumerate()
        .map(|(i, line)| {
            serde_json::from_str::<FeedItem>(line)
                .unwrap_or_else(|e| panic!("{}:{}: not a feed item: {e}", path.display(), i + 1))
        })
        .collect()
}

// Mutation: drop a kind from decode's dispatch, map a kind to another variant, or rename a payload field.
#[test]
fn every_fixture_line_decodes_to_the_variant_of_its_kind() {
    let files = fixture_files();
    assert!(!files.is_empty(), "no feed fixture found");
    for path in files {
        for item in items_of(&path) {
            let event = decode(&item)
                .unwrap_or_else(|e| panic!("{} seq {}: {e}", path.display(), item.seq));
            assert_eq!(
                event.kind(),
                item.kind,
                "{} seq {}",
                path.display(),
                item.seq
            );
        }
    }
}

// Mutation: delete the only fixture line of one kind, or the only remap line of one motif.
#[test]
fn fixtures_cover_all_nine_kinds_and_all_four_remap_motifs() {
    let mut kinds = BTreeSet::new();
    let mut motifs = BTreeSet::new();
    for path in fixture_files() {
        for item in items_of(&path) {
            kinds.insert(item.kind.clone());
            if let FeedEvent::Remap(r) = decode(&item).unwrap() {
                motifs.insert(r.motif);
            }
        }
    }
    let expected: BTreeSet<String> = ALL_KINDS.iter().map(|k| k.to_string()).collect();
    assert_eq!(kinds, expected);
    let expected_motifs = BTreeSet::from([
        RemapMotif::ModelChange,
        RemapMotif::PromptChange,
        RemapMotif::ViewUpgrade,
        RemapMotif::PatternsUpgrade,
    ]);
    assert_eq!(motifs, expected_motifs);
}

// Mutation: duplicate a seq or swap two lines inside a fixture file.
#[test]
fn seq_strictly_increases_within_each_file() {
    for path in fixture_files() {
        let seqs: Vec<i64> = items_of(&path).iter().map(|i| i.seq).collect();
        assert!(
            seqs.windows(2).all(|w| w[0] < w[1]),
            "{}: seq not strictly increasing: {seqs:?}",
            path.display()
        );
    }
}

// Mutation: remove the file named in the scenario list.
#[test]
fn expected_scenarios_are_present() {
    let names: BTreeSet<String> = fixture_files()
        .iter()
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    for scenario in [
        "session_main_lifecycle",
        "subagent_session",
        "divergent_labels",
        "sibling_branches",
        "lineage_merge",
        "reopen_after_stabilize",
        "remap_all_motifs",
        "low_patterns_rev",
        "rewind_abandoned",
    ] {
        assert!(names.contains(scenario), "missing scenario {scenario}");
    }
}

// Mutation: make decode filter out abandoned or proposed facts.
#[test]
fn rewind_fixture_decodes_abandoned_and_proposed_facts() {
    let path = fixture_files()
        .into_iter()
        .find(|p| p.file_stem().unwrap() == "rewind_abandoned")
        .unwrap();
    let mut abandoned = 0;
    let mut proposed = 0;
    for item in items_of(&path) {
        if let FeedEvent::FactsUpdated(f) = decode(&item).unwrap() {
            abandoned += f.facts.iter().filter(|x| x.abandoned).count();
            proposed += f
                .facts
                .iter()
                .filter(|x| x.status == kleos_transcript_consumer::feed::FactStatus::Proposed)
                .count();
        }
    }
    assert!(abandoned >= 1 && proposed >= 1);
}

// Mutation: make decode reject a facts_updated whose secret_patterns_rev is low.
#[test]
fn low_patterns_rev_fixture_still_decodes() {
    let path = fixture_files()
        .into_iter()
        .find(|p| p.file_stem().unwrap() == "low_patterns_rev")
        .unwrap();
    let revs: Vec<u32> = items_of(&path)
        .iter()
        .filter_map(|i| match decode(i).unwrap() {
            FeedEvent::FactsUpdated(f) => Some(f.secret_patterns_rev),
            _ => None,
        })
        .collect();
    assert!(revs.contains(&0));
}
