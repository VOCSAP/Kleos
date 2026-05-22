use crate::config::Config;
use regex::{Regex, RegexSet};

pub fn check_blocked_patterns(command: &str, blocked_patterns: &[String]) -> Option<String> {
    let command_lower = command.to_lowercase();
    for pattern in blocked_patterns {
        let trimmed = pattern.trim();
        if trimmed.is_empty() {
            continue;
        }
        if pattern_matches(&command_lower, &trimmed.to_lowercase()) {
            return Some(format!("Command matched blocked pattern: {}", trimmed));
        }
    }
    None
}

/// Pattern matcher with optional `*` wildcard support (Patch 19b extension).
///
/// - If `pattern` contains no `*`, falls back to `command.contains(pattern)`
///   so existing patterns remain bit-for-bit compatible.
/// - If `pattern` contains one or more `*`, each `*` matches 0 or more chars
///   (greedy from left). Concretely: split on `*`, then every non-empty
///   segment must appear in `command` in order. Empty segments (consecutive
///   `*` or `*` at either end) are skipped.
/// - `*` alone matches any non-empty command (all segments empty after split).
///
/// Both inputs are expected lower-cased by the caller.
pub fn pattern_matches(command_lower: &str, pattern_lower: &str) -> bool {
    if !pattern_lower.contains('*') {
        return command_lower.contains(pattern_lower);
    }
    let mut cursor = 0usize;
    for segment in pattern_lower.split('*') {
        if segment.is_empty() {
            continue;
        }
        match command_lower[cursor..].find(segment) {
            Some(pos) => cursor += pos + segment.len(),
            None => return false,
        }
    }
    true
}

/// Check a command against static dangerous patterns.
/// Returns Some(reason) if the command is blocked, None if it is allowed.
///
/// Ported from Eidolon gate.rs -- covers destructive rm, force push, hard reset,
/// reboot/shutdown, seed data, protected services, interpreter inline execution,
/// encoding-bypass obfuscation, variable indirection, DROP TABLE, and mkfs.
pub fn check_dangerous_patterns(command: &str, config: &Config) -> Option<String> {
    let cmd_lower = command.to_lowercase();

    // Destructive rm patterns
    if cmd_lower.contains("rm -rf /") && !cmd_lower.contains("rm -rf /tmp") {
        return Some("Destructive rm -rf on critical path - not allowed".to_string());
    }
    if cmd_lower.contains("rm -rf ~/") {
        return Some("Destructive rm -rf on home directory - not allowed".to_string());
    }
    if cmd_lower.contains("rm -rf /home") {
        return Some("Destructive rm -rf on /home - not allowed".to_string());
    }
    if cmd_lower.contains("rm -rf /var") {
        return Some("Destructive rm -rf on /var - not allowed".to_string());
    }
    if cmd_lower.contains("rm -rf /etc") {
        return Some("Destructive rm -rf on /etc - not allowed".to_string());
    }
    if cmd_lower.contains("rm -rf /usr") {
        return Some("Destructive rm -rf on /usr - not allowed".to_string());
    }
    if cmd_lower.contains("rm -rf /opt") {
        return Some("Destructive rm -rf on /opt - not allowed".to_string());
    }
    if cmd_lower.contains("rm -rf /boot") {
        return Some("Destructive rm -rf on /boot - not allowed".to_string());
    }

    // Force push to protected branches
    if cmd_lower.contains("git push")
        && cmd_lower.contains("--force")
        && (cmd_lower.contains("main") || cmd_lower.contains("master"))
    {
        return Some("Force push to main/master branch blocked".to_string());
    }

    // Hard reset
    if cmd_lower.contains("git reset --hard") {
        return Some("git reset --hard is destructive - use git stash instead".to_string());
    }

    // Reboot/shutdown: check servers with no_reboot flag
    if cmd_lower.contains("reboot") || cmd_lower.contains("shutdown") {
        for server in &config.eidolon.gate.servers {
            if server.no_reboot {
                let name_match = cmd_lower.contains(&server.name.to_lowercase());
                let alias_match = server
                    .aliases
                    .iter()
                    .any(|a| cmd_lower.contains(&a.to_lowercase()));
                if name_match || alias_match {
                    let notes = if server.notes.is_empty() {
                        String::new()
                    } else {
                        format!(" - {}", server.notes)
                    };
                    return Some(format!(
                        "Reboot/shutdown of {} blocked{}",
                        server.name, notes
                    ));
                }
            }
        }
        // Generic reboot/shutdown block when no server inventory is configured
        if config.eidolon.gate.servers.is_empty() {
            return Some("Reboot/shutdown commands require explicit confirmation".to_string());
        }
    }

    // Seed data in production -- prevent seeding demo/sample/insert into prod
    if cmd_lower.contains("seed") {
        if cmd_lower.contains("demo") {
            return Some(
                "Seeding demo data blocked - do not seed demo data into any instance without explicit authorization".to_string(),
            );
        }
        if cmd_lower.contains("production") || cmd_lower.contains("prod") {
            return Some(
                "Seeding production data blocked - do not seed real data into production without explicit authorization".to_string(),
            );
        }
    }
    if (cmd_lower.contains("sample") || cmd_lower.contains("demo"))
        && (cmd_lower.contains("insert") || cmd_lower.contains("create"))
    {
        return Some("Inserting sample/demo data requires explicit authorization".to_string());
    }

    // Stop/restart protected services
    if cmd_lower.contains("systemctl stop")
        || cmd_lower.contains("systemctl restart")
        || cmd_lower.contains("podman stop")
        || cmd_lower.contains("docker stop")
    {
        for svc in &config.eidolon.gate.protected_services {
            if cmd_lower.contains(&svc.to_lowercase()) {
                return Some(format!(
                    "Stopping/restarting protected service {} requires explicit confirmation",
                    svc
                ));
            }
        }
    }

    // Secondary interpreter / encoding bypass detection
    // These can be used to smuggle dangerous commands past substring checks
    let tokens: Vec<&str> = cmd_lower.split_whitespace().collect();
    {
        for (i, token) in tokens.iter().enumerate() {
            // python/python3 -c, perl/perl5 -e, ruby -e
            // Also catch full-path invocations like /usr/bin/python3 and env-wrapped calls
            let basename = token.rsplit('/').next().unwrap_or(token);
            let is_interpreter = basename == "python"
                || basename == "python3"
                || basename.starts_with("python3.")
                || basename == "perl"
                || basename == "perl5"
                || basename == "ruby";
            // Also catch: env python3 -c
            let is_env_interpreter = *token == "env" && i + 2 < tokens.len() && {
                let next = tokens[i + 1];
                let next_base = next.rsplit('/').next().unwrap_or(next);
                next_base == "python"
                    || next_base == "python3"
                    || next_base.starts_with("python3.")
                    || next_base == "perl"
                    || next_base == "perl5"
                    || next_base == "ruby"
            };
            if is_interpreter {
                if let Some(flag) = tokens.get(i + 1) {
                    if *flag == "-c" || *flag == "-e" {
                        return Some(format!(
                            "Inline code execution via {} {} blocked - use a script file instead",
                            token, flag
                        ));
                    }
                }
            }
            if is_env_interpreter {
                // env python3 -c => flag is at i+2
                if let Some(flag) = tokens.get(i + 2) {
                    if *flag == "-c" || *flag == "-e" {
                        return Some(format!(
                            "Inline code execution via env {} {} blocked - use a script file instead",
                            tokens[i + 1], flag
                        ));
                    }
                }
            }

            // eval with command substitution or string argument
            if *token == "eval" && i + 1 < tokens.len() {
                return Some(
                    "eval command blocked - potential command injection vector".to_string(),
                );
            }
        }

        // base64 decode piped to sh/bash (base64 -d, base64 --decode, base64 -D)
        let has_base64_decode =
            cmd_lower.contains("base64 -d") || cmd_lower.contains("base64 --decode");
        let has_shell_pipe = cmd_lower.contains("| sh")
            || cmd_lower.contains("| bash")
            || cmd_lower.contains("|sh")
            || cmd_lower.contains("|bash")
            || cmd_lower.contains("| /bin/sh")
            || cmd_lower.contains("| /bin/bash");
        if has_base64_decode && has_shell_pipe {
            return Some(
                "base64 decode piped to shell blocked - potential command obfuscation".to_string(),
            );
        }

        // xxd -r piped to shell
        if cmd_lower.contains("xxd -r") && has_shell_pipe {
            return Some(
                "hex decode piped to shell blocked - potential command obfuscation".to_string(),
            );
        }

        // printf with octal/hex escapes piped to shell
        if cmd_lower.contains("printf")
            && (cmd_lower.contains("\\x") || cmd_lower.contains("\\0"))
            && has_shell_pipe
        {
            return Some(
                "printf escape sequence piped to shell blocked - potential command obfuscation"
                    .to_string(),
            );
        }
    }

    // Variable indirection: assignment of dangerous commands to variables
    // Catches: R="rm"; $R -rf / and CMD=rm; $CMD -rf /
    {
        let dangerous_cmds = ["rm", "mkfs", "dd", "shutdown", "reboot", "kill", "pkill"];
        for cmd in &dangerous_cmds {
            let patterns = [
                format!("=\"{}\"", cmd),
                format!("='{}'", cmd),
                format!("={};", cmd),
                format!("={} ", cmd),
                format!("={}&", cmd),
            ];
            if patterns.iter().any(|p| cmd_lower.contains(p)) && cmd_lower.contains('$') {
                return Some(format!(
                    "Shell variable indirection constructing '{}' command blocked",
                    cmd
                ));
            }
        }

        // Backtick command substitution targeting destructive commands
        if cmd_lower.contains('`') {
            let dangerous_cmds_bt = ["rm", "mkfs", "dd", "shutdown", "reboot"];
            for cmd in &dangerous_cmds_bt {
                if cmd_lower.contains(&format!("`echo {}`", cmd))
                    || cmd_lower.contains(&format!("`printf {}`", cmd))
                {
                    return Some(format!(
                        "Command substitution constructing '{}' blocked",
                        cmd
                    ));
                }
            }
        }
    }

    // Extended interpreter coverage: node, deno, lua, php, etc.
    {
        for (i, token) in tokens.iter().enumerate() {
            let basename = token.rsplit('/').next().unwrap_or(token);

            let is_extra_interpreter = matches!(
                basename,
                "node" | "nodejs" | "deno" | "bun" | "lua" | "luajit" | "php" | "tclsh" | "wish"
            ) || basename.starts_with("lua5.")
                || basename.starts_with("php8.");

            if is_extra_interpreter {
                if let Some(flag) = tokens.get(i + 1) {
                    if *flag == "-e"
                        || *flag == "-r"
                        || *flag == "eval"
                        || *flag == "--eval"
                        || *flag == "-c"
                    {
                        return Some(format!(
                            "Inline code execution via {} {} blocked - use a script file",
                            token, flag
                        ));
                    }
                }
            }
        }
    }

    // Drop table / format destructors
    if cmd_lower.contains("drop table") {
        return Some("DROP TABLE statement requires manual confirmation".to_string());
    }
    if cmd_lower.contains("drop database") {
        return Some("DROP DATABASE statement requires manual confirmation".to_string());
    }
    if cmd_lower.contains("mkfs.") || cmd_lower.contains("mkfs ") {
        return Some("Disk format command blocked - requires manual confirmation".to_string());
    }

    None
}

// =============================================================================
// Patch 25 -- regex matcher + per-cascade whitelist + subcommand splitting
// =============================================================================
//
// Adds three additive capabilities (no breaking change to pattern_matches /
// check_blocked_patterns above which remain consumed by existing tests):
//
// 1. CompiledPatternSet -- regex::RegexSet wrapping a heterogeneous list of
//    user patterns. Lines without regex metacharacters (^, $, [, (, \, +, ?,
//    {, |, .) are treated as glob-lite (literals + * wildcard) and anchored
//    as ^pattern$ at compile time. Lines with metacharacters are taken as
//    regex pure. Invalid regex is logged and skipped (does not crash load).
//
// 2. split_into_subcommands -- shell-aware splitter that respects POSIX
//    quoting (via shell-words) AND tracks backticks, $(...), <(...), >(...)
//    via a char-by-char walker. Heredoc bodies (<<EOF ... EOF and variants)
//    are detected at pre-scan and excluded from the splitter (data, not
//    command). Backticks, $(...), <(...) and >(...) inner content is
//    recursed (capped at depth 5). Tokenize errors are governed by
//    TokenizeErrorPolicy.
//
// 3. Helper readers for the env vars KLEOS_EIDOLON_GATE_ON_TOKENIZE_ERROR.
//
// Decision operator 2026-05-22 :
//   - whitelist exists only for blocked + require_approval cascades
//   - extra_dangerous_patterns is NOT exemptable (additive only)
//   - check_dangerous_patterns hardcoded NOT exemptable (ultimate guard rail)
//   - empty whitelist != allow-all : empty set matches nothing -> no
//     exemption applied -> blocklist applies normally as pre-Patch 25.

const REGEX_METACHARS: &[char] =
    &['^', '$', '[', ']', '(', ')', '\\', '+', '?', '{', '}', '|', '.'];

fn is_obvious_regex(pattern: &str) -> bool {
    pattern.chars().any(|c| REGEX_METACHARS.contains(&c))
}

fn glob_lite_to_regex(pattern: &str) -> String {
    let escaped = regex::escape(pattern);
    // regex::escape turns the original '*' into '\*'. Reintroduce '.*'
    // for glob semantic: `*` matches 0 or more chars (greedy).
    //
    // (?i) prefix preserves the pre-Patch 25 case-insensitive matching
    // semantic (Patch 19b `pattern_matches` lower-cased both inputs).
    // Anchored `^...$` enforces full-match for glob-lite, eliminating
    // the `kleos-cli store "git push"` matches `git push` bug (memoire
    // Kleos #3020). Operator who wants contains semantic must use
    // `*pattern*` explicitly.
    let with_wildcard = escaped.replace("\\*", ".*");
    format!("(?i)^{}$", with_wildcard)
}

/// A precompiled set of regex patterns with parallel source labels for
/// reason reporting. Built from a `&[String]` of user-supplied patterns
/// (regex pure OR glob-lite, auto-detected at load).
///
/// An empty `CompiledPatternSet` matches nothing (RegexSet::empty()) so
/// `check_compiled` returns None for any input. This is the semantic
/// expected when the operator does not provide a whitelist: no exemption
/// applied, blocklist applies normally.
pub struct CompiledPatternSet {
    pub set: RegexSet,
    pub sources: Vec<String>,
}

impl CompiledPatternSet {
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

pub fn compile_patterns(raw: &[String]) -> CompiledPatternSet {
    let mut regex_strs: Vec<String> = Vec::new();
    let mut sources: Vec<String> = Vec::new();
    for line in raw {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Patch 25 hyp_cc53cc9b: try-regex-first-then-glob-lite-fallback.
        //
        // The naive heuristic (is_obvious_regex by metachar presence) misclassifies
        // glob-lite patterns that incidentally contain backslash (Windows paths)
        // or brace/pipe/paren syntax (fork bomb literals). Production deploy on
        // LXC 121 2026-05-23 showed these patterns get classified as regex,
        // fail Regex::new(), and silently skip -- leaving blocklists incomplete.
        //
        // New flow: if is_obvious_regex, try compile as regex; if it fails,
        // fall back to glob_lite_to_regex (regex::escape handles all metachars
        // safely). Patterns without metachars always go glob-lite directly.
        let regex_candidate = if is_obvious_regex(trimmed) {
            if trimmed.starts_with("(?") {
                trimmed.to_string()
            } else {
                format!("(?i){}", trimmed)
            }
        } else {
            glob_lite_to_regex(trimmed)
        };
        let (regex_str, mode) = match Regex::new(&regex_candidate) {
            Ok(_) => (regex_candidate, "regex-or-glob"),
            Err(e) => {
                // Regex-classified but failed to compile -> fall back to glob-lite.
                let fallback = glob_lite_to_regex(trimmed);
                match Regex::new(&fallback) {
                    Ok(_) => {
                        tracing::info!(
                            target: "kleos::gate::patch25",
                            "pattern {:?} regex-classified but failed compile ({}), fallback glob-lite -> {:?}",
                            trimmed,
                            e,
                            fallback,
                        );
                        (fallback, "fallback-glob-lite")
                    }
                    Err(e2) => {
                        tracing::warn!(
                            target: "kleos::gate::patch25",
                            "skipping pattern {:?}: regex compile failed ({}), glob-lite fallback also failed ({})",
                            trimmed,
                            e,
                            e2,
                        );
                        continue;
                    }
                }
            }
        };
        if mode == "regex-or-glob" && !is_obvious_regex(trimmed) {
            tracing::info!(
                target: "kleos::gate::patch25",
                "pattern {:?} loaded as glob-lite -> regex {:?}",
                trimmed,
                regex_str,
            );
        }
        regex_strs.push(regex_str);
        sources.push(trimmed.to_string());
    }
    let set = match RegexSet::new(&regex_strs) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                target: "kleos::gate::patch25",
                "RegexSet build failed ({}), falling back to empty set",
                e,
            );
            RegexSet::empty()
        }
    };
    CompiledPatternSet { set, sources }
}

/// Match `command` against a CompiledPatternSet. Returns the first matching
/// source pattern wrapped in a "Command matched blocked pattern: ..." string
/// (the caller may relabel with replacen() to a domain-specific prefix).
///
/// Empty set -> None for any input (no exemption applied).
pub fn check_compiled(command: &str, compiled: &CompiledPatternSet) -> Option<String> {
    if compiled.is_empty() {
        return None;
    }
    let matches = compiled.set.matches(command);
    matches
        .iter()
        .next()
        .map(|i| format!("Command matched blocked pattern: {}", compiled.sources[i]))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenizeErrorPolicy {
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    /// shell-words::split returned an error AND policy is Deny.
    /// Carries the original cmdline for the gate "blocked" reason.
    Malformed(String),
}

/// Read the env var KLEOS_EIDOLON_GATE_ON_TOKENIZE_ERROR.
/// Values: "allow" -> Allow ; anything else (including unset) -> Deny.
pub fn tokenize_error_policy() -> TokenizeErrorPolicy {
    match std::env::var("KLEOS_EIDOLON_GATE_ON_TOKENIZE_ERROR")
        .ok()
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("allow") => TokenizeErrorPolicy::Allow,
        _ => TokenizeErrorPolicy::Deny,
    }
}

const SUBCOMMAND_RECURSION_LIMIT: u32 = 5;

/// Split a cmdline into independently executable subcommands, respecting
/// POSIX quoting and tracking shell substitutions/redirections.
///
/// Connectors recognized: `&&`, `||`, `;`, `|`, `&` (suffix).
/// Recursed into: backticks `...`, `$(...)`, `<(...)`, `>(...)`.
/// Heredoc bodies (`<<EOF`, `<<-EOF`, `<<'EOF'`, `<<"EOF"`) are excluded
/// from splitting (their body is data, not a subcommand).
///
/// Quoting is respected: `kleos-cli store "git push"` stays as ONE
/// subcommand. shell-words is used to validate tokenize correctness;
/// on error, `policy` decides whether to deny or fall back to a single
/// subcommand.
pub fn split_into_subcommands(
    cmdline: &str,
    policy: TokenizeErrorPolicy,
) -> Result<Vec<String>, SplitError> {
    split_into_subcommands_inner(cmdline, policy, 0)
}

fn split_into_subcommands_inner(
    cmdline: &str,
    policy: TokenizeErrorPolicy,
    depth: u32,
) -> Result<Vec<String>, SplitError> {
    if depth >= SUBCOMMAND_RECURSION_LIMIT {
        tracing::warn!(
            target: "kleos::gate::patch25",
            "subcommand recursion depth {} reached on cmdline {:?}, stopping recurse",
            depth,
            cmdline,
        );
        return Ok(vec![cmdline.to_string()]);
    }

    if cmdline.trim().is_empty() {
        return Ok(Vec::new());
    }

    // Step 0: validate tokenize correctness via shell-words. On error,
    // honor the operator-defined policy.
    if let Err(e) = shell_words::split(cmdline) {
        match policy {
            TokenizeErrorPolicy::Allow => {
                tracing::warn!(
                    target: "kleos::gate::patch25",
                    "shell-words tokenize error ({}) on cmdline {:?}, treating as 1 subcommand (policy=Allow)",
                    e,
                    cmdline,
                );
                return Ok(vec![cmdline.to_string()]);
            }
            TokenizeErrorPolicy::Deny => {
                return Err(SplitError::Malformed(cmdline.to_string()));
            }
        }
    }

    // Step 1: pre-scan heredoc bodies, mark byte-ranges to skip.
    let heredoc_skip = find_heredoc_ranges(cmdline);

    // Step 2: char-by-char walker that splits on connectors outside quotes
    // and outside substitutions. Heredoc-body bytes are ignored.
    let chars: Vec<char> = cmdline.chars().collect();
    let mut byte_index: usize = 0;
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_squote = false;
    let mut in_dquote = false;
    let mut in_backtick = false;
    let mut paren_depth: u32 = 0;
    let mut escape = false;

    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let c_len = c.len_utf8();

        // Skip heredoc body byte ranges.
        if is_in_heredoc_range(byte_index, &heredoc_skip) {
            byte_index += c_len;
            i += 1;
            continue;
        }

        if escape {
            current.push(c);
            escape = false;
            byte_index += c_len;
            i += 1;
            continue;
        }
        if c == '\\' && !in_squote {
            current.push(c);
            escape = true;
            byte_index += c_len;
            i += 1;
            continue;
        }
        if c == '\'' && !in_dquote && !in_backtick {
            in_squote = !in_squote;
            current.push(c);
            byte_index += c_len;
            i += 1;
            continue;
        }
        if c == '"' && !in_squote && !in_backtick {
            in_dquote = !in_dquote;
            current.push(c);
            byte_index += c_len;
            i += 1;
            continue;
        }
        if c == '`' && !in_squote && !in_dquote {
            in_backtick = !in_backtick;
            current.push(c);
            byte_index += c_len;
            i += 1;
            continue;
        }
        // $( ... ) substitution
        if c == '$' && !in_squote && i + 1 < chars.len() && chars[i + 1] == '(' {
            paren_depth += 1;
            current.push(c);
            current.push('(');
            byte_index += c_len + chars[i + 1].len_utf8();
            i += 2;
            continue;
        }
        // <( ... ) and >( ... ) process substitution (treated like $())
        if (c == '<' || c == '>')
            && !in_squote
            && !in_dquote
            && i + 1 < chars.len()
            && chars[i + 1] == '('
        {
            paren_depth += 1;
            current.push(c);
            current.push('(');
            byte_index += c_len + chars[i + 1].len_utf8();
            i += 2;
            continue;
        }
        if c == ')' && paren_depth > 0 {
            paren_depth -= 1;
            current.push(c);
            byte_index += c_len;
            i += 1;
            continue;
        }
        // Inside any quote or substitution -> accumulate.
        if in_squote || in_dquote || in_backtick || paren_depth > 0 {
            current.push(c);
            byte_index += c_len;
            i += 1;
            continue;
        }
        // 2-char connectors: && and ||
        if (c == '&' || c == '|') && i + 1 < chars.len() && chars[i + 1] == c {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                segments.push(trimmed.to_string());
            }
            current.clear();
            byte_index += c_len + chars[i + 1].len_utf8();
            i += 2;
            continue;
        }
        // 1-char connectors: ; | &
        if c == ';' || c == '|' || c == '&' {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                segments.push(trimmed.to_string());
            }
            current.clear();
            byte_index += c_len;
            i += 1;
            continue;
        }
        // Heredoc operator marker: skip the marker chars themselves so the
        // walker continues on the cmd that precedes/follows the heredoc.
        // The body itself is excluded by the heredoc_skip pre-scan.
        current.push(c);
        byte_index += c_len;
        i += 1;
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        segments.push(trimmed.to_string());
    }

    // Step 3: for each segment, recurse into backticks, $(...), <(...), >(...).
    let mut all: Vec<String> = Vec::new();
    for seg in segments {
        all.push(seg.clone());
        for inner in extract_inner_substitutions(&seg) {
            match split_into_subcommands_inner(&inner, policy, depth + 1) {
                Ok(subs) => all.extend(subs),
                Err(SplitError::Malformed(_)) => {
                    // Inner cmdline malformed: surface as Deny propagation.
                    // We do not abort the outer call; report the inner as a
                    // single "raw" subcommand so the cascade still gets to
                    // evaluate it against dangerous patterns.
                    all.push(inner);
                }
            }
        }
    }
    Ok(all)
}

/// Pre-scan a cmdline for heredoc bodies. Returns a vector of (start_byte,
/// end_byte) ranges that the splitter should ignore (heredoc body is data).
///
/// Supports `<<TAG`, `<<-TAG`, `<<'TAG'`, `<<"TAG"`. The terminator must
/// appear on its own line (optionally indented if `<<-`). Heuristic: we
/// look for `<<` outside of quotes/backticks at byte level.
fn find_heredoc_ranges(cmdline: &str) -> Vec<(usize, usize)> {
    let bytes = cmdline.as_bytes();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;
    let mut in_squote = false;
    let mut in_dquote = false;
    let mut in_backtick = false;
    while i + 1 < bytes.len() {
        let b = bytes[i];
        if b == b'\'' && !in_dquote && !in_backtick {
            in_squote = !in_squote;
            i += 1;
            continue;
        }
        if b == b'"' && !in_squote && !in_backtick {
            in_dquote = !in_dquote;
            i += 1;
            continue;
        }
        if b == b'`' && !in_squote && !in_dquote {
            in_backtick = !in_backtick;
            i += 1;
            continue;
        }
        if in_squote || in_dquote || in_backtick {
            i += 1;
            continue;
        }
        if b == b'<' && bytes[i + 1] == b'<' {
            // possibly heredoc
            let mut j = i + 2;
            let _strip = if j < bytes.len() && bytes[j] == b'-' {
                j += 1;
                true
            } else {
                false
            };
            // skip whitespace
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            // optional quoting around tag
            let mut tag_start = j;
            let tag_end;
            if j < bytes.len() && (bytes[j] == b'\'' || bytes[j] == b'"') {
                let quote_b = bytes[j];
                j += 1;
                tag_start = j;
                while j < bytes.len() && bytes[j] != quote_b {
                    j += 1;
                }
                tag_end = j;
                if j < bytes.len() {
                    j += 1; // consume closing quote
                }
            } else {
                while j < bytes.len()
                    && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')
                {
                    j += 1;
                }
                tag_end = j;
            }
            if tag_end > tag_start {
                let tag = &bytes[tag_start..tag_end];
                // Body starts at the next newline (or here if no newline yet).
                let mut body_start = j;
                while body_start < bytes.len() && bytes[body_start] != b'\n' {
                    body_start += 1;
                }
                if body_start < bytes.len() {
                    body_start += 1; // skip the \n itself
                }
                // Find a line that is exactly the tag (optionally indented if strip).
                let mut k = body_start;
                let mut body_end = bytes.len();
                while k < bytes.len() {
                    // find start of line
                    let line_start = k;
                    let mut line_end = k;
                    while line_end < bytes.len() && bytes[line_end] != b'\n' {
                        line_end += 1;
                    }
                    let mut ls = line_start;
                    if _strip {
                        while ls < line_end && (bytes[ls] == b'\t' || bytes[ls] == b' ') {
                            ls += 1;
                        }
                    }
                    if &bytes[ls..line_end] == tag {
                        body_end = line_start;
                        break;
                    }
                    k = if line_end < bytes.len() {
                        line_end + 1
                    } else {
                        line_end
                    };
                }
                ranges.push((body_start, body_end));
                i = if body_end < bytes.len() {
                    body_end
                } else {
                    bytes.len()
                };
                continue;
            }
        }
        i += 1;
    }
    ranges
}

fn is_in_heredoc_range(byte_pos: usize, ranges: &[(usize, usize)]) -> bool {
    ranges.iter().any(|&(s, e)| byte_pos >= s && byte_pos < e)
}

/// Walk a segment and extract the inner content of backtick `...` and
/// $(...) / <(...) / >(...) substitutions. Returns the inner strings
/// without their delimiters, in left-to-right order. Unbalanced delimiters
/// are ignored (no inner extracted for that opener).
fn extract_inner_substitutions(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut in_squote = false;
    let mut in_dquote = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' && !in_dquote {
            in_squote = !in_squote;
            i += 1;
            continue;
        }
        if c == '"' && !in_squote {
            in_dquote = !in_dquote;
            i += 1;
            continue;
        }
        if in_squote {
            i += 1;
            continue;
        }
        if c == '`' {
            let start = i + 1;
            let mut j = start;
            while j < chars.len() && chars[j] != '`' {
                j += 1;
            }
            if j < chars.len() {
                out.push(chars[start..j].iter().collect());
                i = j + 1;
                continue;
            }
            break;
        }
        if (c == '$' || c == '<' || c == '>')
            && i + 1 < chars.len()
            && chars[i + 1] == '('
        {
            let start = i + 2;
            let mut depth: i32 = 1;
            let mut j = start;
            while j < chars.len() && depth > 0 {
                if chars[j] == '(' {
                    depth += 1;
                } else if chars[j] == ')' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                j += 1;
            }
            if j < chars.len() {
                out.push(chars[start..j].iter().collect());
                i = j + 1;
                continue;
            }
            break;
        }
        i += 1;
    }
    out
}

// =============================================================================
// Patch 25 -- unit tests
// =============================================================================

#[cfg(test)]
mod patch25_tests {
    use super::*;

    #[test]
    fn patch25_is_obvious_regex_detects_metachars() {
        assert!(is_obvious_regex("^foo"));
        assert!(is_obvious_regex("foo$"));
        assert!(is_obvious_regex("foo[abc]"));
        assert!(is_obvious_regex("foo(a|b)"));
        assert!(is_obvious_regex("foo\\d"));
        assert!(is_obvious_regex("foo.bar"));
        assert!(!is_obvious_regex("foo bar"));
        assert!(!is_obvious_regex("foo*bar"));
        assert!(!is_obvious_regex("plain text"));
    }

    #[test]
    fn patch25_glob_lite_to_regex_anchors_and_wildcards() {
        assert_eq!(glob_lite_to_regex("git push"), "(?i)^git push$");
        assert_eq!(glob_lite_to_regex("apt install*"), "(?i)^apt install.*$");
        assert_eq!(glob_lite_to_regex("*systemctl*"), "(?i)^.*systemctl.*$");
        assert_eq!(glob_lite_to_regex("foo*bar*baz"), "(?i)^foo.*bar.*baz$");
    }

    #[test]
    fn patch25_glob_lite_no_wildcard_matches_exact_only() {
        // The bug fix: "git push" should match "git push" exactly,
        // NOT match `kleos-cli store "git push"` anymore.
        let compiled = compile_patterns(&["git push".to_string()]);
        assert!(check_compiled("git push", &compiled).is_some());
        assert!(check_compiled("git push origin main", &compiled).is_none());
        assert!(check_compiled("kleos-cli store \"git push\"", &compiled).is_none());
    }

    #[test]
    fn patch25_glob_lite_with_wildcard_matches_prefix() {
        let compiled = compile_patterns(&["apt install*".to_string()]);
        assert!(check_compiled("apt install", &compiled).is_some());
        assert!(check_compiled("apt install vim", &compiled).is_some());
        assert!(check_compiled("yum install vim", &compiled).is_none());
    }

    #[test]
    fn patch25_glob_lite_wildcards_at_both_ends_matches_contains() {
        let compiled = compile_patterns(&["*system32*".to_string()]);
        assert!(check_compiled("rm c:\\windows\\system32\\foo", &compiled).is_some());
        assert!(check_compiled("Remove-Item C:\\Windows\\System32", &compiled).is_some());
        assert!(check_compiled("hello world", &compiled).is_none());
    }

    #[test]
    fn patch25_regex_pure_pattern_compiles_as_regex() {
        let compiled = compile_patterns(&["^systemctl\\s+(stop|start|restart)\\b".to_string()]);
        assert!(check_compiled("systemctl stop foo", &compiled).is_some());
        assert!(check_compiled("systemctl status foo", &compiled).is_none());
        assert!(check_compiled("my systemctl stop", &compiled).is_none()); // anchored ^
    }

    #[test]
    fn patch25_case_insensitive_by_default() {
        let compiled = compile_patterns(&["system32".to_string()]);
        // glob-lite -> (?i)^system32$
        assert!(check_compiled("SYSTEM32", &compiled).is_some());
        assert!(check_compiled("System32", &compiled).is_some());
        let compiled2 = compile_patterns(&["^[a-z]+$".to_string()]);
        // regex-pure -> (?i)^[a-z]+$ (case-insensitive applies to char class too)
        assert!(check_compiled("ABCDEF", &compiled2).is_some());
    }

    #[test]
    fn patch25_invalid_regex_falls_back_to_glob_lite() {
        // Post-hyp_cc53cc9b: a pattern like "[invalid" is regex-classified but
        // fails compile. The fallback escapes it via glob_lite_to_regex, so it
        // matches the literal string "[invalid" instead of being skipped.
        let compiled = compile_patterns(&["[invalid".to_string(), "valid_pattern".to_string()]);
        assert_eq!(compiled.sources.len(), 2);
        assert!(check_compiled("[invalid", &compiled).is_some());
        assert!(check_compiled("valid_pattern", &compiled).is_some());
    }

    #[test]
    fn patch25_empty_pattern_set_matches_nothing() {
        let compiled = compile_patterns(&[]);
        assert!(compiled.is_empty());
        assert!(check_compiled("anything goes", &compiled).is_none());
    }

    #[test]
    fn patch25_comment_and_blank_lines_skipped() {
        let raw = vec![
            "# this is a comment".to_string(),
            "".to_string(),
            "   ".to_string(),
            "real_pattern".to_string(),
        ];
        let compiled = compile_patterns(&raw);
        assert_eq!(compiled.sources.len(), 1);
        assert_eq!(compiled.sources[0], "real_pattern");
    }

    #[test]
    fn patch25_split_simple_double_ampersand() {
        let subs = split_into_subcommands("cmd1 && cmd2", TokenizeErrorPolicy::Deny).unwrap();
        assert_eq!(subs, vec!["cmd1".to_string(), "cmd2".to_string()]);
    }

    #[test]
    fn patch25_split_respects_double_quotes() {
        let subs = split_into_subcommands(
            "kleos-cli store \"git push\" && git push",
            TokenizeErrorPolicy::Deny,
        )
        .unwrap();
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0], "kleos-cli store \"git push\"");
        assert_eq!(subs[1], "git push");
    }

    #[test]
    fn patch25_split_respects_single_quotes() {
        let subs =
            split_into_subcommands("echo 'a && b' && echo done", TokenizeErrorPolicy::Deny).unwrap();
        assert_eq!(subs[0], "echo 'a && b'");
        assert_eq!(subs[1], "echo done");
    }

    #[test]
    fn patch25_split_pipe() {
        let subs =
            split_into_subcommands("cat /etc/shadow | grep root", TokenizeErrorPolicy::Deny)
                .unwrap();
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0], "cat /etc/shadow");
        assert_eq!(subs[1], "grep root");
    }

    #[test]
    fn patch25_split_semicolon() {
        let subs = split_into_subcommands("cmd1 ; cmd2", TokenizeErrorPolicy::Deny).unwrap();
        assert_eq!(subs.len(), 2);
    }

    #[test]
    fn patch25_split_pipe_multilevel_4_subcmds() {
        let subs =
            split_into_subcommands("cat foo | grep bar | sort | head", TokenizeErrorPolicy::Deny)
                .unwrap();
        assert_eq!(subs.len(), 4);
    }

    #[test]
    fn patch25_split_background_vs_and_chain() {
        // `cmd &` -> 1 subcmd (background suffix, splits cmd off)
        let subs_bg = split_into_subcommands("cmd &", TokenizeErrorPolicy::Deny).unwrap();
        assert_eq!(subs_bg, vec!["cmd".to_string()]);
        // `cmd1 && cmd2` -> 2 subcmds (AND chain)
        let subs_and = split_into_subcommands("cmd1 && cmd2", TokenizeErrorPolicy::Deny).unwrap();
        assert_eq!(subs_and.len(), 2);
    }

    #[test]
    fn patch25_split_backtick_recursion() {
        let subs = split_into_subcommands("echo `rm -rf /tmp/foo`", TokenizeErrorPolicy::Deny)
            .unwrap();
        // Outer is "echo `rm -rf /tmp/foo`", inner recurse gives "rm -rf /tmp/foo".
        assert!(subs.iter().any(|s| s == "rm -rf /tmp/foo"));
        assert!(subs.iter().any(|s| s.starts_with("echo")));
    }

    #[test]
    fn patch25_split_dollar_paren_recursion() {
        let subs =
            split_into_subcommands("echo $(systemctl status nginx)", TokenizeErrorPolicy::Deny)
                .unwrap();
        assert!(subs.iter().any(|s| s == "systemctl status nginx"));
    }

    #[test]
    fn patch25_split_process_substitution_recurses() {
        let subs = split_into_subcommands("cat <(systemctl status)", TokenizeErrorPolicy::Deny)
            .unwrap();
        assert!(subs.iter().any(|s| s == "systemctl status"));
        assert!(subs.iter().any(|s| s.starts_with("cat")));
    }

    #[test]
    fn patch25_split_tokenize_error_deny_returns_err() {
        let res = split_into_subcommands("cmd \"unclosed quote", TokenizeErrorPolicy::Deny);
        assert!(matches!(res, Err(SplitError::Malformed(_))));
    }

    #[test]
    fn patch25_split_tokenize_error_allow_fallback() {
        let res =
            split_into_subcommands("cmd \"unclosed quote", TokenizeErrorPolicy::Allow).unwrap();
        // Fallback : the cmdline is treated as 1 subcommand.
        assert_eq!(res.len(), 1);
    }

    #[test]
    fn patch25_split_heredoc_body_ignored_as_data() {
        let cmdline = "cat <<EOF\nrm -rf /\nEOF\necho done";
        let subs = split_into_subcommands(cmdline, TokenizeErrorPolicy::Allow).unwrap();
        // The heredoc body (rm -rf /) is data, not a subcommand. We should see
        // the outer cat command and the trailing echo but NOT a bare "rm -rf /".
        assert!(subs.iter().any(|s| s.starts_with("cat")));
        assert!(subs.iter().any(|s| s == "echo done" || s.contains("echo done")));
        assert!(!subs.iter().any(|s| s.trim() == "rm -rf /"));
    }

    #[test]
    fn patch25_split_empty_cmdline_returns_empty() {
        let subs = split_into_subcommands("", TokenizeErrorPolicy::Deny).unwrap();
        assert!(subs.is_empty());
        let subs2 = split_into_subcommands("   \t  ", TokenizeErrorPolicy::Deny).unwrap();
        assert!(subs2.is_empty());
    }

    #[test]
    fn patch25_tokenize_error_policy_env_default_deny() {
        let prev = std::env::var("KLEOS_EIDOLON_GATE_ON_TOKENIZE_ERROR").ok();
        std::env::remove_var("KLEOS_EIDOLON_GATE_ON_TOKENIZE_ERROR");
        assert_eq!(tokenize_error_policy(), TokenizeErrorPolicy::Deny);
        if let Some(v) = prev {
            std::env::set_var("KLEOS_EIDOLON_GATE_ON_TOKENIZE_ERROR", v);
        }
    }

    #[test]
    fn patch25_tokenize_error_policy_env_allow() {
        let prev = std::env::var("KLEOS_EIDOLON_GATE_ON_TOKENIZE_ERROR").ok();
        std::env::set_var("KLEOS_EIDOLON_GATE_ON_TOKENIZE_ERROR", "allow");
        assert_eq!(tokenize_error_policy(), TokenizeErrorPolicy::Allow);
        std::env::set_var("KLEOS_EIDOLON_GATE_ON_TOKENIZE_ERROR", "deny");
        assert_eq!(tokenize_error_policy(), TokenizeErrorPolicy::Deny);
        std::env::set_var("KLEOS_EIDOLON_GATE_ON_TOKENIZE_ERROR", "garbage");
        assert_eq!(tokenize_error_policy(), TokenizeErrorPolicy::Deny);
        match prev {
            Some(v) => std::env::set_var("KLEOS_EIDOLON_GATE_ON_TOKENIZE_ERROR", v),
            None => std::env::remove_var("KLEOS_EIDOLON_GATE_ON_TOKENIZE_ERROR"),
        }
    }

    #[test]
    fn patch25_fork_bomb_literal_compiles_via_glob_lite_fallback() {
        // hyp_cc53cc9b -- the default blocked_patterns fork bomb ":(){ :|:& };:"
        // contains regex metachars { } | ( ) but is NOT valid regex. Fallback to
        // glob-lite escape must succeed so the pattern stays active.
        let compiled = compile_patterns(&[":(){ :|:& };:".to_string()]);
        assert_eq!(compiled.sources.len(), 1, "fork bomb must be loaded, not skipped");
        assert!(check_compiled(":(){ :|:& };:", &compiled).is_some());
    }

    #[test]
    fn patch25_windows_path_glob_pattern_via_fallback() {
        // hyp_cc53cc9b -- patterns like "*remove-item*c:\\windows*" contain `\`
        // (regex metachar) AND glob wildcards. is_obvious_regex returns true,
        // direct regex compile fails (leading * has no expression), must fall
        // back to glob-lite escape + .* substitution.
        let compiled = compile_patterns(&["*remove-item*c:\\windows*".to_string()]);
        assert_eq!(compiled.sources.len(), 1);
        assert!(check_compiled("Remove-Item -Recurse C:\\Windows\\System32", &compiled).is_some());
    }

    #[test]
    fn patch25_genuine_regex_still_compiles_directly() {
        // Regression guard : a valid regex pattern (no metachar misuse) must
        // still compile via the regex path, not fall back.
        let compiled = compile_patterns(&["^systemctl\\s+(stop|start|restart)\\b".to_string()]);
        assert_eq!(compiled.sources.len(), 1);
        assert!(check_compiled("systemctl stop nginx", &compiled).is_some());
        assert!(check_compiled("foo systemctl stop nginx", &compiled).is_none()); // anchored ^
    }

    #[test]
    fn patch25_split_escaped_quote_handled() {
        // shell-words handles \" as escaped quote inside double quotes.
        let subs = split_into_subcommands(
            "echo \"foo \\\"bar\\\"\"",
            TokenizeErrorPolicy::Deny,
        )
        .unwrap();
        assert_eq!(subs.len(), 1);
    }
}
