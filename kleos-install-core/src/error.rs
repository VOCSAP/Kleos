//! Error types for the Kleos installer core library.

use thiserror::Error;

/// All errors that can occur during Kleos installation or upgrade.
#[derive(Debug, Error)]
pub enum InstallError {
    /// A binary or asset download failed.
    #[error("download failed: {0}")]
    Download(String),

    /// The downloaded file's SHA-256 checksum did not match the expected value.
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    Checksum {
        /// Expected SHA-256 hex digest.
        expected: String,
        /// Actual SHA-256 hex digest computed from the downloaded file.
        actual: String,
    },

    /// A filesystem operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Configuration generation or serialization failed.
    #[error("config error: {0}")]
    Config(String),

    /// The current platform is not supported by this installer.
    #[error("unsupported platform: {0}")]
    Platform(String),

    /// A GitHub API request failed or returned unexpected data.
    #[error("GitHub API error: {0}")]
    GitHub(String),

    /// Upgrade detection or execution failed.
    #[error("upgrade error: {0}")]
    Upgrade(String),

    /// The user cancelled the installation.
    #[error("installation cancelled by user")]
    Cancelled,
}
