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
        if input.len() > 64 * 1024 {
            return Err("floor policy exceeds the 64 KiB budget".to_string());
        }
        let raw: RawPolicy = toml::from_str(input).map_err(|err| err.to_string())?;
        if raw.schema != 1 || raw.deny.is_empty() || raw.deny.len() > 64 {
            return Err("unsupported, empty, or oversized floor policy".to_string());
        }
        if let Some(scratch) = &raw.scratch {
            if scratch.roots.is_empty()
                || scratch.ttl_hours != 24
                || scratch.roots.iter().any(|root| {
                    !Path::new(root).is_absolute()
                        || root.contains('*')
                        || root.contains('?')
                        || root.contains('$')
                        || root.contains('%')
                })
            {
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
        scan_fragment(self, command, 0)
            .unwrap_or_else(|| FloorDecision::Deny("floor-opaque-command".to_string()))
    }

    fn rule_hit(&self, verb: &str, flags: &[&str]) -> Option<String> {
        let mut hits: Vec<_> = self
            .rules
            .iter()
            .filter(|(_, rule)| {
                rule.verb == verb && flags.iter().any(|flag| rule.flags_any.contains(*flag))
            })
            .map(|(id, _)| id.as_str())
            .collect();
        hits.sort_unstable();
        (!hits.is_empty()).then(|| hits.join("+"))
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
    if payload
        .get("hook_event_name")
        .and_then(serde_json::Value::as_str)
        != Some("PreToolUse")
        || payload.get("tool_name").and_then(serde_json::Value::as_str) != Some("Bash")
    {
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
    split_commands(command).into_iter().any(|segment| {
        let Some(tokens) = tokenize(segment) else {
            let candidate = normalize_path(segment);
            return candidate.contains(&floor) || candidate.contains(&binary);
        };
        tokens.into_iter().any(|token| {
            let token = strip_redirection_prefix(&token);
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
    })
}

fn strip_redirection_prefix(token: &str) -> &str {
    let first = token
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(token.len());
    token[first..].trim_start_matches(['>', '<'])
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

fn process_heredocs(policy: &FloorPolicy, input: &str, depth: u8) -> Option<String> {
    let mut output = Vec::new();
    let lines: Vec<_> = input.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let Some((header, marker)) = line.split_once("<<") else {
            output.push(line);
            index += 1;
            continue;
        };
        output.push(header);
        let marker = marker.trim_start_matches('-').trim_start();
        let token = marker.split_whitespace().next()?;
        let quoted = token.len() >= 2
            && matches!(token.as_bytes()[0], b'\'' | b'"')
            && token.as_bytes().last() == Some(&token.as_bytes()[0]);
        let delimiter = if quoted {
            &token[1..token.len() - 1]
        } else {
            token
        };
        let mut body = Vec::new();
        index += 1;
        while index < lines.len() && lines[index].trim() != delimiter {
            body.push(lines[index]);
            index += 1;
        }
        if index == lines.len() {
            return None;
        }
        let interpreter = header
            .trim_start_matches(|ch: char| ch == '(' || ch == '{' || ch.is_whitespace())
            .split_whitespace()
            .next()
            .is_some_and(|verb| {
                matches!(
                    normalize_verb(verb).as_deref(),
                    Some("bash" | "sh" | "zsh" | "eval" | "exec")
                )
            });
        let body = body.join("\n");
        if interpreter {
            if let FloorDecision::Deny(reason) = opaque_decision(&body)? {
                return Some(format!("\0{reason}"));
            }
        } else if !quoted {
            scan_expansions(policy, &body, depth + 1)?;
        }
        index += 1;
    }
    Some(output.join("\n"))
}

fn scan_expansions(policy: &FloorPolicy, input: &str, depth: u8) -> Option<()> {
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'$' && bytes.get(index + 1) == Some(&b'(') {
            let end = matching_paren(input, index + 2)?;
            if matches!(
                scan_fragment(policy, &input[index + 2..end], depth)?,
                FloorDecision::Deny(_)
            ) {
                return None;
            }
            index = end;
        } else if bytes[index] == b'`' {
            let end = input[index + 1..]
                .find('`')
                .map(|offset| index + 1 + offset)?;
            if matches!(
                scan_fragment(policy, &input[index + 1..end], depth)?,
                FloorDecision::Deny(_)
            ) {
                return None;
            }
            index = end;
        }
        index += 1;
    }
    Some(())
}

fn scan_fragment(policy: &FloorPolicy, input: &str, depth: u8) -> Option<FloorDecision> {
    if depth > 4 {
        return opaque_decision(input);
    }
    let input = process_heredocs(policy, input, depth)?;
    if let Some(reason) = input.strip_prefix('\0') {
        return Some(FloorDecision::Deny(reason.to_string()));
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
                let Some(end) = matching_paren(&input, i + 2) else {
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
        return opaque_decision(&input);
    }
    scan_simple(policy, &input)
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
    let segments = split_commands(input);
    if segments.len() > 256 {
        return Some(FloorDecision::Deny("floor-budget-exceeded".to_string()));
    }
    let mut token_count = 0usize;
    for segment in segments {
        let Some(mut args) = tokenize(segment) else {
            return opaque_decision(segment);
        };
        token_count += args.len();
        if token_count > 4096 {
            return Some(FloorDecision::Deny("floor-budget-exceeded".to_string()));
        }
        if args.is_empty() {
            continue;
        }
        if let Some(first) = args.first_mut() {
            let trimmed = first.trim_start_matches(['(', '{']);
            if !trimmed.is_empty() {
                *first = trimmed.to_string();
            }
        }
        while args
            .first()
            .is_some_and(|arg| is_assignment(arg) || arg == "!" || arg == "(" || arg == "{")
        {
            args.remove(0);
        }
        if let Some(index) = args
            .iter()
            .rposition(|arg| matches!(arg.as_str(), "then" | "do"))
        {
            args.drain(0..=index);
        }
        if args.is_empty()
            || matches!(
                args[0].as_str(),
                "fi" | "done" | "else" | "elif" | "if" | "while" | "until" | "for"
            )
        {
            continue;
        }
        if matches!(
            normalize_verb(&args[0]).as_deref(),
            Some("bash" | "sh" | "zsh" | "eval" | "exec" | "env" | "xargs" | "find")
        ) {
            if let FloorDecision::Deny(reason) = opaque_decision(segment)? {
                return Some(FloorDecision::Deny(reason));
            }
            continue;
        }
        if normalize_verb(&args[0]).as_deref() == Some("rtk") {
            if let FloorDecision::Deny(reason) = scan_rtk(policy, &args, 1)? {
                return Some(FloorDecision::Deny(reason));
            }
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
            value if value.starts_with('-') => return opaque_decision(segment),
            _ => None,
        };
        if let Some(id) = hit {
            return Some(FloorDecision::Deny(id));
        }
    }
    Some(FloorDecision::Allow)
}

fn is_assignment(token: &str) -> bool {
    token.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    })
}

fn scan_rtk(policy: &FloorPolicy, args: &[String], depth: u8) -> Option<FloorDecision> {
    let mut args = args.get(1..)?;
    while args.first().is_some_and(|arg| {
        arg.chars().all(|ch| ch == '-' || ch == 'v')
            || matches!(arg.as_str(), "--ultra-compact" | "--skip-env")
    }) {
        args = &args[1..];
    }
    let dispatch = normalize_verb(args.first()?)?;
    let carried = match dispatch.as_str() {
        "git" => format!("git {}", args[1..].join(" ")),
        "run" if matches!(args.get(1).map(String::as_str), Some("-c" | "--command")) => {
            args.get(2)?.to_string()
        }
        "run" => args[1..].join(" "),
        "proxy" | "err" | "test" | "summary" => {
            let args = &args[1..];
            if args
                .first()
                .is_some_and(|arg| arg.starts_with('-') && arg != "--")
            {
                return opaque_decision(&args.join(" "));
            }
            args.strip_prefix(&["--".to_string()])
                .unwrap_or(args)
                .join(" ")
        }
        _ => return opaque_decision(&args.join(" ")),
    };
    scan_fragment(policy, &carried, depth)
}

fn split_commands(input: &str) -> Vec<&str> {
    let bytes = input.as_bytes();
    let mut start = 0;
    let mut index = 0;
    let mut quote = None;
    let mut parens = 0usize;
    let mut segments = Vec::new();
    while index < bytes.len() {
        match quote {
            Some(b'\'') if bytes[index] == b'\'' => quote = None,
            Some(b'\'') => {}
            Some(b'"') if bytes[index] == b'"' => quote = None,
            Some(b'"') => {}
            None if matches!(bytes[index], b'\'' | b'"') => quote = Some(bytes[index]),
            None if bytes[index] == b'$' && bytes.get(index + 1) == Some(&b'(') => {
                parens += 1;
                index += 1;
            }
            None if bytes[index] == b'(' && parens > 0 => parens += 1,
            None if bytes[index] == b')' && parens > 0 => parens -= 1,
            None if parens == 0 && matches!(bytes[index], b';' | b'|' | b'&' | b'\n') => {
                segments.push(&input[start..index]);
                start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    segments.push(&input[start..]);
    segments
}

fn tokenize(input: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quote = None;
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match quote {
                Some('\'') => token.push(ch),
                Some('"') => match chars.peek().copied() {
                    Some('$' | '`' | '"' | '\\' | '\n') => token.push(chars.next()?),
                    _ => token.push(ch),
                },
                None => token.push(chars.next()?),
                _ => return None,
            }
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
    if quote.is_some() {
        return None;
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    Some(tokens)
}

fn normalize_verb(token: &str) -> Option<String> {
    let name = token
        .rsplit(['/', '\\'])
        .next()?
        .replace('\\', "")
        .to_ascii_lowercase();
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
                if args.first().is_some_and(|arg| arg == "-p") {
                    args.remove(0);
                }
                if args.first().is_some_and(|arg| arg == "--") {
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
                        value if value.starts_with('-') => return None,
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
    let mut flags = Vec::new();
    for arg in args {
        if arg == "--" {
            break;
        }
        match arg.as_str() {
            "--recursive" => flags.push("recursive"),
            "--force" => flags.push("force"),
            value if value.starts_with('-') && !value.starts_with("--") => {
                for flag in value[1..].chars() {
                    match flag {
                        'r' | 'R' => flags.push("recursive"),
                        'f' => flags.push("force"),
                        _ => return Some("floor-opaque-command".to_string()),
                    }
                }
            }
            value if value.starts_with("--") => return Some("floor-opaque-command".to_string()),
            _ => {}
        }
    }
    policy.rule_hit("rm", &flags)
}

fn classify_git(policy: &FloorPolicy, args: &[String]) -> Option<String> {
    let mut args = args;
    while let Some(arg) = args.first() {
        match arg.as_str() {
            "-C" | "-c" | "--git-dir" | "--work-tree" => {
                if args.len() < 2 {
                    return Some("floor-opaque-command".to_string());
                }
                args = &args[2..];
            }
            value
                if value.starts_with("--git-dir=")
                    || value.starts_with("--work-tree=")
                    || matches!(value, "--no-pager" | "--paginate" | "--literal-pathspecs") =>
            {
                args = &args[1..]
            }
            value if value.starts_with('-') => return Some("floor-opaque-command".to_string()),
            _ => break,
        }
    }
    let (subcommand, rest) = args.split_first()?;
    match normalize_verb(subcommand)?.as_str() {
        "push" => classify_push(policy, rest),
        "reset" => classify_reset(policy, rest),
        "clean" => classify_clean(policy, rest),
        _ => None,
    }
}

fn classify_reset(policy: &FloorPolicy, args: &[String]) -> Option<String> {
    let mut hard = false;
    for (index, arg) in args.iter().enumerate() {
        match arg.as_str() {
            "--hard" => hard = true,
            "-q"
            | "--quiet"
            | "--soft"
            | "--mixed"
            | "--merge"
            | "--keep"
            | "--patch"
            | "-p"
            | "--no-refresh"
            | "--pathspec-file-nul" => {}
            "--pathspec-from-file" if args.get(index + 1).is_some() => {}
            value if value.starts_with('-') => return Some("floor-opaque-command".to_string()),
            _ => {}
        }
    }
    hard.then(|| policy.rule_hit("git reset", &["hard"]))
        .flatten()
}

fn classify_clean(policy: &FloorPolicy, args: &[String]) -> Option<String> {
    let mut flags = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--force" => flags.push("force"),
            "-n" | "--dry-run" | "-q" | "--quiet" | "-i" | "--interactive" => {}
            "-e" | "--exclude" if args.get(index + 1).is_some() => index += 1,
            value if value.starts_with("--exclude=") => {}
            value if value.starts_with('-') && !value.starts_with("--") => {
                for flag in value[1..].chars() {
                    match flag {
                        'f' => flags.push("force"),
                        'd' => flags.push("directories"),
                        'x' => flags.push("ignored"),
                        'X' => flags.push("only_ignored"),
                        'n' | 'q' | 'i' => {}
                        _ => return Some("floor-opaque-command".to_string()),
                    }
                }
            }
            value if value.starts_with('-') => return Some("floor-opaque-command".to_string()),
            _ => {}
        }
        index += 1;
    }
    policy.rule_hit("git clean", &flags)
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
            "-q" | "-v" | "-n" | "-u" | "--quiet" | "--verbose" | "--dry-run"
            | "--set-upstream" | "--all" | "--tags" | "--follow-tags" | "--atomic"
            | "--porcelain" | "--progress" | "--no-verify" | "--thin" => {}
            "-o" | "--push-option" | "--repo" => return Some("floor-opaque-command".to_string()),
            _ if arg.starts_with("--") => return Some("floor-opaque-command".to_string()),
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
    fn denies_git_rewrites_with_exact_rule_ids() {
        for command in [
            "git push origin +main",
            "git push origin main -f",
            "git push -qf origin main",
            "git push --prune origin main",
            "GIT PUSH -f origin main",
        ] {
            assert_eq!(
                policy().evaluate(command),
                FloorDecision::Deny("git-rewrite".into()),
                "{command}"
            );
        }
        assert_eq!(
            policy().evaluate("git reset --hard HEAD~1"),
            FloorDecision::Deny("git-reset-hard".into())
        );
        assert_eq!(
            policy().evaluate("git clean -fdx"),
            FloorDecision::Deny("git-clean-force".into())
        );
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
        assert_eq!(
            policy().evaluate("echo \"$(rm -rf /etc)\""),
            FloorDecision::Deny("rm-recursive".into())
        );
        assert_eq!(
            policy().evaluate("echo `git push -f origin main`"),
            FloorDecision::Deny("git-rewrite".into())
        );
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
    fn unwraps_supported_wrappers_and_rtk_dispatch_with_exact_rule_ids() {
        for command in [
            "nohup rm -rf /etc",
            "time -p rm -rf /etc",
            "command -- rm -rf /etc",
            "timeout -s KILL 5 rm -rf /etc",
            "nice -n 5 rm -rf /etc",
            "stdbuf -oL rm -rf /etc",
            "sudo -- rm -rf /etc",
            "doas -- rm -rf /etc",
            "rtk run -c 'rm -rf /etc'",
            "rtk run rm -rf /etc",
            "rtk proxy rm -rf /etc",
            "rtk err rm -rf /etc",
            "rtk test rm -rf /etc",
            "rtk summary -- rm -rf /etc",
        ] {
            assert_eq!(
                policy().evaluate(command),
                FloorDecision::Deny("rm-recursive".into()),
                "{command}"
            );
        }
        assert_eq!(
            policy().evaluate("rtk git push -f origin main"),
            FloorDecision::Deny("git-rewrite".into())
        );
    }

    #[test]
    fn denies_opaque_regions_only_when_they_contain_targeted_verbs() {
        assert_eq!(
            policy().evaluate("echo $(unterminated"),
            FloorDecision::Allow
        );
        assert_eq!(
            policy().evaluate("echo $(rm -rf /etc"),
            FloorDecision::Deny("floor-opaque-command".into())
        );
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
        let payload = r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"type C:/gate/floor.toml"}}"#;
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

    #[test]
    fn respects_quotes_when_splitting_commands() {
        for command in [
            "git commit -m \"feat: a; b\"",
            "git log --format=\"%h|%s\"",
            "git tag -m \"l1\nl2\" v1",
            "git commit -m \"$(cat <<'EOF'\nfeat: x\n\nBody.\nEOF\n)\"",
        ] {
            assert_eq!(
                policy().evaluate(command),
                FloorDecision::Allow,
                "{command}"
            );
        }
        assert_eq!(
            policy().evaluate("echo \"a;b\" ; rm -rf /etc"),
            FloorDecision::Deny("rm-recursive".into())
        );
        assert_eq!(
            policy().evaluate("echo don\\'t $(rm -rf x) \\"),
            FloorDecision::Deny("floor-opaque-command".into())
        );
    }

    #[test]
    fn scans_command_heads_and_inline_interpreters_as_opaque() {
        for command in [
            "FOO=1 rm -rf x",
            "! rm -rf x",
            "(rm -rf x)",
            "{ rm -rf x; }",
            "if true; then rm -rf x; fi",
            "for d in a b; do rm -rf \"$d\"; done",
        ] {
            assert_eq!(
                policy().evaluate(command),
                FloorDecision::Deny("rm-recursive".into()),
                "{command}"
            );
        }
        assert_eq!(
            policy().evaluate("while false; do git push -f; done"),
            FloorDecision::Deny("git-rewrite".into())
        );
        for command in [
            "env rm -rf x",
            "exec rm -rf x",
            "bash -c 'rm -rf x'",
            "sh -c \"git push -f\"",
            "eval \"rm -rf x\"",
            "xargs rm -rf",
            "find . -name t -exec rm -rf {} +",
        ] {
            assert_eq!(
                policy().evaluate(command),
                FloorDecision::Deny("floor-opaque-command".into()),
                "{command}"
            );
        }
        assert_eq!(
            policy().evaluate("bash -c 'echo safe'"),
            FloorDecision::Allow
        );
    }

    #[test]
    fn recursively_scans_rtk_carried_commands() {
        for command in [
            "rtk run 'rm -rf x'",
            "rtk run -c 'echo a; rm -rf x'",
            "rtk run -c 'nohup rm -rf x'",
            "rtk proxy nohup rm -rf x",
        ] {
            assert_eq!(
                policy().evaluate(command),
                FloorDecision::Deny("rm-recursive".into()),
                "{command}"
            );
        }
        assert_eq!(
            policy().evaluate("rtk -vv git push -f origin main"),
            FloorDecision::Deny("git-rewrite".into())
        );
    }

    #[test]
    fn distinguishes_heredoc_data_from_interpreter_input() {
        assert_eq!(
            policy().evaluate("bash <<'EOF'\nrm -rf x\nEOF"),
            FloorDecision::Deny("floor-opaque-command".into())
        );
        assert_eq!(
            policy().evaluate("cat <<EOF > notes.md\ngit push --force was refused\nEOF"),
            FloorDecision::Allow
        );
    }

    #[test]
    fn protects_quoted_redirections_and_unparseable_segments() {
        let floor = Path::new("C:/gate/floor.toml");
        let binary = Path::new("C:/bin/kleos-sh.exe");
        for command in [
            "cat \"C:\\gate\\floor.toml\"",
            "echo x 2>C:/gate/floor.toml",
            "cat C:/gate/floor.toml \"a;b\"",
        ] {
            let payload = format!("{{\"hook_event_name\":\"PreToolUse\",\"tool_name\":\"Bash\",\"tool_input\":{{\"command\":\"{}\"}}}}", command.replace('\\', "\\\\").replace('"', "\\\""));
            assert_eq!(
                evaluate_payload(&policy(), &payload, floor, binary),
                FloorDecision::Deny("floor-self-protection".into()),
                "{command}"
            );
        }
    }

    #[test]
    fn normalizes_windows_rm_before_unescaping_path_separators() {
        assert_eq!(
            policy().evaluate("'C:\\bin\\rm.exe' -rf x"),
            FloorDecision::Deny("rm-recursive".into())
        );
    }

    #[test]
    fn rejects_unknown_git_and_wrapper_options_but_allows_known_silent_forms() {
        for command in [
            "git -P push -f",
            "git --bare push --force",
            "git push --bogus origin main",
            "nice -5 rm -rf /etc",
            "timeout --bogus 5 rm -rf /etc",
            "timeout -k5 10 rm -rf x",
        ] {
            assert_eq!(
                policy().evaluate(command),
                FloorDecision::Deny("floor-opaque-command".into()),
                "{command}"
            );
        }
        assert_eq!(
            policy().evaluate("time -p -- rm -rf /etc"),
            FloorDecision::Deny("rm-recursive".into())
        );
        for command in ["git clean -n --exclude=*.pdf", "rm --force fichier.txt"] {
            assert_eq!(
                policy().evaluate(command),
                FloorDecision::Allow,
                "{command}"
            );
        }
    }

    #[test]
    fn applies_extension_rules_and_rejects_invalid_scratch_roots() {
        let extension = format!("{POLICY}\n[[deny]]\nid = \"rm-force\"\nverb = \"rm\"\nflags_any = [\"force\"]\n[[deny]]\nid = \"clean-dirs\"\nverb = \"git clean\"\nflags_any = [\"directories\"]");
        let policy = FloorPolicy::parse(&extension).expect("valid extension");
        assert_eq!(
            policy.evaluate("rm -f x"),
            FloorDecision::Deny("rm-force".into())
        );
        assert_eq!(
            policy.evaluate("git clean -d -n"),
            FloorDecision::Deny("clean-dirs".into())
        );
        for root in ["relative/*", "$HOME/.."] {
            assert!(FloorPolicy::parse(&format!(
                "{POLICY}\n[scratch]\nroots = [\"{root}\"]\nttl_hours = 24"
            ))
            .is_err());
        }
    }

    #[test]
    fn enforces_policy_and_scanner_budgets() {
        assert!(FloorPolicy::parse(&"x".repeat(64 * 1024 + 1)).is_err());
        let too_many_rules = format!(
            "{POLICY}\n{}",
            (0..61)
                .map(|index| format!(
                    "[[deny]]\nid = \"extra-{index}\"\nverb = \"rm\"\nflags_any = [\"force\"]"
                ))
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert!(FloorPolicy::parse(&too_many_rules).is_err());
        assert_eq!(
            policy().evaluate(&"true;".repeat(257)),
            FloorDecision::Deny("floor-budget-exceeded".into())
        );
        assert_eq!(
            policy().evaluate(&"true ".repeat(4097)),
            FloorDecision::Deny("floor-budget-exceeded".into())
        );
    }

    #[test]
    fn requires_pret_tool_use_bash_payload_with_text_command() {
        for payload in [
            r#"{"hook_event_name":"PostToolUse","tool_name":"Bash","tool_input":{"command":"rm -rf x"}}"#,
            r#"{"tool_name":"Bash","tool_input":{"command":"rm -rf x"}}"#,
            r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":1}}"#,
            r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":""}}"#,
        ] {
            assert_eq!(
                evaluate_payload(
                    &policy(),
                    payload,
                    Path::new("C:/gate/floor.toml"),
                    Path::new("C:/bin/kleos-sh.exe")
                ),
                FloorDecision::Deny("floor-invalid-payload".into())
            );
        }
    }
}
