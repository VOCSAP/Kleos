use kleos_transcript_consumer::feed::{decode, FeedError, FeedEvent, FeedItem};
use serde_json::{json, Value};

fn item(kind: &str, schema_version: u32, payload: Value) -> FeedItem {
    FeedItem {
        seq: 1,
        kind: kind.to_string(),
        schema_version,
        payload,
    }
}

fn epoch_opened() -> Value {
    json!({
        "file_key": "fk1",
        "epoch": 0,
        "epoch_reason": "initial",
        "view_version": 1,
        "secret_patterns_rev": 3,
        "secret_patterns_hash": "ab12"
    })
}

fn session_meta() -> Value {
    json!({
        "session_id": "s1", "file_key": "fk1", "host_id": "h1", "kind": "main",
        "parent_session": null, "labels": [], "git_root": null, "repo_name": null,
        "project_key": null, "lineage_id": "0123456789abcdef", "branch_id": "b0",
        "cc_version": "2.0.0", "entrypoint": "cli", "agent": null, "view_version": 1,
        "secret_patterns_rev": 3, "secret_patterns_hash": "ab12"
    })
}

// Mutation: replace the unknown-kind arm of decode by a default variant.
#[test]
fn unknown_kind_is_refused() {
    let err = decode(&item("session_closed", 1, json!({}))).unwrap_err();
    assert!(matches!(err, FeedError::UnknownKind(k) if k == "session_closed"));
}

// Mutation: remove the schema_version check in decode.
#[test]
fn schema_version_two_is_refused() {
    let err = decode(&item("epoch_opened", 2, epoch_opened())).unwrap_err();
    assert!(matches!(err, FeedError::UnsupportedSchemaVersion(2)));
}

// Mutation: add #[serde(default)] on a required field of EpochOpened.
#[test]
fn missing_required_field_is_refused() {
    let mut payload = epoch_opened();
    payload.as_object_mut().unwrap().remove("epoch");
    let err = decode(&item("epoch_opened", 1, payload)).unwrap_err();
    assert!(matches!(err, FeedError::Payload { .. }));
}

// Mutation: drop the deserialize_with on SessionMeta::parent_session so that
// absence silently becomes None.
#[test]
fn nullable_field_must_still_be_present() {
    let mut payload = session_meta();
    payload.as_object_mut().unwrap().remove("parent_session");
    assert!(decode(&item("session_meta", 1, payload)).is_err());
}

// Mutation: drop the secret_patterns_version rejection and default rev and hash on EpochOpened (each alone stays green).
#[test]
fn legacy_secret_patterns_version_alone_is_refused() {
    let mut payload = epoch_opened();
    let obj = payload.as_object_mut().unwrap();
    obj.remove("secret_patterns_rev");
    obj.remove("secret_patterns_hash");
    obj.insert("secret_patterns_version".into(), json!(3));
    assert!(decode(&item("epoch_opened", 1, payload)).is_err());
}

// Mutation: remove the explicit secret_patterns_version rejection in decode.
#[test]
fn legacy_secret_patterns_version_next_to_new_fields_is_refused() {
    let mut payload = epoch_opened();
    payload
        .as_object_mut()
        .unwrap()
        .insert("secret_patterns_version".into(), json!(3));
    let err = decode(&item("epoch_opened", 1, payload)).unwrap_err();
    assert!(matches!(err, FeedError::LegacySecretPatternsVersion));
}

// Mutation: add #[serde(deny_unknown_fields)] on a payload struct.
#[test]
fn extra_field_is_tolerated() {
    let mut payload = epoch_opened();
    payload
        .as_object_mut()
        .unwrap()
        .insert("added_later".into(), json!("x"));
    assert!(matches!(
        decode(&item("epoch_opened", 1, payload)).unwrap(),
        FeedEvent::EpochOpened(_)
    ));
}

// Mutation: make secret_patterns_rev a String or secret_patterns_hash a number.
#[test]
fn patterns_rev_is_an_integer_and_hash_a_string() {
    let mut payload = epoch_opened();
    payload["secret_patterns_rev"] = json!("3");
    assert!(decode(&item("epoch_opened", 1, payload)).is_err());
    let mut payload = epoch_opened();
    payload["secret_patterns_hash"] = json!(12);
    assert!(decode(&item("epoch_opened", 1, payload)).is_err());
}

// Mutation: add #[serde(other)] to an enum of strings.
#[test]
fn unknown_enum_value_is_refused() {
    let mut payload = epoch_opened();
    payload["epoch_reason"] = json!("manual");
    assert!(decode(&item("epoch_opened", 1, payload)).is_err());
}
