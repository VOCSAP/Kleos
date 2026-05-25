//! Phylax route tree -- extends credd's base router with /phylax/* endpoints.

use axum::routing::{delete, get, post, put};
use axum::Router;

use crate::handlers::{approvals, ecdh, leases, namespaces, policies, ssh};
use crate::state::PhylaxState;

/// Build the Phylax extension routes. These are merged into the credd
/// router by the phylaxd binary.
pub fn phylax_routes() -> Router<PhylaxState> {
    Router::new()
        // Approval workflows
        .route("/phylax/approvals", post(approvals::request_approval))
        .route("/phylax/approvals", get(approvals::list_approvals))
        .route("/phylax/approvals/{id}", get(approvals::get_approval))
        .route("/phylax/approvals/{id}", put(approvals::decide_approval))
        .route(
            "/phylax/approvals/{id}/wait",
            get(approvals::wait_for_decision),
        )
        // Leases
        .route("/phylax/leases", get(leases::list_leases))
        .route("/phylax/leases/{jti}/redeem", post(leases::redeem_lease))
        // Access policies
        .route("/phylax/policies", get(policies::list_policies))
        .route("/phylax/policies", post(policies::create_policy))
        .route("/phylax/policies/{id}", put(policies::update_policy))
        .route("/phylax/policies/{id}", delete(policies::delete_policy))
        // Namespaces
        .route("/phylax/namespaces", get(namespaces::list_namespaces))
        // ECDH
        .route("/phylax/ecdh/challenge", post(ecdh::issue_challenge))
        .route("/phylax/ecdh/enroll", post(ecdh::enroll_pubkey))
        .route("/phylax/ecdh/revoke", post(ecdh::revoke_pubkey))
        // SSH key settings
        .route(
            "/phylax/ssh/{category}/{name}",
            get(ssh::get_settings).put(ssh::update_settings),
        )
}
