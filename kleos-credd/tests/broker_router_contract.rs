#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

use axum::{
    body::Body,
    http::{header, HeaderMap, Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use kleos_cred::{crypto::KEY_SIZE, storage, types::SecretData};
use kleos_credd::{
    broker::{self, BrokerConfigError, BrokerState, Policy},
    peercred::PeerIdentity,
};
use kleos_lib::db::Database;
use serde_json::{json, Value};
use tower::ServiceExt;

const AUTHORISED_UID: u32 = 10001;
const FOREIGN_UID: u32 = 10003;
const API_VALUE: &str = "FAKE_API_VALUE_FOR_CONTRACT_TESTS";
const UNAUTHORISED_VALUE: &str = "FAKE_UNAUTHORISED_TOKEN_VALUE_X9";
const MASTER_KEY: [u8; KEY_SIZE] = [0x5a; KEY_SIZE];
const OTHER_KEY: [u8; KEY_SIZE] = [0x11; KEY_SIZE];

fn master_hex() -> String {
    hex::encode(MASTER_KEY)
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/broker")
        .join(relative)
}

fn golden(name: &str) -> String {
    fs::read_to_string(fixture(&format!("golden/{name}")))
        .unwrap_or_else(|error| panic!("golden fixture {name} unreadable: {error}"))
        .trim_end()
        .to_owned()
}

fn error_envelope(code: &str) -> String {
    format!(r#"{{"protocol":1,"error":{{"code":"{code}"}}}}"#)
}

struct Seed {
    service: &'static str,
    name: &'static str,
    data: SecretData,
    key: [u8; KEY_SIZE],
}

fn api_seed(service: &'static str, name: &'static str, value: &str) -> Seed {
    Seed {
        service,
        name,
        data: SecretData::ApiKey {
            key: value.to_owned(),
            endpoint: None,
            notes: None,
        },
        key: MASTER_KEY,
    }
}

fn standard_seeds() -> Vec<Seed> {
    vec![
        api_seed("smtp", "api-key", API_VALUE),
        api_seed("kleos", "api-key", API_VALUE),
        api_seed("github", "api-key", API_VALUE),
        api_seed("github", "token", UNAUTHORISED_VALUE),
        api_seed("memory-hub", "api-key", API_VALUE),
        api_seed("stray", "api-key", UNAUTHORISED_VALUE),
    ]
}

fn field_seeds() -> Vec<Seed> {
    vec![
        Seed {
            service: "account-demo",
            name: "login",
            data: SecretData::Login {
                username: "synthetic-user".to_owned(),
                password: UNAUTHORISED_VALUE.to_owned(),
                url: None,
                totp_seed: None,
                notes: None,
            },
            key: MASTER_KEY,
        },
        Seed {
            service: "ssh-demo",
            name: "test-host",
            data: SecretData::SshKey {
                private_key: API_VALUE.to_owned(),
                public_key: Some(UNAUTHORISED_VALUE.to_owned()),
                passphrase: None,
            },
            key: MASTER_KEY,
        },
        Seed {
            service: "endpoint-demo",
            name: "api-key",
            data: SecretData::ApiKey {
                key: UNAUTHORISED_VALUE.to_owned(),
                endpoint: None,
                notes: None,
            },
            key: MASTER_KEY,
        },
    ]
}

async fn populate(db: &Database, seeds: &[Seed]) {
    for seed in seeds {
        storage::store_secret(db, 1, seed.service, seed.name, &seed.data, &seed.key)
            .await
            .unwrap_or_else(|error| {
                panic!("seeding {}/{} failed: {error}", seed.service, seed.name)
            });
    }
}

async fn router_for(policy_fixture: &str, seeds: &[Seed]) -> Router {
    let db = Database::connect_memory().await.expect("in-memory vault");
    populate(&db, seeds).await;
    let policy = Policy::from_file(&fixture(policy_fixture)).expect("fixture policy parses");
    broker::router(BrokerState::new(db, MASTER_KEY, policy, AUTHORISED_UID))
}

async fn standard_router() -> Router {
    router_for("policy.toml", &standard_seeds()).await
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Reply {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

fn peer(uid: u32) -> Option<PeerIdentity> {
    Some(PeerIdentity { uid, pid: 4242 })
}

async fn send(router: &Router, mut request: Request<Body>, peer: Option<PeerIdentity>) -> Reply {
    if let Some(peer) = peer {
        request.extensions_mut().insert(peer);
    }
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("router is infallible");
    let (parts, body) = response.into_parts();
    Reply {
        status: parts.status,
        headers: parts.headers,
        body: body
            .collect()
            .await
            .expect("collect response body")
            .to_bytes()
            .to_vec(),
    }
}

fn resolve_with(body: impl Into<Body>) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/v1/resolve")
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.into())
        .unwrap()
}

fn resolve_json(service: &str, key: &str, field: Option<&str>) -> Request<Body> {
    resolve_with(json!({"protocol": 1, "service": service, "key": key, "field": field}).to_string())
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

fn assert_error(reply: &Reply, status: StatusCode, code: &str, case: &str) {
    assert_eq!(
        reply.status,
        status,
        "{case}: expected {status}, got {} with body {}",
        reply.status,
        reply.text()
    );
    assert_eq!(
        reply.text(),
        error_envelope(code),
        "{case}: error envelope must carry only the code {code}"
    );
}

#[tokio::test]
async fn resolve_answers_with_the_golden_envelope_and_no_store() {
    let router = standard_router().await;
    let request_body = golden("resolve-request.json");

    let reply = send(&router, resolve_with(request_body), peer(AUTHORISED_UID)).await;

    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.text());
    assert_eq!(reply.text(), golden("resolve-response.json"));
    assert_eq!(
        reply
            .headers
            .get(header::CACHE_CONTROL)
            .map(|v| v.as_bytes()),
        Some(b"no-store".as_slice()),
        "a resolved value must never be cacheable"
    );
    assert!(reply
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json")));
}

#[tokio::test]
async fn entries_answer_with_the_golden_list_limited_to_authorised_existing_references() {
    let router = standard_router().await;

    let reply = send(&router, get("/v1/entries"), peer(AUTHORISED_UID)).await;

    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.text());
    assert_eq!(reply.text(), golden("entries-response.json"));
    let body: Value = serde_json::from_slice(&reply.body).unwrap();
    let listed: Vec<String> = body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            format!(
                "{}/{}",
                entry["service"].as_str().unwrap(),
                entry["key"].as_str().unwrap()
            )
        })
        .collect();
    for forbidden in [
        "github/token",
        "stray/api-key",
        "memory-hub/api-key",
        "resend/api-key",
    ] {
        assert!(
            !listed.iter().any(|entry| entry == forbidden),
            "entries leaked or invented {forbidden}: {listed:?}"
        );
    }
}

#[tokio::test]
async fn entries_list_a_reference_once_even_when_several_selectors_target_it() {
    let policy = fixture("policy-field.toml");
    let db = Database::connect_memory().await.unwrap();
    populate(&db, &field_seeds()).await;
    let router = broker::router(BrokerState::new(
        db,
        MASTER_KEY,
        Policy::from_file(&policy).unwrap(),
        AUTHORISED_UID,
    ));

    let reply = send(&router, get("/v1/entries"), peer(AUTHORISED_UID)).await;

    let body: Value = serde_json::from_slice(&reply.body).unwrap();
    let account_rows = body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["service"] == "account-demo")
        .count();
    assert_eq!(
        account_rows, 1,
        "account-demo/login is targeted by two selectors but must be listed once: {body}"
    );
}

#[tokio::test]
async fn healthz_answers_with_the_golden_ready_envelope() {
    let router = standard_router().await;

    let reply = send(&router, get("/healthz"), peer(AUTHORISED_UID)).await;

    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.text(), golden("healthz-response.json"));
}

#[tokio::test]
async fn one_policy_line_exposes_a_new_reference_without_any_other_change() {
    let base = router_for("policy.toml", &standard_seeds()).await;
    let extended = router_for("policy-extended.toml", &standard_seeds()).await;

    let before = send(
        &base,
        resolve_json("memory-hub", "api-key", None),
        peer(AUTHORISED_UID),
    )
    .await;
    let after = send(
        &extended,
        resolve_json("memory-hub", "api-key", None),
        peer(AUTHORISED_UID),
    )
    .await;
    let listed = send(&extended, get("/v1/entries"), peer(AUTHORISED_UID)).await;

    assert_error(
        &before,
        StatusCode::FORBIDDEN,
        "DENIED",
        "memory-hub before its policy line",
    );
    assert_eq!(after.status, StatusCode::OK, "body: {}", after.text());
    assert_eq!(after.text(), golden("resolve-response.json"));
    assert_eq!(listed.text(), golden("entries-extended-response.json"));
}

#[tokio::test]
async fn a_reference_that_exists_but_is_not_in_the_policy_is_denied_with_the_golden_envelope() {
    let router = standard_router().await;

    let reply = send(
        &router,
        resolve_json("github", "token", None),
        peer(AUTHORISED_UID),
    )
    .await;

    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    assert_eq!(reply.text(), golden("error-denied.json"));
    assert!(!reply.text().contains(UNAUTHORISED_VALUE));
}

#[tokio::test]
async fn denial_is_identical_whether_the_unauthorised_reference_exists_or_not() {
    let router = standard_router().await;

    let existing = send(
        &router,
        resolve_json("github", "token", None),
        peer(AUTHORISED_UID),
    )
    .await;
    let absent = send(
        &router,
        resolve_json("nowhere", "nothing", None),
        peer(AUTHORISED_UID),
    )
    .await;
    let foreign_existing = send(
        &router,
        resolve_json("smtp", "api-key", None),
        peer(FOREIGN_UID),
    )
    .await;
    let foreign_absent = send(
        &router,
        resolve_json("nowhere", "nothing", None),
        peer(FOREIGN_UID),
    )
    .await;

    for (case, reply) in [
        ("authorised uid, existing unauthorised reference", &existing),
        ("authorised uid, absent unauthorised reference", &absent),
        (
            "foreign uid, existing authorised reference",
            &foreign_existing,
        ),
        ("foreign uid, absent reference", &foreign_absent),
    ] {
        assert_error(reply, StatusCode::FORBIDDEN, "DENIED", case);
    }
}

#[tokio::test]
async fn selectors_compare_service_and_key_exactly_and_case_sensitively() {
    let router = standard_router().await;

    for (service, key) in [
        ("SMTP", "api-key"),
        ("smtp", "API-KEY"),
        ("smtp.", "api-key"),
        ("smtp", "api-key.old"),
    ] {
        let reply = send(
            &router,
            resolve_json(service, key, None),
            peer(AUTHORISED_UID),
        )
        .await;
        assert_error(
            &reply,
            StatusCode::FORBIDDEN,
            "DENIED",
            &format!("{service}/{key}"),
        );
    }
}

#[tokio::test]
async fn an_authorised_reference_missing_from_the_vault_is_not_found_and_unlisted() {
    let router = standard_router().await;

    let reply = send(
        &router,
        resolve_json("resend", "api-key", None),
        peer(AUTHORISED_UID),
    )
    .await;

    assert_error(
        &reply,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
        "authorised but absent",
    );
}

#[tokio::test]
async fn a_field_selector_authorises_exactly_that_field() {
    let router = router_for("policy-field.toml", &field_seeds()).await;
    let allowed = send(
        &router,
        resolve_json("account-demo", "login", Some("username")),
        peer(AUTHORISED_UID),
    )
    .await;
    assert_eq!(allowed.status, StatusCode::OK, "body: {}", allowed.text());
    assert_eq!(allowed.text(), r#"{"protocol":1,"value":"synthetic-user"}"#);

    for (service, key, field) in [
        ("account-demo", "login", Some("password")),
        ("ssh-demo", "test-host", Some("public_key")),
        ("ssh-demo", "test-host", None),
        ("account-demo", "login", Some("url")),
    ] {
        let reply = send(
            &router,
            resolve_json(service, key, field),
            peer(AUTHORISED_UID),
        )
        .await;
        assert_error(
            &reply,
            StatusCode::FORBIDDEN,
            "DENIED",
            &format!("{service}/{key} field {field:?}"),
        );
        assert!(!reply.text().contains(UNAUTHORISED_VALUE));
    }
}

#[tokio::test]
async fn an_authorised_field_the_secret_does_not_hold_is_unavailable() {
    let router = router_for("policy-field.toml", &field_seeds()).await;

    let missing_field = send(
        &router,
        resolve_json("endpoint-demo", "api-key", Some("endpoint")),
        peer(AUTHORISED_UID),
    )
    .await;
    let bare_login = send(
        &router,
        resolve_json("account-demo", "login", None),
        peer(AUTHORISED_UID),
    )
    .await;

    assert_error(
        &missing_field,
        StatusCode::UNPROCESSABLE_ENTITY,
        "FIELD_UNAVAILABLE",
        "optional field not set",
    );
    assert_error(
        &bare_login,
        StatusCode::UNPROCESSABLE_ENTITY,
        "FIELD_UNAVAILABLE",
        "bare value of a login",
    );
}

#[tokio::test]
async fn resolve_rejects_malformed_requests_with_invalid_request() {
    let router = standard_router().await;
    let long_service = "a".repeat(129);
    let long_field = "f".repeat(65);
    let cases: Vec<(&str, String)> = vec![
        ("truncated json", "{".to_owned()),
        ("empty body", String::new()),
        (
            "unknown field",
            r#"{"protocol":1,"service":"smtp","key":"api-key","field":null,"tool":"mail"}"#
                .to_owned(),
        ),
        (
            "duplicate field",
            r#"{"protocol":1,"service":"smtp","service":"kleos","key":"api-key","field":null}"#
                .to_owned(),
        ),
        (
            "missing field member",
            r#"{"protocol":1,"service":"smtp","key":"api-key"}"#.to_owned(),
        ),
        (
            "missing protocol",
            r#"{"service":"smtp","key":"api-key","field":null}"#.to_owned(),
        ),
        (
            "protocol as string",
            r#"{"protocol":"1","service":"smtp","key":"api-key","field":null}"#.to_owned(),
        ),
        (
            "protocol as float",
            r#"{"protocol":1.5,"service":"smtp","key":"api-key","field":null}"#.to_owned(),
        ),
        (
            "protocol as null",
            r#"{"protocol":null,"service":"smtp","key":"api-key","field":null}"#.to_owned(),
        ),
        (
            "protocol as bool",
            r#"{"protocol":true,"service":"smtp","key":"api-key","field":null}"#.to_owned(),
        ),
        (
            "service not a string",
            r#"{"protocol":1,"service":7,"key":"api-key","field":null}"#.to_owned(),
        ),
        (
            "empty service",
            json!({"protocol":1,"service":"","key":"api-key","field":null}).to_string(),
        ),
        (
            "service with slash",
            json!({"protocol":1,"service":"a/b","key":"api-key","field":null}).to_string(),
        ),
        (
            "service with at sign",
            json!({"protocol":1,"service":"a@b","key":"api-key","field":null}).to_string(),
        ),
        (
            "service wildcard",
            json!({"protocol":1,"service":"*","key":"api-key","field":null}).to_string(),
        ),
        (
            "service leading dash",
            json!({"protocol":1,"service":"-smtp","key":"api-key","field":null}).to_string(),
        ),
        (
            "service with space",
            json!({"protocol":1,"service":"sm tp","key":"api-key","field":null}).to_string(),
        ),
        (
            "service with dot dot",
            json!({"protocol":1,"service":"..","key":"api-key","field":null}).to_string(),
        ),
        (
            "service over 128 bytes",
            json!({"protocol":1,"service":long_service,"key":"api-key","field":null}).to_string(),
        ),
        (
            "key with slash",
            json!({"protocol":1,"service":"smtp","key":"api/key","field":null}).to_string(),
        ),
        (
            "key wildcard",
            json!({"protocol":1,"service":"smtp","key":"*","field":null}).to_string(),
        ),
        (
            "empty field",
            json!({"protocol":1,"service":"smtp","key":"api-key","field":""}).to_string(),
        ),
        (
            "field with dash",
            json!({"protocol":1,"service":"smtp","key":"api-key","field":"a-b"}).to_string(),
        ),
        (
            "field with dot",
            json!({"protocol":1,"service":"smtp","key":"api-key","field":"a.b"}).to_string(),
        ),
        (
            "field over 64 bytes",
            json!({"protocol":1,"service":"smtp","key":"api-key","field":long_field}).to_string(),
        ),
        (
            "field as number",
            r#"{"protocol":1,"service":"smtp","key":"api-key","field":3}"#.to_owned(),
        ),
    ];

    for (case, body) in cases {
        let reply = send(&router, resolve_with(body), peer(AUTHORISED_UID)).await;
        assert_error(&reply, StatusCode::BAD_REQUEST, "INVALID_REQUEST", case);
    }
}

#[tokio::test]
async fn resolve_rejects_a_field_starting_with_a_digit_before_consulting_the_policy() {
    let router = standard_router().await;

    for field in ["1token", "9", "0_field"] {
        let reply = send(
            &router,
            resolve_json("smtp", "api-key", Some(field)),
            peer(AUTHORISED_UID),
        )
        .await;
        assert_error(
            &reply,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
            &format!("field {field:?}"),
        );
    }
}

#[test]
fn policy_rejects_a_field_selector_starting_with_a_digit() {
    for selector in [
        "github/api-key@1field",
        "github/api-key@9",
        "ssh/key@0_private",
    ] {
        assert!(
            matches!(
                policy_from(&one_tool(selector)),
                Err(BrokerConfigError::InvalidPolicy)
            ),
            "selector {selector:?} must be rejected"
        );
    }
}

#[tokio::test]
async fn resolve_names_an_unsupported_protocol_for_every_integer_other_than_one() {
    let router = standard_router().await;

    for protocol in ["0", "2", "255", "256", "300", "4294967296", "-1"] {
        let body =
            format!(r#"{{"protocol":{protocol},"service":"smtp","key":"api-key","field":null}}"#);
        let reply = send(&router, resolve_with(body), peer(AUTHORISED_UID)).await;
        assert_error(
            &reply,
            StatusCode::BAD_REQUEST,
            "UNSUPPORTED_PROTOCOL",
            &format!("protocol {protocol}"),
        );
    }
}

#[tokio::test]
async fn resolve_accepts_fields_with_underscores_and_digits_after_the_first_character() {
    let router = router_for("policy-field.toml", &field_seeds()).await;

    let reply = send(
        &router,
        resolve_json("ssh-demo", "test-host", Some("private_key")),
        peer(AUTHORISED_UID),
    )
    .await;

    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.text());
    assert_eq!(reply.text(), golden("resolve-response.json"));
}

#[tokio::test]
async fn resolve_enforces_the_media_type_the_method_and_the_route_table() {
    let router = standard_router().await;
    let valid = golden("resolve-request.json");
    let plain = Request::builder()
        .method("POST")
        .uri("/v1/resolve")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(valid.clone()))
        .unwrap();
    let untyped = Request::builder()
        .method("POST")
        .uri("/v1/resolve")
        .body(Body::from(valid.clone()))
        .unwrap();
    let query = Request::builder()
        .method("POST")
        .uri("/v1/resolve?x=1")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(valid))
        .unwrap();
    let post_entries = Request::builder()
        .method("POST")
        .uri("/v1/entries")
        .body(Body::empty())
        .unwrap();
    let delete_resolve = Request::builder()
        .method("DELETE")
        .uri("/v1/resolve")
        .body(Body::empty())
        .unwrap();
    let post_healthz = Request::builder()
        .method("POST")
        .uri("/healthz")
        .body(Body::empty())
        .unwrap();

    let cases = vec![
        (
            "text/plain body",
            plain,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "UNSUPPORTED_MEDIA_TYPE",
        ),
        (
            "no content type",
            untyped,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "UNSUPPORTED_MEDIA_TYPE",
        ),
        (
            "query string on resolve",
            query,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        ),
        (
            "GET on resolve",
            get("/v1/resolve"),
            StatusCode::METHOD_NOT_ALLOWED,
            "METHOD_NOT_ALLOWED",
        ),
        (
            "DELETE on resolve",
            delete_resolve,
            StatusCode::METHOD_NOT_ALLOWED,
            "METHOD_NOT_ALLOWED",
        ),
        (
            "POST on entries",
            post_entries,
            StatusCode::METHOD_NOT_ALLOWED,
            "METHOD_NOT_ALLOWED",
        ),
        (
            "POST on healthz",
            post_healthz,
            StatusCode::METHOD_NOT_ALLOWED,
            "METHOD_NOT_ALLOWED",
        ),
        (
            "unknown v1 route",
            get("/v1/secrets"),
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
        ),
        (
            "query string on entries",
            get("/v1/entries?service=smtp"),
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        ),
        (
            "query string on healthz",
            get("/healthz?verbose=1"),
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        ),
    ];

    for (case, request, status, code) in cases {
        let reply = send(&router, request, peer(AUTHORISED_UID)).await;
        assert_error(&reply, status, code, case);
    }
}

#[tokio::test]
async fn read_only_routes_reject_a_request_body() {
    let router = standard_router().await;

    for uri in ["/v1/entries", "/healthz"] {
        let request = Request::builder()
            .method("GET")
            .uri(uri)
            .body(Body::from("unexpected"))
            .unwrap();
        let reply = send(&router, request, peer(AUTHORISED_UID)).await;
        assert_error(&reply, StatusCode::BAD_REQUEST, "INVALID_REQUEST", uri);
    }
}

#[tokio::test]
async fn request_body_is_bounded_at_four_kibibytes() {
    let router = standard_router().await;
    let base = golden("resolve-request.json");
    let padded = |total: usize| format!("{base}{}", " ".repeat(total - base.len()));

    let at_limit = send(&router, resolve_with(padded(4096)), peer(AUTHORISED_UID)).await;
    let over_limit = send(&router, resolve_with(padded(4097)), peer(AUTHORISED_UID)).await;

    assert_eq!(
        at_limit.status,
        StatusCode::OK,
        "4096 bytes: {}",
        at_limit.text()
    );
    assert_error(
        &over_limit,
        StatusCode::PAYLOAD_TOO_LARGE,
        "TOO_LARGE",
        "4097 bytes",
    );
}

#[tokio::test]
async fn selected_value_is_bounded_at_sixty_four_kibibytes() {
    let at_limit = "v".repeat(64 * 1024);
    let over_limit = "v".repeat(64 * 1024 + 1);
    let seeds = vec![
        api_seed("smtp", "api-key", &at_limit),
        api_seed("kleos", "api-key", &over_limit),
    ];
    let router = router_for("policy.toml", &seeds).await;

    let accepted = send(
        &router,
        resolve_json("smtp", "api-key", None),
        peer(AUTHORISED_UID),
    )
    .await;
    let refused = send(
        &router,
        resolve_json("kleos", "api-key", None),
        peer(AUTHORISED_UID),
    )
    .await;

    assert_eq!(accepted.status, StatusCode::OK);
    assert_error(
        &refused,
        StatusCode::PAYLOAD_TOO_LARGE,
        "TOO_LARGE",
        "65537-byte value",
    );
    assert!(!refused.text().contains("vvvv"));
}

#[tokio::test]
async fn a_vault_entry_that_cannot_be_decrypted_yields_an_opaque_internal_error() {
    let mut broken = api_seed("smtp", "api-key", UNAUTHORISED_VALUE);
    broken.key = OTHER_KEY;
    let router = router_for("policy.toml", &[broken]).await;

    let reply = send(
        &router,
        resolve_json("smtp", "api-key", None),
        peer(AUTHORISED_UID),
    )
    .await;

    assert_error(
        &reply,
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL",
        "undecryptable entry",
    );
    assert!(!reply.text().contains(&master_hex()));
}

#[tokio::test(start_paused = true)]
async fn the_seventeenth_concurrent_resolution_is_refused_and_stalled_ones_time_out() {
    let router = standard_router().await;
    let mut stalled = Vec::new();
    for _ in 0..16 {
        let router = router.clone();
        stalled.push(tokio::spawn(async move {
            let body = Body::from_stream(futures::stream::pending::<
                Result<axum::body::Bytes, std::io::Error>,
            >());
            let mut request = Request::builder()
                .method("POST")
                .uri("/v1/resolve")
                .header(header::CONTENT_TYPE, "application/json")
                .body(body)
                .unwrap();
            request.extensions_mut().insert(PeerIdentity {
                uid: AUTHORISED_UID,
                pid: 1,
            });
            router.oneshot(request).await.unwrap().status()
        }));
    }
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }

    let started = tokio::time::Instant::now();
    let seventeenth = send(
        &router,
        resolve_with(golden("resolve-request.json")),
        peer(AUTHORISED_UID),
    )
    .await;

    assert_error(
        &seventeenth,
        StatusCode::SERVICE_UNAVAILABLE,
        "UNAVAILABLE",
        "17th concurrent resolution",
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "the refusal must be immediate, not a timeout"
    );
    for task in stalled {
        assert_eq!(task.await.unwrap(), StatusCode::SERVICE_UNAVAILABLE);
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_secs(3) && elapsed < Duration::from_secs(5),
        "stalled requests must be cut off after three seconds, took {elapsed:?}"
    );

    let recovered = send(
        &router,
        resolve_with(golden("resolve-request.json")),
        peer(AUTHORISED_UID),
    )
    .await;
    assert_eq!(
        recovered.status,
        StatusCode::OK,
        "permits must be released after the timeout"
    );
}

#[tokio::test]
async fn requests_without_peer_credentials_are_denied_on_every_route() {
    let router = standard_router().await;

    for (case, request) in [
        ("resolve", resolve_with(golden("resolve-request.json"))),
        ("entries", get("/v1/entries")),
        ("healthz", get("/healthz")),
    ] {
        let reply = send(&router, request, None).await;
        assert_error(
            &reply,
            StatusCode::FORBIDDEN,
            "DENIED",
            &format!("{case} without peer"),
        );
    }
}

#[tokio::test]
async fn a_foreign_uid_is_denied_on_resolve_and_entries_but_the_broker_uid_may_probe_health() {
    let router = standard_router().await;
    let own_uid = nix::unistd::geteuid().as_raw();
    assert_ne!(
        own_uid, FOREIGN_UID,
        "test needs a uid distinct from the foreign one"
    );
    assert_ne!(
        own_uid, AUTHORISED_UID,
        "test needs a uid distinct from the client one"
    );

    let resolve = send(
        &router,
        resolve_with(golden("resolve-request.json")),
        peer(FOREIGN_UID),
    )
    .await;
    let entries = send(&router, get("/v1/entries"), peer(FOREIGN_UID)).await;
    let health_foreign = send(&router, get("/healthz"), peer(FOREIGN_UID)).await;
    let health_broker = send(&router, get("/healthz"), peer(own_uid)).await;
    let resolve_broker = send(
        &router,
        resolve_with(golden("resolve-request.json")),
        peer(own_uid),
    )
    .await;
    let entries_broker = send(&router, get("/v1/entries"), peer(own_uid)).await;

    assert_error(
        &resolve,
        StatusCode::FORBIDDEN,
        "DENIED",
        "foreign uid resolve",
    );
    assert_error(
        &entries,
        StatusCode::FORBIDDEN,
        "DENIED",
        "foreign uid entries",
    );
    assert_error(
        &health_foreign,
        StatusCode::FORBIDDEN,
        "DENIED",
        "foreign uid healthz",
    );
    assert_eq!(
        health_broker.status,
        StatusCode::OK,
        "the broker uid must reach /healthz"
    );
    assert_error(
        &resolve_broker,
        StatusCode::FORBIDDEN,
        "DENIED",
        "broker uid resolve",
    );
    assert_error(
        &entries_broker,
        StatusCode::FORBIDDEN,
        "DENIED",
        "broker uid entries",
    );
}

#[tokio::test]
async fn forged_identity_headers_neither_grant_nor_alter_access() {
    let router = standard_router().await;
    let forged = [
        ("x-forwarded-for", "127.0.0.1"),
        ("x-forwarded-user", "10001"),
        ("x-peer-uid", "10001"),
        ("x-real-uid", "10001"),
        ("x-uid", "10001"),
        ("x-remote-user", "cred-client"),
        ("forwarded", "for=10001"),
        ("x-forwarded-proto", "https"),
        ("cookie", "uid=10001"),
        ("host", "cred-broker.internal"),
    ];
    let with_headers = |mut request: Request<Body>| {
        for (name, value) in forged {
            request
                .headers_mut()
                .insert(name, header::HeaderValue::from_static(value));
        }
        request
    };

    let denied = send(
        &router,
        with_headers(resolve_with(golden("resolve-request.json"))),
        peer(FOREIGN_UID),
    )
    .await;
    let anonymous = send(
        &router,
        with_headers(resolve_with(golden("resolve-request.json"))),
        None,
    )
    .await;
    let unchanged = send(
        &router,
        with_headers(resolve_with(golden("resolve-request.json"))),
        peer(AUTHORISED_UID),
    )
    .await;

    assert_error(
        &denied,
        StatusCode::FORBIDDEN,
        "DENIED",
        "forged headers from a foreign uid",
    );
    assert_error(
        &anonymous,
        StatusCode::FORBIDDEN,
        "DENIED",
        "forged headers without peer",
    );
    assert_eq!(unchanged.text(), golden("resolve-response.json"));
}

#[tokio::test]
async fn any_authorization_header_is_refused_before_policy_or_peer_checks() {
    let router = standard_router().await;
    let bearer = format!("Bearer {}", master_hex());

    for credential in [
        bearer.as_str(),
        "Bearer synthetic-token",
        "Basic Zm9vOmJhcg==",
        "",
    ] {
        for (route, make) in [
            ("resolve", resolve_with(golden("resolve-request.json"))),
            ("entries", get("/v1/entries")),
            ("healthz", get("/healthz")),
            ("unknown route", get("/anything")),
        ] {
            let mut request = make;
            request.headers_mut().insert(
                header::AUTHORIZATION,
                header::HeaderValue::from_str(credential).unwrap(),
            );
            for peer_case in [peer(AUTHORISED_UID), peer(FOREIGN_UID), None] {
                let reply = send(&router, clone_request(&request), peer_case).await;
                assert_error(
                    &reply,
                    StatusCode::BAD_REQUEST,
                    "INVALID_REQUEST",
                    &format!("{route} with credential {credential:?}"),
                );
                assert!(!reply.text().contains(&master_hex()));
            }
        }
    }
}

fn clone_request(request: &Request<Body>) -> Request<Body> {
    let mut copy = Request::builder()
        .method(request.method().clone())
        .uri(request.uri().clone());
    for (name, value) in request.headers() {
        copy = copy.header(name, value);
    }
    let body = if request.method() == axum::http::Method::POST {
        Body::from(golden("resolve-request.json"))
    } else {
        Body::empty()
    };
    copy.body(body).unwrap()
}

#[tokio::test]
async fn historical_credd_routes_do_not_exist_on_the_broker_router() {
    let router = standard_router().await;
    let bearer = format!("Bearer {}", master_hex());
    let historical = [
        "/health",
        "/secrets",
        "/sync",
        "/secret/smtp/api-key",
        "/resolve/text",
        "/resolve/proxy",
        "/resolve/raw",
        "/agents",
        "/agents/mail",
        "/agents/mail/revoke",
        "/bootstrap/kleos-bearer",
        "/raw",
        "/export",
        "/proxy",
        "/admin",
        "/v1/raw",
        "/v1/export",
        "/v1/proxy",
    ];

    for path in historical {
        for method in ["GET", "POST", "PUT", "DELETE"] {
            let request = Request::builder()
                .method(method)
                .uri(path)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap();
            let reply = send(&router, request, peer(AUTHORISED_UID)).await;
            assert!(
                reply.status == StatusCode::NOT_FOUND
                    || reply.status == StatusCode::METHOD_NOT_ALLOWED,
                "{method} {path} answered {} with {}",
                reply.status,
                reply.text()
            );

            let with_bearer = Request::builder()
                .method(method)
                .uri(path)
                .header(header::AUTHORIZATION, bearer.as_str())
                .body(Body::empty())
                .unwrap();
            let reply = send(&router, with_bearer, peer(AUTHORISED_UID)).await;
            assert_eq!(
                reply.status,
                StatusCode::BAD_REQUEST,
                "{method} {path} with the historical master bearer must not reach any handler: {}",
                reply.text()
            );
        }
    }
}

fn policy_from(text: &str) -> Result<Policy, BrokerConfigError> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.toml");
    fs::write(&path, text).unwrap();
    Policy::from_file(&path)
}

fn one_tool(reference: &str) -> String {
    format!("version = 1\n[tools]\nt = [\"{reference}\"]\n")
}

#[test]
fn policy_rejects_every_malformed_selector() {
    let long_service = format!("{}/api-key", "a".repeat(129));
    let long_field = format!("svc/key@{}", "f".repeat(65));
    let selectors = [
        "github/*",
        "*",
        "*/api-key",
        "github/api-key@*",
        "github/api-key@",
        "/api-key",
        "github/",
        "github",
        "github/api-key/extra",
        "github/api-key@a@b",
        "github/api key",
        "gith ub/api-key",
        "-github/api-key",
        ".github/api-key",
        "github/api-key@field-name",
        "github/api-key@field.name",
        long_service.as_str(),
        long_field.as_str(),
    ];

    for selector in selectors {
        assert!(
            matches!(
                policy_from(&one_tool(selector)),
                Err(BrokerConfigError::InvalidPolicy)
            ),
            "selector {selector:?} must be rejected"
        );
    }
}

#[test]
fn policy_accepts_selectors_inside_the_documented_grammar() {
    let long_service = format!("{}/api-key", "a".repeat(128));
    let long_field = format!("svc/key@{}", "f".repeat(64));
    let selectors = [
        "github/api-key",
        "GitHub/Api-Key",
        "ssh/test-host@private_key",
        "ssh/key@_private",
        "ssh/key@token2",
        "a.b_c-d/e.f_g-h",
        long_service.as_str(),
        long_field.as_str(),
    ];

    for selector in selectors {
        assert!(
            policy_from(&one_tool(selector)).is_ok(),
            "selector {selector:?} must be accepted"
        );
    }
}

#[test]
fn policy_rejects_documents_outside_the_v1_schema() {
    let documents = [
        ("missing version", "[tools]\nt = [\"a/b\"]\n"),
        ("version two", "version = 2\n[tools]\nt = [\"a/b\"]\n"),
        (
            "version as string",
            "version = \"1\"\n[tools]\nt = [\"a/b\"]\n",
        ),
        ("missing tools", "version = 1\n"),
        ("tools as array", "version = 1\ntools = []\n"),
        (
            "extra top-level key",
            "version = 1\nextra = 1\n[tools]\nt = [\"a/b\"]\n",
        ),
        ("empty tool list", "version = 1\n[tools]\nt = []\n"),
        (
            "invalid tool label",
            "version = 1\n[tools]\n\"bad label\" = [\"a/b\"]\n",
        ),
        (
            "empty tool label",
            "version = 1\n[tools]\n\"\" = [\"a/b\"]\n",
        ),
        (
            "duplicate tool key",
            "version = 1\n[tools]\nt = [\"a/b\"]\nt = [\"a/c\"]\n",
        ),
        ("not toml", "this is not toml"),
        ("empty document", ""),
        ("selector as non string", "version = 1\n[tools]\nt = [1]\n"),
    ];

    for (case, text) in documents {
        assert!(
            matches!(policy_from(text), Err(BrokerConfigError::InvalidPolicy)),
            "{case} must be rejected"
        );
    }
}

#[test]
fn policy_refuses_a_missing_file_and_never_becomes_allow_all() {
    let dir = tempfile::tempdir().unwrap();

    let result = Policy::from_file(&dir.path().join("absent.toml"));

    assert!(matches!(result, Err(BrokerConfigError::InvalidPolicy)));
}

#[test]
fn policy_is_bounded_at_two_hundred_fifty_six_distinct_references() {
    let build = |count: usize| {
        let references: Vec<String> = (0..count).map(|n| format!("\"svc{n}/key\"")).collect();
        format!("version = 1\n[tools]\nt = [{}]\n", references.join(", "))
    };

    assert!(
        policy_from(&build(256)).is_ok(),
        "256 references must be accepted"
    );
    assert!(
        matches!(
            policy_from(&build(257)),
            Err(BrokerConfigError::InvalidPolicy)
        ),
        "257 references must be rejected"
    );
}

#[test]
fn policy_counts_a_shared_selector_once_across_tools() {
    let text = "version = 1\n[tools]\na = [\"svc/key\"]\nb = [\"svc/key\", \"svc/key\"]\n";

    assert!(policy_from(text).is_ok());
}

#[test]
fn policy_allows_an_explicit_deny_all_document() {
    assert!(policy_from("version = 1\ntools = {}\n").is_ok());
}

mod real_binary {
    use super::*;

    const BIN: &str = env!("CARGO_BIN_EXE_kleos-credd");

    fn credd(args: &[&str]) -> std::process::Output {
        Command::new(BIN)
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .output()
            .expect("spawn kleos-credd")
    }

    fn exit_code(output: &std::process::Output) -> i32 {
        output
            .status
            .code()
            .unwrap_or_else(|| panic!("kleos-credd was killed by a signal: {:?}", output.status))
    }

    fn require_group_matching_uid() {
        let uid = nix::unistd::geteuid().as_raw();
        let in_group = nix::unistd::getgroups()
            .unwrap()
            .iter()
            .any(|gid| gid.as_raw() == uid)
            || nix::unistd::getegid().as_raw() == uid;
        assert!(
            in_group,
            "the broker chowns its socket to gid == client uid; run as a user that belongs to group {uid}"
        );
    }

    struct Broker {
        child: Child,
        dir: tempfile::TempDir,
        socket: PathBuf,
    }

    impl Broker {
        fn stdout_path(&self) -> PathBuf {
            self.dir.path().join("broker.stdout")
        }

        fn stderr_path(&self) -> PathBuf {
            self.dir.path().join("broker.stderr")
        }

        fn client(&self) -> reqwest::Client {
            reqwest::Client::builder()
                .unix_socket(self.socket.as_path())
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap()
        }

        async fn logs(&mut self) -> String {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let mut text = String::new();
            for path in [self.stdout_path(), self.stderr_path()] {
                text.push_str(&fs::read_to_string(path).unwrap_or_default());
            }
            text
        }
    }

    impl Drop for Broker {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    async fn build_vault(dir: &Path, seeds: &[Seed]) -> PathBuf {
        let vault_dir = dir.join("vault");
        fs::create_dir_all(&vault_dir).unwrap();
        let db_path = vault_dir.join("cred.db");
        let db = Database::connect_encrypted(&db_path.to_string_lossy(), None)
            .await
            .expect("create synthetic vault");
        populate(&db, seeds).await;
        drop(db);
        db_path
    }

    fn broker_command(dir: &Path, policy: &Path, db_path: &Path, socket: &Path) -> Command {
        broker_command_with(dir, policy, db_path, socket, None)
    }

    fn broker_command_with(
        dir: &Path,
        policy: &Path,
        db_path: &Path,
        socket: &Path,
        trace: Option<&Path>,
    ) -> Command {
        let keyfile = dir.join("master.key");
        fs::write(&keyfile, format!("{}\n", master_hex())).unwrap();
        fs::set_permissions(&keyfile, fs::Permissions::from_mode(0o600)).unwrap();
        let mut command = match trace {
            Some(trace) => {
                let mut command = Command::new("strace");
                command
                    .args(["-f", "-qq", "-e", "trace=execve,network", "-o"])
                    .arg(trace)
                    .arg(BIN);
                command
            }
            None => Command::new(BIN),
        };
        command
            .args(["--mode", "broker", "--broker-socket"])
            .arg(socket)
            .arg("--broker-policy")
            .arg(policy)
            .args(["--broker-client-uid"])
            .arg(nix::unistd::geteuid().as_raw().to_string())
            .args(["--auth-mode", "keyfile", "--keyfile"])
            .arg(&keyfile)
            .arg("--db-path")
            .arg(db_path)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", dir)
            .env("XDG_CONFIG_HOME", dir.join("xdg"));
        command
    }

    async fn start(policy: &Path, seeds: &[Seed]) -> Broker {
        start_with(policy, seeds, None).await
    }

    async fn start_with(policy: &Path, seeds: &[Seed], trace: Option<&Path>) -> Broker {
        require_group_matching_uid();
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("cred.sock");
        let db_path = build_vault(dir.path(), seeds).await;
        let mut command = broker_command_with(dir.path(), policy, &db_path, &socket, trace);
        command
            .stdout(Stdio::from(
                fs::File::create(dir.path().join("broker.stdout")).unwrap(),
            ))
            .stderr(Stdio::from(
                fs::File::create(dir.path().join("broker.stderr")).unwrap(),
            ));
        let child = command.spawn().expect("spawn broker");
        let mut broker = Broker { child, dir, socket };
        let client = broker.client();
        for _ in 0..200 {
            if let Some(status) = broker.child.try_wait().unwrap() {
                let stderr = fs::read_to_string(broker.stderr_path()).unwrap_or_default();
                panic!("broker exited early with {status}: {stderr}");
            }
            if broker.socket.exists()
                && client
                    .get("http://cred-broker/healthz")
                    .send()
                    .await
                    .is_ok_and(|response| response.status().is_success())
            {
                return broker;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("broker never became healthy");
    }

    async fn get_text(client: &reqwest::Client, path: &str) -> (u16, String) {
        let response = client
            .get(format!("http://cred-broker{path}"))
            .send()
            .await
            .unwrap();
        (response.status().as_u16(), response.text().await.unwrap())
    }

    #[tokio::test]
    async fn the_real_socket_serves_the_golden_responses_with_the_documented_ownership() {
        let broker = start(&fixture("policy.toml"), &standard_seeds()).await;
        let client = broker.client();

        let (health_status, health) = get_text(&client, "/healthz").await;
        let (entries_status, entries) = get_text(&client, "/v1/entries").await;
        let resolve = client
            .post("http://cred-broker/v1/resolve")
            .header(header::CONTENT_TYPE.as_str(), "application/json")
            .body(golden("resolve-request.json"))
            .send()
            .await
            .unwrap();
        let resolve_status = resolve.status().as_u16();
        let resolve_text = resolve.text().await.unwrap();

        assert_eq!(
            (health_status, health),
            (200, golden("healthz-response.json"))
        );
        assert_eq!(
            (entries_status, entries),
            (200, golden("entries-response.json"))
        );
        assert_eq!(
            (resolve_status, resolve_text),
            (200, golden("resolve-response.json"))
        );
        let metadata = fs::metadata(&broker.socket).unwrap();
        let uid = nix::unistd::geteuid().as_raw();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o660, "socket mode");
        assert_eq!(metadata.uid(), uid, "socket owner");
        assert_eq!(metadata.gid(), uid, "socket group must be the client group");
    }

    #[tokio::test]
    async fn the_real_socket_refuses_authorization_and_historical_routes() {
        let broker = start(&fixture("policy.toml"), &standard_seeds()).await;
        let client = broker.client();

        let bearer = client
            .get("http://cred-broker/v1/entries")
            .header(
                header::AUTHORIZATION.as_str(),
                format!("Bearer {}", master_hex()),
            )
            .send()
            .await
            .unwrap();
        let bearer_status = bearer.status().as_u16();
        let bearer_body = bearer.text().await.unwrap();
        let (secrets_status, secrets_body) = get_text(&client, "/secrets").await;
        let (raw_status, _) = get_text(&client, "/resolve/raw").await;

        assert_eq!(bearer_status, 400, "{bearer_body}");
        assert_eq!(bearer_body, error_envelope("INVALID_REQUEST"));
        assert_eq!(secrets_status, 404, "{secrets_body}");
        assert_eq!(raw_status, 404);
    }

    #[tokio::test]
    async fn neither_the_master_key_nor_any_secret_reaches_the_logs_or_error_bodies() {
        let mut seeds = standard_seeds();
        let mut broken = api_seed("resend", "api-key", UNAUTHORISED_VALUE);
        broken.key = OTHER_KEY;
        seeds.push(broken);
        let mut broker = start(&fixture("policy.toml"), &seeds).await;
        let client = broker.client();
        let bearer = format!("Bearer {}", master_hex());
        let mut bodies = Vec::new();

        for (service, key) in [
            ("smtp", "api-key"),
            ("github", "token"),
            ("resend", "api-key"),
            ("grafana", "api-key"),
        ] {
            let response = client
                .post("http://cred-broker/v1/resolve")
                .header(header::CONTENT_TYPE.as_str(), "application/json")
                .body(json!({"protocol":1,"service":service,"key":key,"field":null}).to_string())
                .send()
                .await
                .unwrap();
            bodies.push(response.text().await.unwrap());
        }
        for (method, path) in [
            ("GET", "/v1/entries"),
            ("GET", "/secrets"),
            ("POST", "/v1/resolve"),
        ] {
            let response = client
                .request(method.parse().unwrap(), format!("http://cred-broker{path}"))
                .header(header::AUTHORIZATION.as_str(), bearer.as_str())
                .body("{broken")
                .send()
                .await
                .unwrap();
            bodies.push(response.text().await.unwrap());
        }
        let logs = broker.logs().await;

        let needles = [
            master_hex(),
            master_hex().to_uppercase(),
            UNAUTHORISED_VALUE.to_owned(),
            bearer,
        ];
        for needle in &needles {
            assert!(
                !logs.contains(needle.as_str()),
                "log leaked {needle}: {logs}"
            );
            for body in &bodies {
                assert!(
                    !body.contains(needle.as_str()),
                    "response body leaked {needle}: {body}"
                );
            }
        }
        assert!(
            bodies
                .iter()
                .any(|body| body == &error_envelope("INTERNAL")),
            "the undecryptable entry must surface as the opaque INTERNAL envelope: {bodies:?}"
        );
    }

    #[tokio::test]
    async fn serving_requests_never_spawns_a_process_nor_opens_an_inet_socket() {
        assert!(
            Command::new("strace")
                .arg("-V")
                .output()
                .is_ok_and(|output| output.status.success()),
            "this test traces the broker with strace; install it on the Linux test host"
        );
        let trace_dir = tempfile::tempdir().unwrap();
        let trace = trace_dir.path().join("broker.trace");
        let mut broker = start_with(&fixture("policy.toml"), &standard_seeds(), Some(&trace)).await;
        let client = broker.client();
        let bearer = format!("Bearer {}", master_hex());

        for (service, key) in [
            ("smtp", "api-key"),
            ("github", "token"),
            ("nowhere", "none"),
        ] {
            client
                .post("http://cred-broker/v1/resolve")
                .header(header::CONTENT_TYPE.as_str(), "application/json")
                .body(json!({"protocol":1,"service":service,"key":key,"field":null}).to_string())
                .send()
                .await
                .unwrap();
        }
        for path in ["/v1/entries", "/healthz", "/resolve/raw", "/export"] {
            client
                .get(format!("http://cred-broker{path}"))
                .header(header::AUTHORIZATION.as_str(), bearer.as_str())
                .send()
                .await
                .unwrap();
            client
                .get(format!("http://cred-broker{path}"))
                .send()
                .await
                .unwrap();
        }
        let tracer_pid = broker.child.id().to_string();
        let killed = Command::new("pkill")
            .args(["-KILL", "-P", &tracer_pid])
            .status()
            .unwrap();
        assert!(killed.success(), "the traced broker process was not found");
        broker.child.wait().unwrap();

        let trace_text = fs::read_to_string(&trace).unwrap();
        let spawns = trace_text
            .lines()
            .filter(|line| line.contains("execve("))
            .count();
        assert_eq!(
            spawns, 1,
            "only the broker's own exec is allowed, trace: {trace_text}"
        );
        assert!(
            trace_text.contains("AF_UNIX"),
            "the trace must show the Unix socket activity, otherwise it observed nothing: {trace_text}"
        );
        assert!(
            !trace_text.contains("AF_INET"),
            "the broker opened an inet socket: {trace_text}"
        );
    }

    #[test]
    fn check_config_validates_the_policy_without_touching_the_vault_or_the_socket() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("cred.sock");
        let valid = credd(&[
            "--mode",
            "broker",
            "--check-config",
            "--broker-policy",
            fixture("policy.toml").to_str().unwrap(),
        ]);
        let bad_policy = dir.path().join("wildcard.toml");
        fs::write(&bad_policy, "version = 1\n[tools]\nt = [\"github/*\"]\n").unwrap();
        let invalid = credd(&[
            "--mode",
            "broker",
            "--check-config",
            "--broker-policy",
            bad_policy.to_str().unwrap(),
        ]);

        assert_eq!(exit_code(&valid), 0, "{:?}", valid);
        assert_eq!(exit_code(&invalid), 78, "{:?}", invalid);
        assert!(!socket.exists());
    }

    #[test]
    fn check_config_is_exclusive_with_the_vault_and_socket_options() {
        let output = credd(&[
            "--mode",
            "broker",
            "--check-config",
            "--broker-policy",
            fixture("policy.toml").to_str().unwrap(),
            "--keyfile",
            "/nonexistent/master.key",
        ]);

        assert_eq!(exit_code(&output), 78, "{:?}", output);
    }

    #[test]
    fn healthcheck_reports_unavailable_when_no_broker_listens() {
        let dir = tempfile::tempdir().unwrap();
        let output = credd(&[
            "--mode",
            "broker",
            "--healthcheck",
            "--broker-socket",
            dir.path().join("absent.sock").to_str().unwrap(),
        ]);

        assert_eq!(exit_code(&output), 69, "{:?}", output);
    }

    #[test]
    fn broker_mode_refuses_tcp_and_password_configuration_before_opening_anything() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("cred.sock");
        let policy = fixture("policy.toml");
        let base = [
            "--mode",
            "broker",
            "--broker-policy",
            policy.to_str().unwrap(),
            "--broker-socket",
            socket.to_str().unwrap(),
            "--auth-mode",
            "keyfile",
            "--keyfile",
            "/nonexistent/master.key",
            "--db-path",
            "/nonexistent/cred.db",
        ];
        let mut with_listen = base.to_vec();
        with_listen.extend(["--listen", "127.0.0.1:4400"]);
        let mut with_password = base.to_vec();
        with_password.extend(["--master-password", "synthetic-password"]);
        let default_auth = [
            "--mode",
            "broker",
            "--broker-policy",
            policy.to_str().unwrap(),
            "--broker-socket",
            socket.to_str().unwrap(),
        ];
        let tcp_env = Command::new(BIN)
            .args(base)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("CREDD_BIND", "127.0.0.1:4400")
            .output()
            .unwrap();

        assert_eq!(exit_code(&credd(&with_listen)), 78, "explicit --listen");
        assert_eq!(exit_code(&credd(&with_password)), 78, "--master-password");
        assert_eq!(
            exit_code(&credd(&default_auth)),
            78,
            "default yubikey auth mode"
        );
        assert_eq!(exit_code(&tcp_env), 78, "CREDD_BIND");
        assert!(
            !socket.exists(),
            "a refused configuration must not bind anything"
        );
    }

    #[tokio::test]
    async fn an_invalid_policy_stops_startup_before_the_vault_or_socket_is_opened() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("cred.sock");
        let decoy_db = dir.path().join("decoy.db");
        fs::write(&decoy_db, "sentinel").unwrap();
        let policy = dir.path().join("policy.toml");
        fs::write(&policy, "version = 1\n[tools]\nt = [\"github/*\"]\n").unwrap();
        let output = broker_command(dir.path(), &policy, &decoy_db, &socket)
            .output()
            .unwrap();

        assert_eq!(exit_code(&output), 78, "{:?}", output);
        assert!(!socket.exists());
        assert_eq!(fs::read_to_string(&decoy_db).unwrap(), "sentinel");
    }

    #[tokio::test]
    async fn startup_refuses_a_preexisting_regular_file_or_symlink_at_the_socket_path() {
        require_group_matching_uid();
        let dir = tempfile::tempdir().unwrap();
        let db_path = build_vault(dir.path(), &standard_seeds()).await;
        let socket = dir.path().join("cred.sock");
        fs::write(&socket, "keep me").unwrap();
        let regular = broker_command(dir.path(), &fixture("policy.toml"), &db_path, &socket)
            .output()
            .unwrap();
        fs::remove_file(&socket).unwrap();
        let target = dir.path().join("target.txt");
        fs::write(&target, "keep me too").unwrap();
        std::os::unix::fs::symlink(&target, &socket).unwrap();
        let symlink = broker_command(dir.path(), &fixture("policy.toml"), &db_path, &socket)
            .output()
            .unwrap();

        assert_eq!(exit_code(&regular), 70, "{:?}", regular);
        assert_eq!(exit_code(&symlink), 70, "{:?}", symlink);
        assert_eq!(fs::read_to_string(&target).unwrap(), "keep me too");
        assert!(fs::symlink_metadata(&socket)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[tokio::test]
    async fn a_second_broker_cannot_dislodge_a_live_listener() {
        let broker = start(&fixture("policy.toml"), &standard_seeds()).await;
        let inode_before = fs::metadata(&broker.socket).unwrap().ino();
        let db_path = broker.dir.path().join("vault/cred.db");
        let second = broker_command(
            broker.dir.path(),
            &fixture("policy.toml"),
            &db_path,
            &broker.socket,
        )
        .output()
        .unwrap();

        assert_ne!(exit_code(&second), 0, "{:?}", second);
        assert_eq!(fs::metadata(&broker.socket).unwrap().ino(), inode_before);
        let (status, _) = get_text(&broker.client(), "/healthz").await;
        assert_eq!(status, 200, "the first broker must keep serving");
    }
}
