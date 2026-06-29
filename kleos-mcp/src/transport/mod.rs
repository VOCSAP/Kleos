//! Transport layer re-exports.
//!
//! Conditionally compiles the HTTP transport behind the `http` feature flag;
//! the stdio transport is always available.

/// HTTP transport (feature-gated behind `http`).
#[cfg(feature = "http")]
pub mod http;
/// Stdio JSON-RPC transport with auto-detected framing.
pub mod stdio;
