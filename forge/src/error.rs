//! Forge error types.

/// All errors the forge binary can produce.
#[derive(Debug, thiserror::Error)]
pub enum ForgeError {
    /// Authentication failed or no valid credentials found.
    #[error("auth: {0}")]
    Auth(String),

    /// The requested skill does not exist in the dispatch table.
    #[error("unknown skill '{0}' -- run `forge exec --list` to see available skills")]
    UnknownSkill(String),

    /// The skill exists but is currently disabled.
    #[error("skill '{0}' is disabled")]
    DisabledSkill(String),

    /// A required parameter was not provided and has no default.
    #[error("missing required parameter: --{0}")]
    MissingParam(String),

    /// A parameter was passed that the skill does not accept.
    #[error("unknown parameter: --{0}")]
    UnknownParam(String),

    /// A parameter value could not be parsed as the expected type.
    #[error("invalid value for --{0}: {1}")]
    InvalidParam(String, String),

    /// The server returned a non-2xx status.
    #[error("server error (HTTP {0}): {1}")]
    Server(u16, String),

    /// Could not connect to the Kleos server.
    #[error("connection failed: {0}")]
    Connection(String),

    /// HTTP or IO transport error.
    #[error("{0}")]
    Transport(#[from] reqwest::Error),

    /// JSON serialization/deserialization error.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Crate-local result alias.
pub type Result<T> = std::result::Result<T, ForgeError>;

/// Map a ForgeError to a process exit code.
pub fn exit_code(err: &ForgeError) -> i32 {
    match err {
        ForgeError::Auth(_) => 2,
        ForgeError::Server(..) => 3,
        ForgeError::Connection(_) | ForgeError::Transport(_) => 4,
        _ => 1,
    }
}
