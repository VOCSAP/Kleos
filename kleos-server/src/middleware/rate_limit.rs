use std::collections::HashSet;
use std::path::PathBuf;

use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use kleos_lib::auth::AuthContext;
use kleos_lib::gate::approval_patterns;
use kleos_lib::ratelimit;

use crate::middleware::client_ip::client_ip_key;
use crate::state::AppState;

const OPEN_PATHS: &[&str] = &["/health", "/live", "/ready", "/bootstrap"];

/// Pre-authentication per-IP rate limit (requests per minute). Kept low to
/// resist brute-force auth attempts; authenticated callers use per-key limits.
///
/// Patch 28 (2026-05-23): the const is the upstream-aligned default. Operators
/// can override at runtime via `KLEOS_PREAUTH_IP_LIMIT` (positive integer).
/// Patch 26 confirmed that 20/min is too tight for multi-client dev hosts
/// (sidecar + TUI + CLI + hooks sharing a single source IP); the override
/// unblocks those workloads without changing the safe upstream default.
const DEFAULT_PREAUTH_IP_LIMIT: i64 = 20;

/// Patch 28 (2026-05-23): read `KLEOS_PREAUTH_IP_LIMIT` per-request. Invalid
/// or non-positive values silently fall back to `DEFAULT_PREAUTH_IP_LIMIT` so
/// a typo cannot accidentally disable the preauth rate limit. Read on every
/// request (no `LazyLock`) so the operator can hot-tune via a service reload
/// without a full restart cycle.
fn preauth_ip_limit() -> i64 {
    std::env::var("KLEOS_PREAUTH_IP_LIMIT")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_PREAUTH_IP_LIMIT)
}

/// Patch 28 (2026-05-23): resolve the path of the trusted-IPs whitelist file.
/// Cascade: explicit env var `KLEOS_PREAUTH_IP_TRUSTED_FILE` wins; otherwise
/// auto-resolve to `${KLEOS_DATA_DIR}/preauth_ip_trusted.txt` (mirrors the
/// Patch 19b `gate_data_file` convention but without the `gate/` subdir,
/// since this lives at the server middleware level, not the gate domain).
fn preauth_ip_trusted_file() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("KLEOS_PREAUTH_IP_TRUSTED_FILE") {
        if !v.is_empty() {
            return Some(PathBuf::from(v));
        }
    }
    for env in ["KLEOS_DATA_DIR", "ENGRAM_DATA_DIR"] {
        if let Some(raw) = std::env::var_os(env) {
            if !raw.is_empty() {
                return Some(PathBuf::from(raw).join("preauth_ip_trusted.txt"));
            }
        }
    }
    None
}

/// Patch 28 (2026-05-23): IPs listed in the trusted file bypass the preauth
/// IP middleware entirely. Defense-in-depth on hosts whose source IPs are
/// known and operator-controlled. Reuses the Patch 19b loader
/// (`kleos_lib::gate::approval_patterns::load`) which caches reads for 5s,
/// honours `#` comments and blank lines, and returns `Vec::new()` when the
/// file is absent or empty. We pass `env = None` because the operator chose
/// file-only management (decision 2026-05-23: env var CSV harder to maintain
/// than a one-IP-per-line file).
fn preauth_ip_trusted_set() -> HashSet<String> {
    let path = preauth_ip_trusted_file();
    let patterns = approval_patterns::load(path.as_deref(), None, &[]);
    patterns.into_iter().collect()
}

fn too_many_requests(retry_after: i64) -> Response {
    let body = serde_json::json!({
        "error": "Rate limit exceeded.",
        "retry_after": retry_after,
    });
    axum::response::Response::builder()
        .status(axum::http::StatusCode::TOO_MANY_REQUESTS)
        .header("Content-Type", "application/json")
        .header("Retry-After", retry_after.to_string())
        .body(axum::body::Body::from(body.to_string()))
        // M-R3-004: builder failure must not fall back to empty 200; that
        // would let a client whose request triggered builder failure bypass
        // the rate limit silently. Return 500 instead.
        .unwrap_or_else(|_| {
            axum::response::Response::builder()
                .status(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
                .body(axum::body::Body::empty())
                .expect("static 500 response body")
        })
}

// -- Per-endpoint cost multipliers (3.16) ------------------------------------
//
// Expensive operations consume more rate-limit tokens per request.
// This prevents a caller from burning all their budget on LLM-heavy
// endpoints while keeping cheap reads affordable.

/// Return the cost multiplier for a given request path and method.
/// Default cost is 1 for reads, 2 for writes.
fn endpoint_cost(path: &str, method: &axum::http::Method) -> i64 {
    // Admin table-scale operations. These walk the entire corpus or
    // rebuild an ANN index; a single call can cost minutes of CPU +
    // embedding + vector-index work. Charging the full budget prevents
    // an admin-scoped API key from fire-hosing these endpoints.
    if path.starts_with("/admin/reembed") || path.starts_with("/admin/vector/rebuild-index") {
        return 100;
    }
    if path.starts_with("/admin/rebuild-fts") || path.starts_with("/admin/pagerank/rebuild") {
        return 50;
    }

    // Context assembly -- involves search + embedding + LLM inference
    if path.starts_with("/context") {
        return 5;
    }
    // Batch operations -- up to 100 sub-ops
    if path.starts_with("/batch") {
        return 10;
    }
    // Ingestion -- embedding + chunking
    if path.starts_with("/ingest") {
        return 3;
    }
    // Search -- embedding + reranking
    if path.starts_with("/search") || path.starts_with("/memories/search") {
        return 2;
    }
    // Graph pagerank recompute
    if path.starts_with("/graph/pagerank") && *method == axum::http::Method::POST {
        return 3;
    }
    // Store/update memory -- embedding + indexing
    if path.starts_with("/memories")
        && (*method == axum::http::Method::POST || *method == axum::http::Method::PUT)
    {
        return 2;
    }
    // Prometheus metrics scrape -- cheap but shouldn't be called rapidly
    if path.starts_with("/metrics") {
        return 1;
    }
    // Default: reads cost 1, writes cost 2
    match *method {
        axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS => 1,
        _ => 2,
    }
}

#[tracing::instrument(skip_all, fields(middleware = "server.preauth_rate_limit"))]
pub async fn preauth_rate_limit_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    if OPEN_PATHS
        .iter()
        .any(|p| path == *p || path.starts_with(&format!("{}/", p)))
    {
        return next.run(request).await;
    }

    let key = client_ip_key(&request, &state.config.trusted_proxies);

    // Patch 28 (2026-05-23): trusted-IP whitelist bypass. `key` is formatted
    // by `client_ip_key` as `ip:<addr>` (see kleos-server/src/middleware/
    // client_ip.rs:62); strip the prefix to match the raw IP form the
    // operator writes in the whitelist file.
    let ip = key.strip_prefix("ip:").unwrap_or(&key);
    if preauth_ip_trusted_set().contains(ip) {
        return next.run(request).await;
    }

    // Patch 28: limit is now read from env on every request (default 20/min
    // upstream-aligned).
    let limit = preauth_ip_limit();
    match ratelimit::check_and_increment(&state.db, &key, limit, 60).await {
        Ok(true) => next.run(request).await,
        Ok(false) => {
            // Patch 26: surface preauth IP rejects (HTTP 429) as WARN so
            // /var/log/kleos-server.log identifies which bucket saturated
            // without requiring rate_limits DB decryption. Patch 28: emit
            // the dynamic limit so the operator can tell which cap was
            // active when the reject fired.
            tracing::warn!(
                bucket = %key,
                limit = limit,
                path = %path,
                "preauth_rate_limit reject (429)"
            );
            too_many_requests(60)
        }
        Err(e) => {
            tracing::error!("preauth rate_limit check failed for {}: {}", key, e);
            let body = serde_json::json!({
                "error": "Rate limit backend unavailable. Retry shortly.",
                "retry_after": 5,
            });
            axum::response::Response::builder()
                .status(axum::http::StatusCode::SERVICE_UNAVAILABLE)
                .header("Content-Type", "application/json")
                .header("Retry-After", "5")
                .body(axum::body::Body::from(body.to_string()))
                .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
        }
    }
}

/// Axum middleware implementing per-user sliding-window rate limiting.
///
/// Uses the DB-backed rate limiter from kleos-lib. The limit (requests/minute)
/// is read from the authenticated API key's `rate_limit` field.
///
/// Per-endpoint cost multipliers (3.16) make expensive operations (context,
/// batch, ingest) consume more rate-limit tokens than cheap reads.
///
/// Returns HTTP 429 with a `Retry-After` header when the limit is exceeded.
/// Open paths and unauthenticated requests bypass the limiter.
#[tracing::instrument(skip_all, fields(middleware = "server.rate_limit"))]
pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();

    // Skip rate limiting for health/bootstrap paths.
    if OPEN_PATHS
        .iter()
        .any(|p| path == *p || path.starts_with(&format!("{}/", p)))
    {
        return next.run(request).await;
    }

    let auth_ctx = request.extensions().get::<AuthContext>().cloned();

    // Patch 20 (2026-05-22): rate-limit bucket is keyed per-API-key
    // (`key:{api_key.id}`) instead of per-user (`user:{user_id}`). The
    // `ApiKey.rate_limit` field is already declared per-key in the auth
    // module; previously the middleware aggregated all keys of the same
    // user into one shared bucket, so a buggy bearer (e.g. a TUI ignoring
    // Retry-After) saturated every other bearer of the same user. The
    // new keying gives each bearer its own independent budget while
    // keeping the declared `rate_limit` value as the limit.
    //
    // Patch 20b (2026-05-22): synthetic AuthContexts produced by
    // `open_access_context()` and `synthetic_key_for_identity_with_scopes()`
    // both stamp `ApiKey.id = 0` (no row backing them in the api_keys
    // table). With the naive Patch 20 keying every synthetic context
    // (open access + every PIV identity holder) would collide on the same
    // `key:0` bucket cross-tenant, which is strictly worse than the
    // pre-Patch 20 per-user keying. When the resolved key id is zero we
    // fall back to a per-user synthetic key so PIV identities and open
    // access remain isolated per tenant.
    let (bucket_key, limit) = match auth_ctx {
        Some(ctx) => {
            let limit = ctx.key.rate_limit as i64;
            let key = if ctx.key.id == 0 {
                format!("synth:user:{}", ctx.user_id)
            } else {
                format!("key:{}", ctx.key.id)
            };
            (key, limit)
        }
        // Unauthenticated requests are handled by auth middleware; pass through here.
        None => return next.run(request).await,
    };

    let key = bucket_key;
    let cost = endpoint_cost(&path, request.method());

    match ratelimit::check_and_increment_by(&state.db, &key, limit, 60, cost).await {
        Ok(true) => next.run(request).await,
        Ok(false) => {
            // Patch 26: surface per-key rejects (HTTP 429) as WARN.
            // bucket discriminates key:N (real API key) from synth:user:N
            // (synthetic AuthContext fallback per Patch 20b).
            tracing::warn!(
                bucket = %key,
                limit = limit,
                cost = cost,
                path = %path,
                "per_key_rate_limit reject (429)"
            );
            too_many_requests(60)
        }
        Err(e) => {
            // SECURITY: fail CLOSED on backend errors for authenticated
            // requests. Previously we passed the request through on error,
            // which turned any flaky query into a rate-limit bypass: an
            // attacker could intentionally poison the rate_limits table (e.g.
            // via heavy write contention) to get unlimited throughput.
            tracing::error!("rate_limit check failed for {}: {}", key, e);
            let body = serde_json::json!({
                "error": "Rate limit backend unavailable. Retry shortly.",
                "retry_after": 5,
            });
            axum::response::Response::builder()
                .status(axum::http::StatusCode::SERVICE_UNAVAILABLE)
                .header("Content-Type", "application/json")
                .header("Retry-After", "5")
                .body(axum::body::Body::from(body.to_string()))
                .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
        }
    }
}
