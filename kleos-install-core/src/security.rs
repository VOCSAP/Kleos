//! Cryptographic key generation utilities for the Kleos installer.
//!
//! All keys are generated using the OS CSPRNG via the `rand` crate and
//! encoded as lowercase hexadecimal strings. None of these functions require
//! a running Kleos server.

use rand::RngCore;

/// Generate a random hex string containing `bytes` bytes of entropy.
///
/// Uses `rand::rng()` (the OS CSPRNG) to fill a buffer, then encodes it as
/// lowercase hex. The returned string has length `bytes * 2`.
pub fn generate_hex_key(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

/// Generate a 256-bit (32-byte / 64-char) encryption key for SQLCipher.
///
/// This key must be kept secret and is stored in the `.env` file, never in
/// the TOML config file.
pub fn generate_encryption_key() -> String {
    generate_hex_key(32)
}

/// Generate a 256-bit pepper for API key hashing.
///
/// The pepper is mixed into the hash input before PBKDF2 / Argon2 so that
/// a database dump alone is insufficient to brute-force API keys.
pub fn generate_api_key_pepper() -> String {
    generate_hex_key(32)
}

/// Generate an initial API key for the first Kleos user.
///
/// The key is 32 random bytes encoded as hex and prefixed with `kleos_` for
/// easy identification in logs and configuration files.
pub fn generate_api_key() -> String {
    format!("kleos_{}", generate_hex_key(32))
}

/// Generate a 256-bit HMAC signing secret.
///
/// Used to sign session tokens and inter-service messages. Stored in `.env`.
pub fn generate_hmac_secret() -> String {
    generate_hex_key(32)
}
