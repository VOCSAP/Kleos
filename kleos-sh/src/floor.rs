use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FloorDecision {
    Allow,
    Deny(String),
}

#[derive(Debug)]
pub struct FloorPolicy {
    rules: HashMap<String, Rule>,
}

#[derive(Debug)]
struct Rule {
    verb: String,
    flags_any: HashSet<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPolicy {
    schema: u8,
    deny: Vec<RawRule>,
    scratch: Option<RawScratch>,
    exception: Option<Vec<RawException>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    id: String,
    verb: String,
    flags_any: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawScratch {
    roots: Vec<String>,
    ttl_hours: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawException {
    id: String,
    rule: String,
    verb: String,
    flags_allowed: Vec<String>,
    proof: String,
}

const REQUIRED_RULES: [(&str, &str, &[&str]); 4] = [
    ("rm-recursive", "rm", &["recursive"]),
    ("git-rewrite", "git push", &["force", "delete"]),
    ("git-reset-hard", "git reset", &["hard"]),
    ("git-clean-force", "git clean", &["force"]),
];

impl FloorPolicy {
    pub fn load(path: &Path) -> Result<Self, String> {
        let input = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
        Self::parse(&input)
    }

    pub fn parse(input: &str) -> Result<Self, String> {
        let raw: RawPolicy = toml::from_str(input).map_err(|err| err.to_string())?;
        if raw.schema != 1 || raw.deny.is_empty() {
            return Err("unsupported or empty floor policy".to_string());
        }
        if let Some(scratch) = &raw.scratch {
            if scratch.roots.is_empty() || scratch.ttl_hours != 24 {
                return Err("invalid scratch policy".to_string());
            }
        }
        if let Some(exceptions) = &raw.exception {
            for exception in exceptions {
                if exception.id != "workspace-touched-directory"
                    || exception.rule != "rm-recursive"
                    || exception.verb != "rm"
                    || exception
                        .flags_allowed
                        .iter()
                        .any(|flag| flag != "recursive" && flag != "force")
                    || exception.proof != "workspace_touch_registry"
                {
                    return Err("invalid floor exception".to_string());
                }
            }
        }

        let mut rules = HashMap::new();
        for rule in raw.deny {
            if rule.id.is_empty() || rule.flags_any.is_empty() || rules.contains_key(&rule.id) {
                return Err("invalid floor rule".to_string());
            }
            let allowed =
                allowed_flags(&rule.verb).ok_or_else(|| "unknown floor verb".to_string())?;
            if rule
                .flags_any
                .iter()
                .any(|flag| !allowed.contains(&flag.as_str()))
            {
                return Err("unknown floor flag".to_string());
            }
            rules.insert(
                rule.id,
                Rule {
                    verb: rule.verb,
                    flags_any: rule.flags_any.into_iter().collect(),
                },
            );
        }
        for (id, verb, flags) in REQUIRED_RULES {
            let rule = rules
                .get(id)
                .ok_or_else(|| format!("missing required floor rule {id}"))?;
            if rule.verb != verb || !flags.iter().all(|flag| rule.flags_any.contains(*flag)) {
                return Err(format!("weakened required floor rule {id}"));
            }
        }
        Ok(Self { rules })
    }

    pub fn evaluate(&self, command: &str) -> FloorDecision {
        if command.len() > 256 * 1024 {
            return FloorDecision::Deny("floor-input-too-large".to_string());
        }
        let command = strip_quoted_heredocs(command);
        scan_fragment(self, &command, 0)
            .unwrap_or_else(|| FloorDecision::Deny("floor-opaque-command".to_string()))
    }

    fn rule_hit(&self, verb: &str, flags: &[&str]) -> Option<String> {
        self.rules.iter().find_map(|(id, rule)| {
            (rule.verb == verb && flags.iter().any(|flag| rule.flags_any.contains(*flag)))
                .then(|| id.clone())
        })
    }
}

fn allowed_flags(verb: &str) -> Option<&'static [&'static str]> {
    match verb {
        "rm" => Some(&["recursive", "force"]),
        "git push" => Some(&["force", "delete"]),
        "git reset" => Some(&["hard"]),
        "git clean" => Some(&["force", "directories", "ignored", "only_ignored"]),
        _ => None,
    }
}

pub fn evaluate_payload(
    policy: &FloorPolicy,
    payload: &str,
    floor_file: &Path,
    binary: &Path,
) -> FloorDecision {
    let Ok(payload) = serde_json::from_str::<serde_json::Value>(payload) else {
        return FloorDecision::Deny("floor-invalid-payload".to_string());
    };
    let Some(command) = payload
        .get("tool_input")
        .and_then(|input| input.get("command"))
        .and_then(serde_json::Value::as_str)
        .filter(|command| !command.trim().is_empty())
    else {
        return FloorDecision::Deny("floor-invalid-payload".to_string());
    };
    if payload.get("tool_name").and_then(serde_json::Value::as_str) != Some("Bash") {
        return FloorDecision::Deny("floor-invalid-payload".to_string());
    }
    if targets_protected_path(command, floor_file, binary) {
        return FloorDecision::Deny("floor-self-protection".to_string());
    }
    policy.evaluate(command)
}

fn targets_protected_path(command: &str, floor_file: &Path, binary: &Path) -> bool {
    let floor = normalize_path(&floor_file.to_string_lossy());
    let binary = normalize_path(&binary.to_string_lossy());
    let floor_dir = floor.rsplit_once('/').map(|(dir, _)| dir).unwrap_or(&floor);
    split_commands(command)
        .into_iter()
        .filter_map(tokenize)
        .flatten()
        .any(|token| {
            let token = token.trim_start_matches(['>', '<']);
            let token = token
                .split_once('=')
                .map(|(_, value)| value)
                .unwrap_or(token);
            let candidate = normalize_path(token);
            candidate == floor
                || candidate == binary
                || candidate == floor_dir
                || candidate.starts_with(&format!("{floor_dir}/"))
        })
}

fn normalize_path(path: &str) -> String {
    let path = path
        .strip_prefix("~/")
        .and_then(|suffix| {
            std::env::var("USERPROFILE")
                .ok()
                .map(|home| format!("{home}/{suffix}"))
        })
        .unwrap_or_else(|| path.to_string());
    let mut path = path.replace('\\', "/").to_ascii_lowercase();
    if path.starts_with('/')
        && path.as_bytes().get(2) == Some(&b'/')
        && path.as_bytes().get(1).is_some_and(u8::is_ascii_alphabetic)
    {
        path = format!("{}:{}", &path[1..2], &path[2..]);
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" if parts.is_empty() => parts.push(String::new()),
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part.to_string()),
        }
    }
    parts.join("/").trim_end_matches('/').to_string()
}

fn strip_quoted_heredocs(input: &str) -> String {
    let mut output = Vec::new();
    let mut delimiter = None;
    for line in input.lines() {
        if let Some(expected) = &delimiter {
            if line.trim() == expected {
                delimiter = None;
            }
            continue;
        }
        output.push(line);
        let Some((_, marker)) = line.split_once("<<") else {
            continue;
        };
        let marker = marker.trim_start_matches('-').trim_start();
        let Some(token) = marker.split_whitespace().next() else {
            continue;
        };
        if token.len() >= 2
            && matches!(token.as_bytes()[0], b'\'' | b'"')
            && token.as_bytes().last() == Some(&token.as_bytes()[0])
        {
            delimiter = Some(token[1..token.len() - 1].to_string());
        }
    }
    output.join("\n")
}

fn scan_fragment(policy: &FloorPolicy, input: &str, depth: u8) -> Option<FloorDecision> {
    if depth > 4 {
        return opaque_decision(input);
    }
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut quote = None;
    while i < bytes.len() {
        let byte = bytes[i];
        match quote {
            Some(b'\'') if byte == b'\'' => quote = None,
            Some(b'\'') => {}
            Some(b'"') if byte == b'"' => quote = None,
            Some(b'"') | None if byte == b'$' && bytes.get(i + 1) == Some(&b'(') => {
                let Some(end) = matching_paren(input, i + 2) else {
                    return opaque_decision(&input[i..]);
                };
                if let FloorDecision::Deny(reason) =
                    scan_fragment(policy, &input[i + 2..end], depth + 1)?
                {
                    return Some(FloorDecision::Deny(reason));
                }
                i = end;
            }
            Some(b'"') | None if byte == b'`' => {
                let Some(end) = input[i + 1..].find('`').map(|offset| i + 1 + offset) else {
                    return opaque_decision(&input[i..]);
                };
                if let FloorDecision::Deny(reason) =
                    scan_fragment(policy, &input[i + 1..end], depth + 1)?
                {
                    return Some(FloorDecision::Deny(reason));
                }
                i = end;
            }
            None if byte == b'\'' || byte == b'"' => quote = Some(byte),
            _ => {}
        }
        i += 1;
    }
    if quote.is_some() {
        return opaque_decision(input);
    }
    scan_simple(policy, input)
}

fn matching_paren(input: &str, start: usize) -> Option<usize> {
    let mut nesting = 1usize;
    for (offset, byte) in input.as_bytes()[start..].iter().enumerate() {
        match byte {
            b'(' => nesting += 1,
            b')' => {
                nesting -= 1;
                if nesting == 0 {
                    return Some(start + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn opaque_decision(input: &str) -> Option<FloorDecision> {
    contains_targeted_verb(input)
        .then(|| FloorDecision::Deny("floor-opaque-command".to_string()))
        .or(Some(FloorDecision::Allow))
}

fn contains_targeted_verb(input: &str) -> bool {
    input
        .split(|c: char| {
            !c.is_ascii_alphanumeric() && c != '\\' && c != '.' && c != '/' && c != '_'
        })
        .any(|token| matches!(normalize_verb(token).as_deref(), Some("rm") | Some("git")))
}

fn scan_simple(policy: &FloorPolicy, input: &str) -> Option<FloorDecision> {
    for segment in split_commands(input) {
        let Some(args) = tokenize(segment) else {
            return opaque_decision(segment);
        };
        if args.is_empty() {
            continue;
        }
        let Some(args) = unwrap_wrappers(args) else {
            return opaque_decision(segment);
        };
        if args.is_empty() {
            continue;
        }
        let Some(verb) = normalize_verb(&args[0]) else {
            return opaque_decision(segment);
        };
        let hit = match verb.as_str() {
            "rm" => classify_rm(policy, &args[1..]),
            "git" => classify_git(policy, &args[1..]),
            _ => None,
        };
        if let Some(id) = hit {
            return Some(FloorDecision::Deny(id));
        }
    }
    Some(FloorDecision::Allow)
}

fn split_commands(input: &str) -> Vec<&str> {
    input
        .split(|c| matches!(c, ';' | '|' | '&' | '\n'))
        .collect()
}

fn tokenize(input: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quote = None;
    let mut escaped = false;
    for ch in input.chars() {
        if escaped {
            token.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            escaped = true;
            continue;
        }
        if matches!(ch, '\'' | '"') {
            if quote == Some(ch) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(ch);
            } else {
                token.push(ch);
            }
            continue;
        }
        if ch.is_whitespace() && quote.is_none() {
            if !token.is_empty() {
                tokens.push(std::mem::take(&mut token));
            }
        } else {
            token.push(ch);
        }
    }
    if escaped || quote.is_some() {
        return None;
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    Some(tokens)
}

fn normalize_verb(token: &str) -> Option<String> {
    let token = token.replace('\\', "");
    let name = token.rsplit(['/', '\\']).next()?.to_ascii_lowercase();
    Some(name.strip_suffix(".exe").unwrap_or(&name).to_string())
}

fn unwrap_wrappers(mut args: Vec<String>) -> Option<Vec<String>> {
    for _ in 0..8 {
        let verb = normalize_verb(args.first()?)?;
        match verb.as_str() {
            "nohup" => {
                args.remove(0);
                if args.first().is_some_and(|arg| arg == "--") {
                    args.remove(0);
                }
            }
            "time" | "command" => {
                args.remove(0);
                if args.first().is_some_and(|arg| arg == "-v" || arg == "-V") {
                    return Some(Vec::new());
                }
                if args.first().is_some_and(|arg| arg == "-p" || arg == "--") {
                    args.remove(0);
                }
            }
            "timeout" => {
                args.remove(0);
                while let Some(arg) = args.first() {
                    match arg.as_str() {
                        "--preserve-status" | "--foreground" | "-v" => {
                            args.remove(0);
                        }
                        "-s" | "-k" | "--signal" | "--kill-after" => {
                            if args.len() < 2 {
                                return None;
                            }
                            args.drain(0..2);
                        }
                        value
                            if value.starts_with("--signal=")
                                || value.starts_with("--kill-after=") =>
                        {
                            args.remove(0);
                        }
                        "--" => {
                            args.remove(0);
                            break;
                        }
                        _ => break,
                    }
                }
                if args.is_empty() {
                    return None;
                }
                args.remove(0);
            }
            "nice" => {
                args.remove(0);
                if args.first().is_some_and(|arg| arg == "-n") {
                    if args.len() < 2 {
                        return None;
                    }
                    args.drain(0..2);
                } else if args
                    .first()
                    .is_some_and(|arg| arg.starts_with("--adjustment="))
                {
                    args.remove(0);
                }
                if args.first().is_some_and(|arg| arg == "--") {
                    args.remove(0);
                }
            }
            "stdbuf" => {
                args.remove(0);
                while let Some(arg) = args.first() {
                    match arg.as_str() {
                        "-i" | "-o" | "-e" => {
                            if args.len() < 2 {
                                return None;
                            }
                            args.drain(0..2);
                        }
                        value
                            if value.starts_with("-i")
                                || value.starts_with("-o")
                                || value.starts_with("-e")
                                || value.starts_with("--input=")
                                || value.starts_with("--output=")
                                || value.starts_with("--error=") =>
                        {
                            args.remove(0);
                        }
                        "--" => {
                            args.remove(0);
                            break;
                        }
                        value if value.starts_with('-') => return None,
                        _ => break,
                    }
                }
            }
            "sudo" | "doas" => {
                args.remove(0);
                if args.first().is_some_and(|arg| arg == "--") {
                    args.remove(0);
                } else {
                    return None;
                }
            }
            "rtk" => return unwrap_rtk(args),
            _ => return Some(args),
        }
    }
    None
}

fn unwrap_rtk(mut args: Vec<String>) -> Option<Vec<String>> {
    args.remove(0);
    while args
        .first()
        .is_some_and(|arg| arg == "-v" || arg == "--ultra-compact" || arg == "--skip-env")
    {
        args.remove(0);
    }
    let dispatch = normalize_verb(args.first()?)?;
    match dispatch.as_str() {
        "git" => {
            args.remove(0);
            args.insert(0, "git".to_string());
            Some(args)
        }
        "run" => {
            args.remove(0);
            if args
                .first()
                .is_some_and(|arg| arg == "-c" || arg == "--command")
            {
                if args.len() < 2 {
                    return None;
                }
                tokenize(&args[1])
            } else {
                Some(args)
            }
        }
        "proxy" | "err" | "test" | "summary" => {
            args.remove(0);
            if args.first().is_some_and(|arg| arg == "--") {
                args.remove(0);
            } else if args.first().is_some_and(|arg| arg.starts_with('-')) {
                return None;
            }
            Some(args)
        }
        _ => Some(Vec::new()),
    }
}

fn classify_rm(policy: &FloorPolicy, args: &[String]) -> Option<String> {
    let mut recursive = false;
    for arg in args {
        if arg == "--" {
            break;
        }
        if arg == "--recursive" {
            recursive = true;
        } else if arg.starts_with('-') && !arg.starts_with("--") {
            if arg[1..].chars().any(|ch| ch == 'r' || ch == 'R') {
                recursive = true;
            }
            if arg[1..].chars().any(|ch| !matches!(ch, 'r' | 'R' | 'f')) {
                return Some("floor-opaque-command".to_string());
            }
        } else if arg.starts_with("--") {
            return Some("floor-opaque-command".to_string());
        }
    }
    recursive
        .then(|| policy.rule_hit("rm", &["recursive"]))
        .flatten()
}

fn classify_git(policy: &FloorPolicy, args: &[String]) -> Option<String> {
    let mut args = args;
    while let Some(arg) = args.first() {
        match arg.as_str() {
            "-C" | "-c" => {
                if args.len() < 2 {
                    return Some("floor-opaque-command".to_string());
                }
                args = &args[2..];
            }
            "--git-dir" | "--work-tree" => {
                if args.len() < 2 {
                    return Some("floor-opaque-command".to_string());
                }
                args = &args[2..];
            }
            value
                if value.starts_with("--git-dir=")
                    || value.starts_with("--work-tree=")
                    || value == "--no-pager"
                    || value == "--paginate"
                    || value == "--literal-pathspecs" =>
            {
                args = &args[1..]
            }
            _ => break,
        }
    }
    let (subcommand, rest) = args.split_first()?;
    match normalize_verb(subcommand)?.as_str() {
        "push" => classify_push(policy, rest),
        "reset" => rest
            .iter()
            .any(|arg| arg == "--hard")
            .then(|| policy.rule_hit("git reset", &["hard"]))
            .flatten(),
        "clean" => rest
            .iter()
            .any(|arg| arg == "--force" || (arg.starts_with('-') && arg[1..].contains('f')))
            .then(|| policy.rule_hit("git clean", &["force"]))
            .flatten(),
        _ => None,
    }
}

fn classify_push(policy: &FloorPolicy, args: &[String]) -> Option<String> {
    let mut force = false;
    let mut delete = false;
    for arg in args {
        match arg.as_str() {
            "-f" | "--force" | "--force-if-includes" => force = true,
            "-d" | "--delete" | "--prune" => delete = true,
            "--mirror" => {
                force = true;
                delete = true;
            }
            _ if arg.starts_with("--force-with-lease") => force = true,
            _ if arg.starts_with('-') && !arg.starts_with("--") => {
                if arg[1..].contains('f') {
                    force = true;
                }
                if arg[1..].contains('d') {
                    delete = true;
                }
            }
            _ if arg.starts_with('+') => force = true,
            _ if arg.starts_with(':') => delete = true,
            _ => {}
        }
    }
    let mut flags = Vec::new();
    if force {
        flags.push("force");
    }
    if delete {
        flags.push("delete");
    }
    (!flags.is_empty())
        .then(|| policy.rule_hit("git push", &flags))
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLICY: &str = r#"
schema = 1
[[deny]]
id = "rm-recursive"
verb = "rm"
flags_any = ["recursive"]
[[deny]]
id = "git-rewrite"
verb = "git push"
flags_any = ["force", "delete"]
[[deny]]
id = "git-reset-hard"
verb = "git reset"
flags_any = ["hard"]
[[deny]]
id = "git-clean-force"
verb = "git clean"
flags_any = ["force"]
"#;

    fn policy() -> FloorPolicy {
        FloorPolicy::parse(POLICY).expect("valid fixture")
    }

    #[test]
    fn rejects_policy_missing_a_mandatory_rule() {
        let input = POLICY.replace(
            "[[deny]]\nid = \"git-clean-force\"\nverb = \"git clean\"\nflags_any = [\"force\"]\n",
            "",
        );
        assert!(FloorPolicy::parse(&input).is_err());
    }

    #[test]
    fn rejects_unknown_policy_content() {
        assert!(FloorPolicy::parse(&format!("{POLICY}\nunknown = true")).is_err());
    }

    #[test]
    fn rejects_absent_and_truncated_policy_files() {
        let missing = std::env::temp_dir().join(format!(
            "kleos-sh-floor-missing-{}.toml",
            std::process::id()
        ));
        assert!(FloorPolicy::load(&missing).is_err());
        assert!(FloorPolicy::parse("schema = 1\n[[deny]").is_err());
    }

    #[test]
    fn denies_recursive_rm_in_supported_forms() {
        for command in [
            "rm -r -f /etc",
            "rm -rf /etc",
            "rm -rf \"/etc\"",
            "rm -rf /tmp /etc",
            "RM.exe -rf /etc",
            "r\\m -rf /etc",
        ] {
            assert_eq!(
                policy().evaluate(command),
                FloorDecision::Deny("rm-recursive".into()),
                "{command}"
            );
        }
    }

    #[test]
    fn denies_git_rewrites_with_groups_refspec_and_prune() {
        for command in [
            "git push origin +main",
            "git push origin main -f",
            "git push -qf origin main",
            "git push --prune origin main",
            "GIT PUSH -f origin main",
            "git reset --hard HEAD~1",
            "git clean -fdx",
        ] {
            assert!(
                matches!(policy().evaluate(command), FloorDecision::Deny(_)),
                "{command}"
            );
        }
    }

    #[test]
    fn allows_safe_git_commands() {
        for command in [
            "git push origin main",
            "git reset --soft HEAD~1",
            "git clean -n",
        ] {
            assert_eq!(
                policy().evaluate(command),
                FloorDecision::Allow,
                "{command}"
            );
        }
    }

    #[test]
    fn scans_substitutions_but_not_single_quoted_data() {
        for command in ["echo \"$(rm -rf /etc)\"", "echo `git push -f origin main`"] {
            assert!(
                matches!(policy().evaluate(command), FloorDecision::Deny(_)),
                "{command}"
            );
        }
        assert_eq!(
            policy().evaluate("echo 'rm -rf /etc'"),
            FloorDecision::Allow
        );
        assert_eq!(
            policy().evaluate("cat <<'EOF'\nrm -rf /etc\nEOF"),
            FloorDecision::Allow
        );
    }

    #[test]
    fn unwraps_supported_wrappers_and_rtk_dispatch() {
        for command in [
            "nohup rm -rf /etc",
            "time -p rm -rf /etc",
            "command -- rm -rf /etc",
            "timeout -s KILL 5 rm -rf /etc",
            "nice -n 5 rm -rf /etc",
            "stdbuf -oL rm -rf /etc",
            "sudo -- rm -rf /etc",
            "doas -- rm -rf /etc",
            "rtk git push -f origin main",
            "rtk run -c 'rm -rf /etc'",
            "rtk run rm -rf /etc",
            "rtk proxy rm -rf /etc",
            "rtk err rm -rf /etc",
            "rtk test rm -rf /etc",
            "rtk summary -- rm -rf /etc",
        ] {
            assert!(
                matches!(policy().evaluate(command), FloorDecision::Deny(_)),
                "{command}"
            );
        }
    }

    #[test]
    fn denies_opaque_regions_only_when_they_contain_targeted_verbs() {
        assert_eq!(
            policy().evaluate("echo $(unterminated"),
            FloorDecision::Allow
        );
        assert!(matches!(
            policy().evaluate("echo $(rm -rf /etc"),
            FloorDecision::Deny(_)
        ));
    }

    #[test]
    fn payload_without_cwd_denies_a_recursive_rm() {
        let payload = r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"rm -rf /etc"}}"#;
        assert_eq!(
            evaluate_payload(
                &policy(),
                payload,
                Path::new("C:/gate/floor.toml"),
                Path::new("C:/bin/kleos-sh.exe")
            ),
            FloorDecision::Deny("rm-recursive".into())
        );
    }

    #[test]
    fn rejects_commands_targeting_floor_files() {
        let payload = r#"{"tool_name":"Bash","tool_input":{"command":"type C:/gate/floor.toml"}}"#;
        assert_eq!(
            evaluate_payload(
                &policy(),
                payload,
                Path::new("C:/gate/floor.toml"),
                Path::new("C:/bin/kleos-sh.exe")
            ),
            FloorDecision::Deny("floor-self-protection".into())
        );
    }

    #[test]
    fn rejects_malformed_payloads() {
        assert_eq!(
            evaluate_payload(
                &policy(),
                "{",
                Path::new("C:/gate/floor.toml"),
                Path::new("C:/bin/kleos-sh.exe")
            ),
            FloorDecision::Deny("floor-invalid-payload".into())
        );
    }
}
