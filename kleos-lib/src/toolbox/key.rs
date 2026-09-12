//! Canonical tool keys.
//!
//! A tool is identified by one string shared by every user that indexes it: two
//! people who clone the same repository must land on the same
//! `github.com/owner/repo`, whatever spelling their remote uses (https, scp-like
//! ssh, `ssh://`, with or without credentials, with or without `.git`). That key
//! is what makes the sheet and its embedding shareable.
//!
//! The server always recomputes the key from the raw fields the client sent; the
//! client never supplies one.

use super::types::{KeyInput, KeyKind};
use crate::{EngError, Result};

/// Compute the canonical key and its kind from the raw identity fields.
///
/// Priority: `git_remote` > `url` > `local_path`. At least one must be present.
///
/// * git / url -> `<host>/<path>`, lowercased, no scheme, no credentials, no
///   port, no `.git` suffix, no trailing slash. The path is kept whole for urls
///   (`github.com/a/b/tree/main/x` stays deep -- only the repository root is a
///   git identity).
/// * local -> `local:<host>:<path>`. The path keeps its case (filesystems are
///   case-sensitive); an absent host yields `local::<path>`.
pub fn normalize_tool_key(input: &KeyInput) -> Result<(String, KeyKind)> {
    if let Some(remote) = non_empty(&input.git_remote) {
        return Ok((normalize_remote_like(remote)?, KeyKind::Git));
    }
    if let Some(url) = non_empty(&input.url) {
        return Ok((normalize_remote_like(url)?, KeyKind::Url));
    }
    if let Some(path) = non_empty(&input.local_path) {
        let host = non_empty(&input.host).unwrap_or("").trim().to_lowercase();
        return Ok((
            format!("local:{}:{}", host, normalize_local_path(path)?),
            KeyKind::Local,
        ));
    }
    Err(EngError::InvalidInput(
        "toolbox key needs one of git_remote, url or local_path".to_string(),
    ))
}

/// `Some(trimmed)` when the option holds a non-blank string.
fn non_empty(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// Normalize any remote-ish string (git remote or url) to `<host>/<path>`.
fn normalize_remote_like(raw: &str) -> Result<String> {
    let raw = raw.trim();
    let had_scheme = raw.contains("://");
    let rest = match raw.find("://") {
        Some(i) => &raw[i + 3..],
        None => raw,
    };

    let (authority, path) = if !had_scheme && rest.contains(':') && !looks_like_host_port(rest) {
        // scp-like: git@github.com:Owner/Repo.git
        let idx = rest.find(':').unwrap_or(0);
        (&rest[..idx], &rest[idx + 1..])
    } else {
        match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i + 1..]),
            None => (rest, ""),
        }
    };

    // Drop userinfo (user, user:token) and any explicit port.
    let host = match authority.rfind('@') {
        Some(i) => &authority[i + 1..],
        None => authority,
    };
    let host = strip_port(host).to_lowercase();
    if host.is_empty() {
        return Err(EngError::InvalidInput(format!(
            "toolbox key: no host in {raw:?}"
        )));
    }

    let path = path.trim_matches('/');
    let mut key = if path.is_empty() {
        host
    } else {
        format!("{host}/{path}")
    };
    key = key.to_lowercase();
    if let Some(stripped) = key.strip_suffix(".git") {
        key = stripped.to_string();
    }
    let key = key.trim_end_matches('/').to_string();
    if key.is_empty() {
        return Err(EngError::InvalidInput(format!(
            "toolbox key: {raw:?} normalizes to nothing"
        )));
    }
    Ok(key)
}

/// True when `rest` looks like `host:1234/...` rather than an scp-like
/// `host:path`. Only an all-digit segment after the colon counts as a port.
fn looks_like_host_port(rest: &str) -> bool {
    let Some(idx) = rest.find(':') else {
        return false;
    };
    let after = &rest[idx + 1..];
    let port = after.split('/').next().unwrap_or("");
    !port.is_empty() && port.chars().all(|c| c.is_ascii_digit())
}

/// Remove a trailing `:<digits>` port from a host.
fn strip_port(host: &str) -> &str {
    match host.rfind(':') {
        Some(i)
            if !host[i + 1..].is_empty() && host[i + 1..].chars().all(|c| c.is_ascii_digit()) =>
        {
            &host[..i]
        }
        _ => host,
    }
}

/// Normalize a filesystem path for a `local:` key: backslashes become slashes,
/// repeated slashes collapse, a trailing slash is dropped. Case is preserved.
fn normalize_local_path(raw: &str) -> Result<String> {
    let raw = raw.trim().replace('\\', "/");
    let mut out = String::with_capacity(raw.len());
    let mut prev_slash = false;
    for c in raw.chars() {
        if c == '/' {
            if prev_slash {
                continue;
            }
            prev_slash = true;
        } else {
            prev_slash = false;
        }
        out.push(c);
    }
    while out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    if out.is_empty() {
        return Err(EngError::InvalidInput(
            "toolbox key: empty local_path".to_string(),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(remote: &str) -> (String, KeyKind) {
        normalize_tool_key(&KeyInput {
            git_remote: Some(remote.to_string()),
            ..Default::default()
        })
        .unwrap()
    }

    #[test]
    fn https_remote_drops_scheme_and_git_suffix() {
        let (key, kind) = git("https://github.com/Owner/Repo.git");
        assert_eq!(key, "github.com/owner/repo");
        assert_eq!(kind, KeyKind::Git);
    }

    #[test]
    fn scp_like_ssh_remote_matches_https() {
        assert_eq!(
            git("git@github.com:Owner/Repo.git").0,
            "github.com/owner/repo"
        );
    }

    #[test]
    fn ssh_scheme_remote_matches_https() {
        assert_eq!(
            git("ssh://git@github.com/Owner/Repo").0,
            "github.com/owner/repo"
        );
    }

    #[test]
    fn credentials_are_stripped() {
        assert_eq!(
            git("https://user:ghp_secret@github.com/Owner/Repo.git").0,
            "github.com/owner/repo"
        );
    }

    #[test]
    fn case_is_folded() {
        assert_eq!(
            git("https://GitHub.COM/VOCSAP/Kleos").0,
            "github.com/vocsap/kleos"
        );
    }

    #[test]
    fn trailing_slash_is_dropped() {
        assert_eq!(
            git("https://github.com/Owner/Repo/").0,
            "github.com/owner/repo"
        );
    }

    #[test]
    fn explicit_port_is_dropped_so_protocols_agree() {
        assert_eq!(
            git("ssh://git@git.example.org:2222/team/tool.git").0,
            "git.example.org/team/tool"
        );
    }

    #[test]
    fn url_keeps_its_whole_path() {
        let (key, kind) = normalize_tool_key(&KeyInput {
            url: Some("https://github.com/a/b/tree/main/x/".to_string()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(key, "github.com/a/b/tree/main/x");
        assert_eq!(kind, KeyKind::Url);
    }

    #[test]
    fn local_path_with_host_keeps_case() {
        let (key, kind) = normalize_tool_key(&KeyInput {
            local_path: Some("/home/user/Kleos/".to_string()),
            host: Some("Workstation".to_string()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(key, "local:workstation:/home/user/Kleos");
        assert_eq!(kind, KeyKind::Local);
    }

    #[test]
    fn local_path_without_host_uses_empty_segment() {
        let (key, _) = normalize_tool_key(&KeyInput {
            local_path: Some("C:\\Users\\me\\tools\\".to_string()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(key, "local::C:/Users/me/tools");
    }

    #[test]
    fn git_remote_wins_over_url_and_local_path() {
        let (key, kind) = normalize_tool_key(&KeyInput {
            git_remote: Some("git@github.com:o/r.git".to_string()),
            url: Some("https://example.org/doc".to_string()),
            local_path: Some("/tmp/x".to_string()),
            host: Some("h".to_string()),
        })
        .unwrap();
        assert_eq!(key, "github.com/o/r");
        assert_eq!(kind, KeyKind::Git);
    }

    #[test]
    fn no_identity_field_is_invalid_input() {
        let err = normalize_tool_key(&KeyInput::default()).unwrap_err();
        assert!(matches!(err, EngError::InvalidInput(_)), "got {err:?}");

        // Blank strings count as absent, not as an identity.
        let err = normalize_tool_key(&KeyInput {
            git_remote: Some("   ".to_string()),
            ..Default::default()
        })
        .unwrap_err();
        assert!(matches!(err, EngError::InvalidInput(_)), "got {err:?}");
    }
}
