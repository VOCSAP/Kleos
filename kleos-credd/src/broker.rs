use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
    time::Duration,
};

use axum::{
    body::to_bytes,
    extract::{Extension, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use kleos_cred::{crypto::KEY_SIZE, storage, CredError};
use kleos_lib::db::Database;

use crate::peercred::PeerIdentity;

pub const PROTOCOL_VERSION: u8 = 1;
pub const REQUEST_LIMIT_BYTES: usize = 4 * 1024;
pub const RESPONSE_LIMIT_BYTES: usize = 512 * 1024;
pub const VALUE_LIMIT_BYTES: usize = 64 * 1024;
pub const MAX_POLICY_REFERENCES: usize = 256;
pub const MAX_CONCURRENT_REQUESTS: usize = 16;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Selector {
    pub service: String,
    pub key: String,
    pub field: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Policy {
    selectors: BTreeSet<Selector>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPolicy {
    version: u8,
    tools: BTreeMap<String, Vec<String>>,
}

impl Policy {
    pub fn from_file(path: &Path) -> Result<Self, BrokerConfigError> {
        let input = std::fs::read_to_string(path).map_err(|_| BrokerConfigError::InvalidPolicy)?;
        let raw: RawPolicy =
            toml::from_str(&input).map_err(|_| BrokerConfigError::InvalidPolicy)?;
        if raw.version != PROTOCOL_VERSION {
            return Err(BrokerConfigError::InvalidPolicy);
        }

        let mut selectors = BTreeSet::new();
        for (tool, references) in raw.tools {
            if !valid_component(&tool, 128, false) || references.is_empty() {
                return Err(BrokerConfigError::InvalidPolicy);
            }
            for reference in references {
                let selector =
                    parse_policy_selector(&reference).ok_or(BrokerConfigError::InvalidPolicy)?;
                selectors.insert(selector);
                if selectors.len() > MAX_POLICY_REFERENCES {
                    return Err(BrokerConfigError::InvalidPolicy);
                }
            }
        }

        Ok(Self { selectors })
    }

    fn allows(&self, service: &str, key: &str, field: Option<&str>) -> bool {
        self.selectors.contains(&Selector {
            service: service.to_owned(),
            key: key.to_owned(),
            field: field.map(ToOwned::to_owned),
        })
    }

    fn entries(&self) -> impl Iterator<Item = &Selector> {
        self.selectors.iter()
    }
}

fn parse_policy_selector(reference: &str) -> Option<Selector> {
    let (service, suffix) = reference.split_once('/')?;
    if suffix.contains('/') {
        return None;
    }
    let (key, field) = match suffix.split_once('@') {
        Some((key, field)) if !field.contains('@') => (key, Some(field)),
        Some(_) => return None,
        None => (suffix, None),
    };
    if !valid_component(service, 128, false)
        || !valid_component(key, 128, false)
        || field.is_some_and(|field| !valid_component(field, 64, true))
    {
        return None;
    }
    Some(Selector {
        service: service.to_owned(),
        key: key.to_owned(),
        field: field.map(ToOwned::to_owned),
    })
}

fn valid_component(value: &str, max_len: usize, field: bool) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if value.len() > max_len || !first.is_ascii_alphanumeric() && !(field && first == '_') {
        return false;
    }
    chars.all(|character| {
        character.is_ascii_alphanumeric()
            || (!field && matches!(character, '.' | '_' | '-'))
            || (field && character == '_')
    })
}

#[derive(Debug, thiserror::Error)]
pub enum BrokerConfigError {
    #[error("invalid broker policy")]
    InvalidPolicy,
    #[error("broker mode requires keyfile authentication")]
    InvalidAuthMode,
    #[error("broker mode requires an absolute socket path")]
    InvalidSocket,
    #[error("broker mode requires a keyfile")]
    MissingKeyfile,
    #[error("broker mode is unavailable on this platform")]
    UnsupportedPlatform,
}

#[derive(Clone)]
pub struct BrokerState {
    db: Arc<Database>,
    master_key: Arc<[u8; KEY_SIZE]>,
    policy: Arc<Policy>,
    client_uid: u32,
    permits: Arc<Semaphore>,
}

impl BrokerState {
    pub fn new(db: Database, master_key: [u8; KEY_SIZE], policy: Policy, client_uid: u32) -> Self {
        Self {
            db: Arc::new(db),
            master_key: Arc::new(master_key),
            policy: Arc::new(policy),
            client_uid,
            permits: Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS)),
        }
    }

    fn is_health_uid(&self, peer: PeerIdentity) -> bool {
        #[cfg(unix)]
        {
            peer.uid == self.client_uid || peer.uid == nix::unistd::geteuid().as_raw()
        }
        #[cfg(not(unix))]
        {
            peer.uid == self.client_uid
        }
    }

    fn require_permit(&self) -> Result<OwnedSemaphorePermit, BrokerError> {
        self.permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| BrokerError::unavailable())
    }
}

#[derive(Serialize)]
struct ErrorEnvelope {
    protocol: u8,
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
}

struct BrokerError {
    status: StatusCode,
    code: &'static str,
}

impl BrokerError {
    const fn invalid_request() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "INVALID_REQUEST",
        }
    }

    const fn unsupported_protocol() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "UNSUPPORTED_PROTOCOL",
        }
    }

    const fn denied() -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "DENIED",
        }
    }

    const fn not_found() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "NOT_FOUND",
        }
    }

    const fn method_not_allowed() -> Self {
        Self {
            status: StatusCode::METHOD_NOT_ALLOWED,
            code: "METHOD_NOT_ALLOWED",
        }
    }

    const fn too_large() -> Self {
        Self {
            status: StatusCode::PAYLOAD_TOO_LARGE,
            code: "TOO_LARGE",
        }
    }

    const fn unsupported_media_type() -> Self {
        Self {
            status: StatusCode::UNSUPPORTED_MEDIA_TYPE,
            code: "UNSUPPORTED_MEDIA_TYPE",
        }
    }

    const fn field_unavailable() -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "FIELD_UNAVAILABLE",
        }
    }

    const fn unavailable() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "UNAVAILABLE",
        }
    }

    const fn internal() -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "INTERNAL",
        }
    }
}

impl IntoResponse for BrokerError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorEnvelope {
                protocol: PROTOCOL_VERSION,
                error: ErrorBody { code: self.code },
            }),
        )
            .into_response()
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum NullableField {
    String(String),
    Null(()),
}

impl NullableField {
    fn as_deref(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            Self::Null(()) => None,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolveRequest {
    protocol: u8,
    service: String,
    key: String,
    field: NullableField,
}

#[derive(Serialize)]
struct ResolveResponse {
    protocol: u8,
    value: String,
}

#[derive(Serialize)]
struct EntriesResponse {
    protocol: u8,
    entries: Vec<Entry>,
}

#[derive(Serialize)]
struct Entry {
    service: String,
    key: String,
    #[serde(rename = "type")]
    secret_type: String,
}

#[derive(Deserialize, Serialize)]
struct HealthResponse {
    protocol: u8,
    status: String,
}

pub fn router(state: BrokerState) -> Router {
    Router::new()
        .route("/v1/resolve", post(resolve))
        .route("/v1/entries", get(entries))
        .route("/healthz", get(healthz))
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            reject_authorization,
        ))
        .layer(middleware::from_fn(process_timeout))
        .with_state(state)
}

async fn process_timeout(request: Request, next: Next) -> Response {
    match tokio::time::timeout(Duration::from_secs(3), next.run(request)).await {
        Ok(response) => response,
        Err(_) => BrokerError::unavailable().into_response(),
    }
}

async fn reject_authorization(request: Request, next: Next) -> Response {
    if request.headers().contains_key(header::AUTHORIZATION) {
        return BrokerError::invalid_request().into_response();
    }
    next.run(request).await
}

async fn resolve(
    State(state): State<BrokerState>,
    peer: Option<Extension<PeerIdentity>>,
    request: Request,
) -> Result<Response, BrokerError> {
    let peer = peer.ok_or_else(BrokerError::denied)?.0;
    if peer.uid != state.client_uid {
        return Err(BrokerError::denied());
    }
    if request.uri().query().is_some() {
        return Err(BrokerError::invalid_request());
    }
    let _permit = state.require_permit()?;
    require_json(request.headers())?;
    let bytes = to_bytes(request.into_body(), REQUEST_LIMIT_BYTES)
        .await
        .map_err(|_| BrokerError::too_large())?;
    let input: ResolveRequest =
        serde_json::from_slice(&bytes).map_err(|_| BrokerError::invalid_request())?;
    if input.protocol != PROTOCOL_VERSION {
        return Err(BrokerError::unsupported_protocol());
    }
    if !valid_component(&input.service, 128, false)
        || !valid_component(&input.key, 128, false)
        || input
            .field
            .as_deref()
            .is_some_and(|field| !valid_component(field, 64, true))
    {
        return Err(BrokerError::invalid_request());
    }
    if !state
        .policy
        .allows(&input.service, &input.key, input.field.as_deref())
    {
        return Err(BrokerError::denied());
    }

    let (_, secret) = storage::get_secret(
        &state.db,
        1,
        &input.service,
        &input.key,
        state.master_key.as_ref(),
    )
    .await
    .map_err(map_storage_error)?;
    let value = match input.field.as_deref() {
        Some(field) => secret.get_field(field),
        None => secret.bare_value(),
    }
    .ok_or_else(BrokerError::field_unavailable)?;
    if value.len() > VALUE_LIMIT_BYTES {
        return Err(BrokerError::too_large());
    }

    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(ResolveResponse {
            protocol: PROTOCOL_VERSION,
            value,
        }),
    )
        .into_response())
}

async fn entries(
    State(state): State<BrokerState>,
    peer: Option<Extension<PeerIdentity>>,
    request: Request,
) -> Result<Json<EntriesResponse>, BrokerError> {
    let peer = peer.ok_or_else(BrokerError::denied)?.0;
    if peer.uid != state.client_uid {
        return Err(BrokerError::denied());
    }
    if request.uri().query().is_some() {
        return Err(BrokerError::invalid_request());
    }
    if !to_bytes(request.into_body(), REQUEST_LIMIT_BYTES)
        .await
        .map_err(|_| BrokerError::too_large())?
        .is_empty()
    {
        return Err(BrokerError::invalid_request());
    }
    let _permit = state.require_permit()?;
    let secrets = storage::list_secrets(&state.db, 1, None)
        .await
        .map_err(map_storage_error)?;
    let mut known = BTreeMap::new();
    for secret in secrets {
        known.insert((secret.category, secret.name), secret.secret_type);
    }
    let references = state
        .policy
        .entries()
        .map(|selector| (selector.service.as_str(), selector.key.as_str()))
        .collect::<BTreeSet<_>>();
    let entries = references
        .into_iter()
        .filter_map(|(service, key)| {
            known
                .get(&(service.to_owned(), key.to_owned()))
                .map(|secret_type| Entry {
                    service: service.to_owned(),
                    key: key.to_owned(),
                    secret_type: secret_type.as_str().to_owned(),
                })
        })
        .collect();
    Ok(Json(EntriesResponse {
        protocol: PROTOCOL_VERSION,
        entries,
    }))
}

async fn healthz(
    State(state): State<BrokerState>,
    peer: Option<Extension<PeerIdentity>>,
    request: Request,
) -> Result<Json<HealthResponse>, BrokerError> {
    let peer = peer.ok_or_else(BrokerError::denied)?.0;
    if !state.is_health_uid(peer) {
        return Err(BrokerError::denied());
    }
    if request.uri().query().is_some() {
        return Err(BrokerError::invalid_request());
    }
    if !to_bytes(request.into_body(), REQUEST_LIMIT_BYTES)
        .await
        .map_err(|_| BrokerError::too_large())?
        .is_empty()
    {
        return Err(BrokerError::invalid_request());
    }
    Ok(Json(HealthResponse {
        protocol: PROTOCOL_VERSION,
        status: "ready".to_owned(),
    }))
}

async fn not_found() -> BrokerError {
    BrokerError::not_found()
}

async fn method_not_allowed() -> BrokerError {
    BrokerError::method_not_allowed()
}

fn require_json(headers: &HeaderMap) -> Result<(), BrokerError> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(BrokerError::unsupported_media_type)?;
    if content_type.split(';').next() == Some("application/json") {
        Ok(())
    } else {
        Err(BrokerError::unsupported_media_type())
    }
}

fn map_storage_error(error: CredError) -> BrokerError {
    match error {
        CredError::NotFound(_) => BrokerError::not_found(),
        _ => BrokerError::internal(),
    }
}

#[cfg(unix)]
pub async fn listen(socket_path: &Path, client_group: u32, app: Router) -> anyhow::Result<()> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};

    match std::fs::symlink_metadata(socket_path) {
        Ok(metadata) => {
            if !metadata.file_type().is_socket()
                || metadata.uid() != nix::unistd::geteuid().as_raw()
            {
                anyhow::bail!("refusing pre-existing broker socket");
            }
            if tokio::net::UnixStream::connect(socket_path).await.is_ok() {
                anyhow::bail!("broker socket already has an active listener");
            }
            std::fs::remove_file(socket_path)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    let listener = tokio::net::UnixListener::bind(socket_path)?;
    nix::unistd::chown(
        socket_path,
        None,
        Some(nix::unistd::Gid::from_raw(client_group)),
    )?;
    std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o660))?;

    let metadata = std::fs::metadata(socket_path)?;
    if metadata.uid() != nix::unistd::geteuid().as_raw()
        || metadata.gid() != client_group
        || metadata.mode() & 0o777 != 0o660
    {
        anyhow::bail!("broker socket ownership or mode is invalid");
    }

    loop {
        let (stream, _) = listener.accept().await?;
        let peer = stream.peer_cred().ok().map(|credentials| PeerIdentity {
            uid: credentials.uid(),
            pid: credentials.pid().unwrap_or(0),
        });
        let app = app.clone();
        tokio::spawn(async move {
            use tower::ServiceExt;

            let service =
                tower::service_fn(move |mut request: hyper::Request<hyper::body::Incoming>| {
                    if let Some(peer) = peer {
                        request.extensions_mut().insert(peer);
                    }
                    let app = app.clone();
                    async move {
                        Ok::<_, std::convert::Infallible>(
                            app.oneshot(request.map(axum::body::Body::new)).await,
                        )
                    }
                });
            let mut builder = hyper::server::conn::http1::Builder::new();
            builder.timer(hyper_util::rt::TokioTimer::new());
            builder.header_read_timeout(Duration::from_secs(5));
            let service = hyper_util::service::TowerToHyperService::new(service);
            let _ = builder
                .serve_connection(hyper_util::rt::TokioIo::new(stream), service)
                .await;
        });
    }
}

#[cfg(unix)]
pub async fn healthcheck(socket_path: &Path) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .unix_socket(socket_path)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()?;
    let response = client.get("http://cred-broker/healthz").send().await?;
    if response.status() != StatusCode::OK {
        anyhow::bail!("broker healthcheck returned non-200 status");
    }
    let body: HealthResponse = response.json().await?;
    if body.protocol != PROTOCOL_VERSION || body.status != "ready" {
        anyhow::bail!("broker healthcheck returned an invalid response");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_accepts_exact_references_and_deduplicates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("policy.toml");
        std::fs::write(
            &path,
            "version = 1\n[tools]\nmail = [\"smtp/api-key\", \"smtp/api-key\"]\nssh = [\"ssh/key@private_key\"]\n",
        )
        .unwrap();

        let policy = Policy::from_file(&path).unwrap();
        assert!(policy.allows("smtp", "api-key", None));
        assert!(policy.allows("ssh", "key", Some("private_key")));
        assert!(!policy.allows("ssh", "key", None));
        assert_eq!(policy.entries().count(), 2);
    }

    #[test]
    fn policy_rejects_wildcards_and_unknown_top_level_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("policy.toml");
        std::fs::write(
            &path,
            "version = 1\nextra = true\n[tools]\na = [\"github/*\"]\n",
        )
        .unwrap();
        assert!(matches!(
            Policy::from_file(&path),
            Err(BrokerConfigError::InvalidPolicy)
        ));
    }

    #[test]
    fn policy_rejects_empty_tool_references() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("policy.toml");
        std::fs::write(&path, "version = 1\n[tools]\na = []\n").unwrap();
        assert!(matches!(
            Policy::from_file(&path),
            Err(BrokerConfigError::InvalidPolicy)
        ));
    }

    #[test]
    fn policy_allows_an_explicit_deny_all() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("policy.toml");
        std::fs::write(&path, "version = 1\ntools = {}\n").unwrap();
        assert_eq!(Policy::from_file(&path).unwrap().entries().count(), 0);
    }

    #[test]
    fn resolve_schema_rejects_missing_unknown_and_duplicate_fields() {
        assert!(serde_json::from_str::<ResolveRequest>(
            r#"{"protocol":1,"service":"mail","key":"api-key"}"#,
        )
        .is_err());
        assert!(serde_json::from_str::<ResolveRequest>(
            r#"{"protocol":1,"service":"mail","key":"api-key","field":null,"extra":true}"#,
        )
        .is_err());
        assert!(serde_json::from_str::<ResolveRequest>(
            r#"{"protocol":1,"service":"mail","service":"other","key":"api-key","field":null}"#,
        )
        .is_err());
    }

    #[test]
    fn selector_validation_matches_contract_grammar() {
        assert!(parse_policy_selector("memory-hub/api-key").is_some());
        assert!(parse_policy_selector("ssh/key@private_key").is_some());
        assert!(parse_policy_selector("memory-hub/api-key@field.with.dot").is_none());
        assert!(parse_policy_selector("memory-hub/api-key@*").is_none());
        assert!(parse_policy_selector("memory-hub/api-key/extra").is_none());
    }

    #[tokio::test]
    async fn resolve_rejects_a_query_before_reading_the_body() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;

        let state = BrokerState::new(
            Database::connect_memory().await.unwrap(),
            [0; KEY_SIZE],
            Policy {
                selectors: BTreeSet::new(),
            },
            10001,
        );
        let mut request = Request::builder()
            .method("POST")
            .uri("/v1/resolve?unexpected=true")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(vec![b'x'; REQUEST_LIMIT_BYTES + 1]))
            .unwrap();
        request
            .extensions_mut()
            .insert(PeerIdentity { uid: 10001, pid: 1 });

        let response = router(state).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
