//! UTF-8 safe string slicing helpers.
//!
//! Centralises the pattern of "truncate this `&str` to at most N bytes
//! without splitting a multi-byte codepoint". Standard `&s[..N]` panics
//! when `N` lands inside a multi-byte char (emoji, `é`, CJK, ...).
//!
//! Pattern miroir de la defense explicite deja en place dans
//! `services/broca.rs:1309-1314`, factorisee ici pour reuse.

/// Return the largest prefix of `s` that fits within `max_bytes` AND
/// ends on a valid UTF-8 char boundary.
///
/// - `max_bytes >= s.len()` -> returns `s` unchanged (zero cost).
/// - `max_bytes == 0`        -> returns `""`.
/// - First codepoint of `s` is larger than `max_bytes` -> returns `""`.
/// - Otherwise returns a prefix `&s[..k]` with `k <= max_bytes` and
///   `s.is_char_boundary(k) == true`.
///
/// Equivalent to `&s[..max_bytes]` for pure-ASCII input but never panics
/// on multilingual text. Use this anywhere a fixed-N truncation feeds
/// into a `format!`, a log, an LLM prompt, or any other consumer that
/// only cares about an approximate size cap rather than an exact one.
pub fn truncate_at_char(s: &str, max_bytes: usize) -> &str {
    if max_bytes >= s.len() {
        return s;
    }
    if max_bytes == 0 {
        return "";
    }
    // Walk codepoints, keep the last byte position whose endpoint
    // (byte_pos + char.len_utf8()) still fits within max_bytes.
    let end = s
        .char_indices()
        .take_while(|(byte_pos, ch)| byte_pos + ch.len_utf8() <= max_bytes)
        .last()
        .map(|(byte_pos, ch)| byte_pos + ch.len_utf8())
        .unwrap_or(0);
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_pure_truncates_at_exact_byte() {
        assert_eq!(truncate_at_char("hello world", 5), "hello");
        assert_eq!(truncate_at_char("hello world", 11), "hello world");
        assert_eq!(truncate_at_char("hello world", 100), "hello world");
    }

    #[test]
    fn empty_string_returns_empty() {
        assert_eq!(truncate_at_char("", 0), "");
        assert_eq!(truncate_at_char("", 100), "");
    }

    #[test]
    fn max_bytes_zero_returns_empty() {
        assert_eq!(truncate_at_char("hello", 0), "");
        assert_eq!(truncate_at_char("café", 0), "");
    }

    #[test]
    fn fr_accent_does_not_split() {
        // `café` = "caf" (3 bytes) + "é" (2 bytes) = 5 bytes total.
        // Truncate at 4 bytes would land inside `é` (bytes 3..5).
        // Helper must back off to byte 3 -> "caf".
        let s = "café";
        assert_eq!(s.len(), 5);
        assert_eq!(truncate_at_char(s, 4), "caf");
        assert_eq!(truncate_at_char(s, 5), "café");
        assert_eq!(truncate_at_char(s, 3), "caf");
        assert_eq!(truncate_at_char(s, 2), "ca");
    }

    #[test]
    fn emoji_4byte_does_not_split() {
        // U+1F600 GRINNING FACE = 4 bytes UTF-8.
        let s = "ab😀cd";
        assert_eq!(s.len(), 8); // 1 + 1 + 4 + 1 + 1
        // Trunc at 3 would land inside the emoji -> back off to "ab".
        assert_eq!(truncate_at_char(s, 3), "ab");
        assert_eq!(truncate_at_char(s, 4), "ab");
        assert_eq!(truncate_at_char(s, 5), "ab"); // still inside emoji
        assert_eq!(truncate_at_char(s, 6), "ab😀");
        assert_eq!(truncate_at_char(s, 7), "ab😀c");
    }

    #[test]
    fn cjk_3byte_does_not_split() {
        // CJK char "中" = 3 bytes UTF-8.
        let s = "x中y";
        assert_eq!(s.len(), 5);
        assert_eq!(truncate_at_char(s, 2), "x");
        assert_eq!(truncate_at_char(s, 3), "x");
        assert_eq!(truncate_at_char(s, 4), "x中");
        assert_eq!(truncate_at_char(s, 5), "x中y");
    }

    #[test]
    fn leading_multibyte_larger_than_cap_returns_empty() {
        // "é" (2 bytes) when max_bytes = 1 -> first char does not fit.
        assert_eq!(truncate_at_char("éé", 1), "");
        // "😀" (4 bytes) when max_bytes < 4 -> empty.
        assert_eq!(truncate_at_char("😀tail", 3), "");
        assert_eq!(truncate_at_char("😀tail", 4), "😀");
    }

    #[test]
    fn returns_full_str_when_cap_equals_or_exceeds_len() {
        let s = "déjà vu";
        assert_eq!(truncate_at_char(s, s.len()), s);
        assert_eq!(truncate_at_char(s, s.len() + 100), s);
    }
}
