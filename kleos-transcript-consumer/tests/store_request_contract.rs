use kleos_lib::db::Database;
use kleos_lib::lexicon::word_class;
use kleos_lib::llm::prompts::load_prompt;
use kleos_lib::llm::think_setting;
use kleos_lib::memory::types::StoreRequest;
use kleos_transcript_consumer::feed::FactCategory;
use kleos_transcript_consumer::publish::{store_request, PublishFact, PublishSession, SourceTurn};
use serde_json::Value;

fn fact(importance: i32, parent: Option<i64>) -> PublishFact {
    PublishFact {
        lineage_id: "0123456789abcdef".into(),
        branch_id: "b1".into(),
        fact_id: 4,
        category: FactCategory::Decision,
        importance,
        content: "The relay publishes capped importance.".into(),
        turn: SourceTurn {
            uuid: "11111111-2222-3333-4444-555555555555".into(),
            ts: "2026-10-01T12:30:00Z".into(),
        },
        parent_memory_id: parent,
    }
}

fn session(space: Option<&str>) -> PublishSession {
    PublishSession {
        session_id: "sess-1".into(),
        project: "demo-project".into(),
        space: space.map(str::to_string),
    }
}

fn roundtrip(f: &PublishFact, s: &PublishSession) -> (Value, StoreRequest) {
    let body = serde_json::to_value(store_request(f, s)).unwrap();
    let parsed: StoreRequest = serde_json::from_value(body.clone()).unwrap();
    (body, parsed)
}

// Mutation: change source, category, created_at, session_id, space, tags or
// is_static in store_request.
#[test]
fn body_deserializes_into_store_request_with_every_contract_field() {
    let (body, req) = roundtrip(&fact(4, None), &session(Some("demo-space")));
    assert_eq!(req.source, "kleos-transcript");
    assert_eq!(req.category, "decision");
    assert_eq!(req.importance, 4);
    assert_eq!(req.content, "The relay publishes capped importance.");
    assert_eq!(req.created_at.as_deref(), Some("2026-10-01T12:30:00Z"));
    assert_eq!(req.session_id.as_deref(), Some("sess-1"));
    assert_eq!(req.space.as_deref(), Some("demo-space"));
    assert_eq!(
        req.tags.unwrap(),
        [
            "transcript",
            "demo-project",
            "turn:11111111-2222-3333-4444-555555555555",
            "fact:01234567-b1-4"
        ]
    );
    assert_eq!(req.parent_memory_id, None);
    assert_eq!(req.is_static, None);
    assert!(body["is_static"].is_null());
}

// Mutation: replace min(importance, 5) by importance, or by a constant.
#[test]
fn importance_is_capped_at_five_and_lower_values_are_kept() {
    assert_eq!(roundtrip(&fact(9, None), &session(None)).1.importance, 5);
    assert_eq!(roundtrip(&fact(3, None), &session(None)).1.importance, 3);
    assert_eq!(roundtrip(&fact(5, None), &session(None)).1.importance, 5);
}

// Mutation: ignore parent_memory_id in store_request.
#[test]
fn a_version_carries_parent_memory_id() {
    let (_, req) = roundtrip(&fact(4, Some(812)), &session(None));
    assert_eq!(req.parent_memory_id, Some(812));
}

// Mutation: send a default space name instead of None when the session has none.
#[test]
fn a_session_without_space_sends_none() {
    assert_eq!(roundtrip(&fact(4, None), &session(None)).1.space, None);
}

// Mutation: rename or change the signature of any referenced kleos-lib function.
#[test]
fn kleos_lib_entry_points_still_exist_with_the_expected_shape() {
    let _think: fn() -> Option<bool> = think_setting;
    let _load: fn(&str, &'static str) -> std::borrow::Cow<'static, str> = load_prompt;
    let _lexicon: fn(&str, &str) -> Vec<String> = word_class;
    // An async fn body does not run until polled.
    drop(Database::connect_encrypted("unused.db", None));
}
