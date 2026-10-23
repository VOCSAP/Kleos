use brush_parser::{ast, word, Parser, ParserOptions};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::io::{BufReader, Cursor};
use std::ops::Range;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FloorDecision {
    Allow,
    Warn(String),
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
        if command.len() > 192 * 1024 {
            return FloorDecision::Warn("floor-input-too-large".to_string());
        }
        if raw_syntax_complexity(command) > 256 || non_whitespace_token_count(command) > 4096 {
            return FloorDecision::Warn("floor-budget-exceeded".to_string());
        }
        scan_ast(self, command)
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
            token.split(['>', '<']).any(|token| {
                let token = strip_redirection_prefix(token);
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

fn scan_ast(policy: &FloorPolicy, input: &str) -> FloorDecision {
    let mut parser = Parser::new(
        BufReader::new(Cursor::new(input.as_bytes())),
        &ParserOptions::default(),
    );
    let Ok(program) = parser.parse_program() else {
        return FloorDecision::Warn("floor-opaque-command".to_string());
    };

    enum Task<'a> {
        List(&'a ast::CompoundList),
        AndOr(&'a ast::AndOrList),
        Pipeline(&'a ast::Pipeline),
        Command(&'a ast::Command),
        Compound(&'a ast::CompoundCommand),
    }

    if program.complete_commands.len() > 256 {
        return FloorDecision::Warn("floor-budget-exceeded".to_string());
    }
    let mut tasks = Vec::new();
    for command in program.complete_commands.iter().rev() {
        tasks.push(Task::List(command));
    }
    let mut nodes = 0usize;
    let mut justified_words: Vec<Range<usize>> = Vec::new();
    while let Some(task) = tasks.pop() {
        nodes += 1;
        if nodes > 4096 {
            return FloorDecision::Warn("floor-budget-exceeded".to_string());
        }
        match task {
            Task::List(list) => {
                if list.0.len() > 256 {
                    return FloorDecision::Warn("floor-budget-exceeded".to_string());
                }
                for item in list.0.iter().rev() {
                    tasks.push(Task::AndOr(&item.0));
                }
            }
            Task::AndOr(list) => {
                for (_, pipeline) in list.iter() {
                    tasks.push(Task::Pipeline(pipeline));
                }
            }
            Task::Pipeline(pipeline) => {
                if let Some(decision) = pipeline_executed_decision(policy, pipeline) {
                    return decision;
                }
                for command in pipeline.seq.iter().rev() {
                    tasks.push(Task::Command(command));
                }
            }
            Task::Command(ast::Command::Simple(command)) => {
                let mut args = Vec::new();
                let mut words: Vec<&ast::Word> = Vec::new();
                let mut input_sources: Vec<(&str, bool)> = Vec::new();
                if let Some(prefix) = &command.prefix {
                    for item in &prefix.0 {
                        match item {
                            ast::CommandPrefixOrSuffixItem::Word(word) => {
                                if let Some(FloorDecision::Deny(reason)) =
                                    scan_ast_word(policy, word)
                                {
                                    return FloorDecision::Deny(reason);
                                }
                                args.push(word.value.clone());
                                words.push(word)
                            }
                            ast::CommandPrefixOrSuffixItem::AssignmentWord(_, _) => {}
                            ast::CommandPrefixOrSuffixItem::ProcessSubstitution(_, subshell) => {
                                tasks.push(Task::List(&subshell.list));
                            }
                            ast::CommandPrefixOrSuffixItem::IoRedirect(
                                ast::IoRedirect::HereDocument(_, here),
                            ) => {
                                input_sources.push((&here.doc.value, true));
                            }
                            ast::CommandPrefixOrSuffixItem::IoRedirect(
                                ast::IoRedirect::HereString(_, word),
                            ) => {
                                input_sources.push((&word.value, false));
                            }
                            ast::CommandPrefixOrSuffixItem::IoRedirect(_) => {}
                        }
                    }
                }
                if let Some(word) = &command.word_or_name {
                    if let Some(FloorDecision::Deny(reason)) = scan_ast_word(policy, word) {
                        return FloorDecision::Deny(reason);
                    }
                    args.push(word.value.clone());
                    words.push(word);
                }
                if let Some(suffix) = &command.suffix {
                    for item in &suffix.0 {
                        match item {
                            ast::CommandPrefixOrSuffixItem::Word(word) => {
                                if let Some(FloorDecision::Deny(reason)) =
                                    scan_ast_word(policy, word)
                                {
                                    return FloorDecision::Deny(reason);
                                }
                                args.push(word.value.clone());
                                words.push(word)
                            }
                            ast::CommandPrefixOrSuffixItem::AssignmentWord(_, _) => {}
                            ast::CommandPrefixOrSuffixItem::ProcessSubstitution(_, subshell) => {
                                tasks.push(Task::List(&subshell.list));
                            }
                            ast::CommandPrefixOrSuffixItem::IoRedirect(
                                ast::IoRedirect::HereDocument(_, here),
                            ) => {
                                input_sources.push((&here.doc.value, true));
                            }
                            ast::CommandPrefixOrSuffixItem::IoRedirect(
                                ast::IoRedirect::HereString(_, word),
                            ) => {
                                input_sources.push((&word.value, false));
                            }
                            ast::CommandPrefixOrSuffixItem::IoRedirect(_) => {}
                        }
                    }
                }
                let interpreter = unwrap_wrappers(args.clone())
                    .and_then(|args| args.first().and_then(|word| normalize_verb(word)))
                    .is_some_and(|verb| {
                        matches!(verb.as_str(), "bash" | "sh" | "zsh" | "eval" | "exec")
                    });
                for (source, heredoc) in input_sources {
                    if interpreter {
                        if let decision @ (FloorDecision::Deny(_) | FloorDecision::Warn(_)) =
                            scan_ast(policy, source)
                        {
                            return decision;
                        }
                    }
                    if heredoc {
                        let Ok(pieces) = word::parse_heredoc(source, &ParserOptions::default())
                        else {
                            return FloorDecision::Warn("floor-opaque-command".to_string());
                        };
                        if let Some(FloorDecision::Deny(reason)) = scan_word_pieces(policy, &pieces)
                        {
                            return FloorDecision::Deny(reason);
                        }
                    }
                }
                match scan_ast_args(policy, args.clone()) {
                    Some(FloorDecision::Deny(reason)) => return FloorDecision::Deny(reason),
                    Some(FloorDecision::Warn(reason)) => return FloorDecision::Warn(reason),
                    Some(FloorDecision::Allow) | None => {
                        justified_words.extend(justified_word_spans(&args, &words));
                    }
                }
            }
            Task::Command(ast::Command::Compound(command, _)) => {
                tasks.push(Task::Compound(command))
            }
            Task::Command(ast::Command::Function(function)) => {
                tasks.push(Task::Compound(&function.body.0))
            }
            Task::Command(ast::Command::ExtendedTest(_, _)) => {}
            Task::Compound(ast::CompoundCommand::Arithmetic(_)) => {}
            Task::Compound(ast::CompoundCommand::ArithmeticForClause(command)) => {
                tasks.push(Task::List(&command.body.list));
            }
            Task::Compound(ast::CompoundCommand::BraceGroup(command)) => {
                tasks.push(Task::List(&command.list))
            }
            Task::Compound(ast::CompoundCommand::Subshell(command)) => {
                tasks.push(Task::List(&command.list))
            }
            Task::Compound(ast::CompoundCommand::ForClause(command)) => {
                tasks.push(Task::List(&command.body.list))
            }
            Task::Compound(ast::CompoundCommand::CaseClause(command)) => {
                for case in command.cases.iter().rev() {
                    if let Some(list) = &case.cmd {
                        tasks.push(Task::List(list));
                    }
                }
            }
            Task::Compound(ast::CompoundCommand::IfClause(command)) => {
                if let Some(elses) = &command.elses {
                    for clause in elses.iter().rev() {
                        tasks.push(Task::List(&clause.body));
                        if let Some(condition) = &clause.condition {
                            tasks.push(Task::List(condition));
                        }
                    }
                }
                tasks.push(Task::List(&command.then));
                tasks.push(Task::List(&command.condition));
            }
            Task::Compound(ast::CompoundCommand::WhileClause(command))
            | Task::Compound(ast::CompoundCommand::UntilClause(command)) => {
                tasks.push(Task::List(&command.1.list));
                tasks.push(Task::List(&command.0));
            }
            Task::Compound(ast::CompoundCommand::Coprocess(command)) => {
                tasks.push(Task::Command(&command.body))
            }
        }
    }
    let _ = justified_words;
    FloorDecision::Allow
}

fn raw_syntax_complexity(input: &str) -> usize {
    let punctuation = ['{', '(', '`']
        .into_iter()
        .map(|needle| input.matches(needle).count())
        .sum::<usize>();
    let compound = ["$(", "${", "[["]
        .into_iter()
        .map(|needle| input.match_indices(needle).count())
        .sum::<usize>();
    let keywords = input
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .filter(|word| {
            matches!(
                *word,
                "if" | "while" | "until" | "for" | "case" | "select" | "function"
            )
        })
        .count();
    punctuation + compound + keywords
}

fn non_whitespace_token_count(input: &str) -> usize {
    let mut count = 0;
    let mut in_word = false;
    for ch in input.chars() {
        if ch.is_whitespace() {
            in_word = false;
        } else if ch.is_ascii_alphanumeric() || ch == '_' {
            if !in_word {
                count += 1;
                in_word = true;
            }
        } else {
            count += 1;
            in_word = false;
        }
    }
    count
}

fn targeted_occurrences(input: &str) -> Vec<Range<usize>> {
    let chars: Vec<_> = input.chars().collect();
    let mut occurrences = Vec::new();
    let is_word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    let bounded = |start: usize, end: usize| {
        !chars
            .get(start.wrapping_sub(1))
            .is_some_and(|ch| is_word(*ch))
            && !chars.get(end).is_some_and(|ch| is_word(*ch))
    };
    let mut index = 0;
    while index < chars.len() {
        let lower = chars[index].to_ascii_lowercase();
        let end = if lower == 'r'
            && chars
                .get(index + 1)
                .is_some_and(|ch| ch.eq_ignore_ascii_case(&'m'))
        {
            Some(index + 2)
        } else if lower == 'g'
            && chars
                .get(index + 1)
                .is_some_and(|ch| ch.eq_ignore_ascii_case(&'i'))
            && chars
                .get(index + 2)
                .is_some_and(|ch| ch.eq_ignore_ascii_case(&'t'))
        {
            Some(index + 3)
        } else if lower == 'r' && chars.get(index + 1) == Some(&'?') {
            Some(index + 2)
        } else if lower == 'r'
            && chars.get(index + 1) == Some(&'[')
            && chars
                .get(index + 2)
                .is_some_and(|ch| ch.eq_ignore_ascii_case(&'m'))
            && chars.get(index + 3) == Some(&']')
        {
            Some(index + 4)
        } else if lower == 'r'
            && chars.get(index + 1) == Some(&'\\')
            && chars
                .get(index + 2)
                .is_some_and(|ch| ch.eq_ignore_ascii_case(&'m'))
        {
            Some(index + 3)
        } else {
            None
        };
        if let Some(end) = end {
            if bounded(index, end) {
                occurrences.push(index..end);
            }
            index = end;
        } else {
            index += 1;
        }
    }
    occurrences
}

fn justified_word_spans(args: &[String], words: &[&ast::Word]) -> Vec<Range<usize>> {
    let Some(verb) = args
        .first()
        .and_then(|word| word_text(word))
        .and_then(|word| normalize_verb(&word))
    else {
        return Vec::new();
    };
    if !matches!(
        verb.as_str(),
        "rm" | "git" | "echo" | "printf" | "grep" | "rg" | "ls" | "cat" | "cargo" | "docker" | "gh"
    ) {
        return Vec::new();
    }
    words
        .iter()
        .enumerate()
        .filter_map(|(index, word)| {
            let static_word = word_text(&word.value).is_some();
            let quoted_message_heredoc = matches!(verb.as_str(), "git" | "gh")
                && args.get(..index).is_some_and(|prior| {
                    prior
                        .iter()
                        .any(|arg| matches!(arg.as_str(), "-m" | "-F" | "--message" | "--body"))
                })
                && word.value.contains("$(cat <<'");
            (static_word || quoted_message_heredoc)
                .then(|| word.loc.as_ref().map(|loc| loc.start.index..loc.end.index))
                .flatten()
        })
        .collect()
}

fn pipeline_executed_decision(
    policy: &FloorPolicy,
    pipeline: &ast::Pipeline,
) -> Option<FloorDecision> {
    let interpreter = pipeline.seq.iter().position(command_is_interpreter)?;
    let ast::Command::Simple(source) = pipeline.seq.get(interpreter.checked_sub(1)?)? else {
        return Some(FloorDecision::Warn("floor-opaque-command".to_string()));
    };
    let verb = source
        .word_or_name
        .as_ref()
        .and_then(|word| word_text(&word.value))
        .and_then(|word| normalize_verb(&word))?;
    let suffix = &source.suffix.as_ref()?.0;
    if verb == "cat" {
        let mut found_heredoc = false;
        for item in suffix {
            let ast::CommandPrefixOrSuffixItem::IoRedirect(ast::IoRedirect::HereDocument(_, here)) =
                item
            else {
                continue;
            };
            found_heredoc = true;
            if let decision @ (FloorDecision::Deny(_) | FloorDecision::Warn(_)) =
                scan_ast(policy, &here.doc.value)
            {
                return Some(decision);
            }
        }
        return Some(if found_heredoc {
            FloorDecision::Allow
        } else {
            FloorDecision::Warn("floor-opaque-command".to_string())
        });
    }
    if !matches!(verb.as_str(), "echo" | "printf") {
        return Some(FloorDecision::Warn("floor-opaque-command".to_string()));
    }
    let words = suffix.iter().filter_map(|item| match item {
        ast::CommandPrefixOrSuffixItem::Word(word) => word_text(&word.value),
        _ => None,
    });
    Some(scan_ast(policy, &words.collect::<Vec<_>>().join(" ")))
}

fn command_is_interpreter(command: &ast::Command) -> bool {
    let ast::Command::Simple(command) = command else {
        return false;
    };
    command
        .word_or_name
        .as_ref()
        .and_then(|word| word_text(&word.value))
        .and_then(|word| normalize_verb(&word))
        .is_some_and(|verb| {
            matches!(
                verb.as_str(),
                "bash" | "sh" | "zsh" | "cmd" | "powershell" | "pwsh" | "xargs"
            )
        })
}

fn scan_ast_word(policy: &FloorPolicy, word: &ast::Word) -> Option<FloorDecision> {
    word::parse(&word.value, &ParserOptions::default())
        .ok()
        .and_then(|pieces| scan_word_pieces(policy, &pieces))
}

fn scan_word_pieces(
    policy: &FloorPolicy,
    pieces: &[word::WordPieceWithSource],
) -> Option<FloorDecision> {
    for piece in pieces {
        match &piece.piece {
            word::WordPiece::CommandSubstitution(source)
            | word::WordPiece::BackquotedCommandSubstitution(source) => {
                let source = if source.ends_with('\n') {
                    source.to_string()
                } else {
                    format!("{source}\n")
                };
                if let decision @ (FloorDecision::Deny(_) | FloorDecision::Warn(_)) =
                    scan_ast(policy, &source)
                {
                    return Some(decision);
                }
            }
            word::WordPiece::DoubleQuotedSequence(children)
            | word::WordPiece::GettextDoubleQuotedSequence(children) => {
                if let Some(decision @ (FloorDecision::Deny(_) | FloorDecision::Warn(_))) =
                    scan_word_pieces(policy, children)
                {
                    return Some(decision);
                }
            }
            _ => {}
        }
    }
    Some(FloorDecision::Allow)
}

fn word_text(value: &str) -> Option<String> {
    fn pieces_text(pieces: &[word::WordPieceWithSource]) -> Option<String> {
        let mut output = String::new();
        for piece in pieces {
            match &piece.piece {
                word::WordPiece::Text(value)
                | word::WordPiece::SingleQuotedText(value)
                | word::WordPiece::AnsiCQuotedText(value)
                | word::WordPiece::EscapeSequence(value) => output.push_str(value),
                word::WordPiece::DoubleQuotedSequence(children)
                | word::WordPiece::GettextDoubleQuotedSequence(children) => {
                    output.push_str(&pieces_text(children)?);
                }
                _ => return None,
            }
        }
        Some(output)
    }

    pieces_text(&word::parse(value, &ParserOptions::default()).ok()?)
}

fn scan_rtk_program(policy: &FloorPolicy, args: &[String]) -> Option<FloorDecision> {
    let args = args
        .iter()
        .map(|word| word_text(word))
        .collect::<Option<Vec<_>>>()?;
    let mut rest = args.get(1..)?;
    while rest.first().is_some_and(|arg| {
        arg.chars().all(|ch| ch == '-' || ch == 'v')
            || matches!(arg.as_str(), "--ultra-compact" | "--skip-env")
    }) {
        rest = &rest[1..];
    }
    let dispatch = normalize_verb(rest.first()?)?;
    let source = match dispatch.as_str() {
        "git" => format!("git {}", rest[1..].join(" ")),
        "run" if matches!(rest.get(1).map(String::as_str), Some("-c" | "--command")) => {
            rest.get(2)?.to_string()
        }
        "run" => rest[1..].join(" "),
        "proxy" | "err" | "test" | "summary" => {
            let rest = &rest[1..];
            if rest
                .first()
                .is_some_and(|arg| arg.starts_with('-') && arg != "--")
            {
                return opaque_decision(&rest.join(" "));
            }
            rest.strip_prefix(&["--".to_string()])
                .unwrap_or(rest)
                .join(" ")
        }
        _ => return opaque_decision(&rest.join(" ")),
    };
    Some(scan_ast(policy, &source))
}

fn scan_executed_args(policy: &FloorPolicy, verb: &str, args: &[String]) -> FloorDecision {
    let source = match verb {
        "bash" | "sh" | "zsh" | "dash" | "ksh" => args
            .iter()
            .position(|arg| arg == "-c" || arg == "--command")
            .and_then(|index| args.get(index + 1))
            .cloned(),
        "eval" => Some(args[1..].join(" ")),
        "xargs" => args
            .iter()
            .skip(1)
            .find(|arg| !arg.starts_with('-'))
            .map(|_| args[1..].join(" ")),
        "find" => args
            .iter()
            .position(|arg| arg == "-exec")
            .map(|index| args[index + 1..].join(" ")),
        _ => None,
    };
    source
        .map(|source| scan_ast(policy, &source))
        .unwrap_or_else(|| FloorDecision::Warn("floor-opaque-command".to_string()))
}

fn scan_git_executed_args(policy: &FloorPolicy, args: &[String]) -> Option<FloorDecision> {
    let command = match args.get(1).map(String::as_str) {
        Some("rebase") => args
            .iter()
            .find_map(|arg| arg.strip_prefix("--exec=").map(str::to_owned))
            .or_else(|| {
                args.iter()
                    .position(|arg| arg == "-x" || arg == "--exec")
                    .and_then(|index| args.get(index + 1))
                    .cloned()
            }),
        Some("submodule") => args
            .iter()
            .position(|arg| arg == "foreach")
            .and_then(|index| args.get(index + 1))
            .cloned(),
        Some("bisect") => args
            .iter()
            .position(|arg| arg == "run")
            .map(|index| args[index + 1..].join(" ")),
        Some("difftool" | "mergetool") => args
            .iter()
            .find_map(|arg| arg.strip_prefix("--extcmd=").map(str::to_owned))
            .or_else(|| {
                args.iter()
                    .position(|arg| arg == "-x" || arg == "--extcmd")
                    .and_then(|index| args.get(index + 1))
                    .cloned()
            }),
        _ => return None,
    };
    Some(
        command
            .map(|command| scan_ast(policy, &command))
            .unwrap_or_else(|| FloorDecision::Warn("floor-opaque-command".to_string())),
    )
}

fn scan_ast_args(policy: &FloorPolicy, args: Vec<String>) -> Option<FloorDecision> {
    if args.len() > 4096 {
        return Some(FloorDecision::Warn("floor-budget-exceeded".to_string()));
    }
    let command = args.first().and_then(|word| word_text(word));
    let verb = command.as_deref().and_then(normalize_verb);
    if matches!(verb.as_deref(), Some("rm" | "git"))
        && args[1..].iter().enumerate().any(|(index, arg)| {
            let message_data = verb.as_deref() == Some("git")
                && index > 0
                && matches!(args[index].as_str(), "-m" | "-F" | "--message" | "--body");
            !message_data && (word_text(arg).is_none() || arg.contains(['{', '}']))
        })
    {
        return Some(FloorDecision::Warn("floor-dynamic-argument".to_string()));
    }
    let mut args = args
        .iter()
        .enumerate()
        .map(|(index, word)| {
            word_text(word).map(|word| {
                if index == 0 {
                    word
                } else {
                    word.replace('\\', "")
                }
            })
        })
        .collect::<Option<Vec<_>>>()?;
    for _ in 0..4096 {
        let Some(command_word) = args.first().and_then(|word| word_text(word)) else {
            return Some(FloorDecision::Allow);
        };
        if command_word.contains(['*', '?', '[', ']']) {
            return Some(FloorDecision::Warn("floor-opaque-command".to_string()));
        }
        let Some(verb) = normalize_verb(&command_word) else {
            return Some(FloorDecision::Allow);
        };
        if verb == "find" && !args.iter().any(|arg| arg == "-exec") {
            return Some(FloorDecision::Allow);
        }
        if matches!(
            verb.as_str(),
            "bash" | "sh" | "zsh" | "dash" | "ksh" | "eval" | "xargs" | "find"
        ) {
            return Some(scan_executed_args(policy, &verb, &args));
        }
        if verb == "rtk" {
            return scan_rtk_program(policy, &args);
        }
        args = match unwrap_wrappers(args) {
            Some(args) => args,
            None => return Some(FloorDecision::Warn("floor-opaque-command".to_string())),
        };
        let Some(command_word) = args.first().and_then(|word| word_text(word)) else {
            return Some(FloorDecision::Allow);
        };
        if command_word.contains(['*', '?', '[', ']']) {
            return Some(FloorDecision::Warn("floor-opaque-command".to_string()));
        }
        let Some(verb) = normalize_verb(&command_word) else {
            return Some(FloorDecision::Allow);
        };
        if verb == "rtk" {
            continue;
        }
        if verb == "git" {
            if let Some(decision) = scan_git_executed_args(policy, &args) {
                return Some(decision);
            }
        }
        let hit = match verb.as_str() {
            "rm" => classify_rm(policy, &args[1..]),
            "git" => classify_git(policy, &args[1..]),
            value if value.starts_with('-') => return opaque_decision(&args.join(" ")),
            _ => None,
        };
        return Some(match hit.as_deref() {
            Some("floor-opaque-command") => FloorDecision::Warn("floor-opaque-command".to_string()),
            Some(reason) => FloorDecision::Deny(reason.to_string()),
            None => FloorDecision::Allow,
        });
    }
    Some(FloorDecision::Warn("floor-budget-exceeded".to_string()))
}

fn opaque_decision(input: &str) -> Option<FloorDecision> {
    (!targeted_occurrences(input).is_empty())
        .then(|| FloorDecision::Warn("floor-opaque-command".to_string()))
        .or(Some(FloorDecision::Allow))
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
    let name = if token.contains(":\\") || token.starts_with("\\\\") {
        token.rsplit(['/', '\\']).next()?
    } else {
        token.rsplit('/').next()?.trim_matches('\\')
    };
    let name = name.replace('\\', "").to_ascii_lowercase();
    Some(name.strip_suffix(".exe").unwrap_or(&name).to_string())
}

fn unwrap_wrappers(mut args: Vec<String>) -> Option<Vec<String>> {
    for _ in 0..8 {
        let verb = normalize_verb(args.first()?)?;
        match verb.as_str() {
            "nohup" | "exec" => {
                args.remove(0);
                if args.first().is_some_and(|arg| arg == "--") {
                    args.remove(0);
                }
            }
            "env" => {
                args.remove(0);
                while args
                    .first()
                    .is_some_and(|arg| arg.starts_with('-') || arg.contains('='))
                {
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
                while args.first().is_some_and(|arg| arg.starts_with('-')) {
                    args.remove(0);
                }
            }
            "rtk" => return Some(args),
            _ => return Some(args),
        }
    }
    None
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
                        'i' | 'v' => {}
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
            "-C" | "--git-dir" | "--work-tree" => {
                if args.len() < 2 {
                    return Some("floor-opaque-command".to_string());
                }
                args = &args[2..];
            }
            "-c" => return Some("floor-opaque-command".to_string()),
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
        "submodule" | "filter-branch" => Some("floor-opaque-command".to_string()),
        "rebase"
            if rest
                .iter()
                .any(|arg| matches!(arg.as_str(), "-x" | "--exec")) =>
        {
            Some("floor-opaque-command".to_string())
        }
        "bisect" if matches!(rest.first().map(String::as_str), Some("run")) => {
            Some("floor-opaque-command".to_string())
        }
        "config" if rest.iter().any(|arg| arg.starts_with("alias.")) => {
            Some("floor-opaque-command".to_string())
        }
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
    fn rejects_substitutions_but_allows_single_quoted_data() {
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
    fn scans_wrappers_and_rtk_dispatch_with_exact_rule_ids() {
        for command in [
            "nohup rm -rf /etc",
            "time -p rm -rf /etc",
            "command -- rm -rf /etc",
            "timeout -s KILL 5 rm -rf /etc",
            "nice -n 5 rm -rf /etc",
            "stdbuf -oL rm -rf /etc",
            "sudo -- rm -rf /etc",
            "doas -- rm -rf /etc",
        ] {
            assert_eq!(
                policy().evaluate(command),
                FloorDecision::Deny("rm-recursive".into()),
                "{command}"
            );
        }
        for command in [
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
    fn denies_opaque_regions_and_parse_failures() {
        assert_eq!(
            policy().evaluate("echo $(unterminated"),
            FloorDecision::Warn("floor-opaque-command".into())
        );
        assert_eq!(
            policy().evaluate("echo $(rm -rf /etc"),
            FloorDecision::Warn("floor-opaque-command".into())
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
            FloorDecision::Warn("floor-opaque-command".into())
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
        ] {
            assert_eq!(
                policy().evaluate(command),
                FloorDecision::Deny("rm-recursive".into()),
                "{command}"
            );
        }
        assert_eq!(
            policy().evaluate("for d in a b; do rm -rf \"$d\"; done"),
            FloorDecision::Warn("floor-dynamic-argument".into())
        );
        assert_eq!(
            policy().evaluate("while false; do git push -f; done"),
            FloorDecision::Deny("git-rewrite".into())
        );
        for command in [
            "env rm -rf x",
            "exec rm -rf x",
            "bash -c 'rm -rf x'",
            "eval \"rm -rf x\"",
            "xargs rm -rf",
        ] {
            assert_eq!(
                policy().evaluate(command),
                FloorDecision::Deny("rm-recursive".into()),
                "{command}"
            );
        }
        assert_eq!(
            policy().evaluate("find . -name t -exec rm -rf {} +"),
            FloorDecision::Warn("floor-dynamic-argument".into())
        );
        assert_eq!(
            policy().evaluate("sh -c \"git push -f\""),
            FloorDecision::Deny("git-rewrite".into())
        );
        assert_eq!(
            policy().evaluate("bash -c 'echo safe'"),
            FloorDecision::Allow
        );
    }

    #[test]
    fn recursively_scans_rtk_carried_commands() {
        for command in ["rtk run 'rm -rf x'", "rtk run -c 'echo a; rm -rf x'"] {
            assert_eq!(
                policy().evaluate(command),
                FloorDecision::Deny("rm-recursive".into()),
                "{command}"
            );
        }
        for command in ["rtk run -c 'nohup rm -rf x'", "rtk proxy nohup rm -rf x"] {
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
            FloorDecision::Deny("rm-recursive".into())
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
                FloorDecision::Warn("floor-opaque-command".into()),
                "{command}"
            );
        }
        assert_eq!(
            policy().evaluate("time -p -- rm -rf /etc"),
            FloorDecision::Warn("floor-opaque-command".into())
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
            FloorDecision::Warn("floor-budget-exceeded".into())
        );
        assert_eq!(
            policy().evaluate(&"true ".repeat(4097)),
            FloorDecision::Warn("floor-budget-exceeded".into())
        );
        assert_eq!(
            policy().evaluate(&format!("[[ {} ]]", "!".repeat(4097))),
            FloorDecision::Warn("floor-budget-exceeded".into())
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

    #[test]
    fn denies_deep_rtk_wrapping_without_recursing() {
        let command = "rtk run ".repeat(300) + "rm -rf x";
        assert_eq!(
            policy().evaluate(&command),
            FloorDecision::Deny("rm-recursive".to_string())
        );
    }

    #[test]
    fn denies_deep_command_substitution_without_recursing() {
        let command = "$(".repeat(40_000) + "rm -rf x" + &")".repeat(40_000);
        assert_eq!(
            policy().evaluate(&command),
            FloorDecision::Warn("floor-budget-exceeded".to_string())
        );
    }

    #[test]
    fn reviewed_probe_cases_keep_their_exact_decisions() {
        let cases: &[(&str, Option<&str>)] = &[
            (
                r#"git commit -m "$(cat <<'EOF'
feat: x

Body.
EOF
)""#,
                None,
            ),
            (r#"git commit -m "feat: a; b""#, None),
            (r#"git log --format="%h|%s""#, None),
            ("echo \"a;b\" ; rm -rf /etc", Some("rm-recursive")),
            ("FOO=1 rm -rf x", Some("rm-recursive")),
            ("env rm -rf x", Some("rm-recursive")),
            ("exec rm -rf x", Some("rm-recursive")),
            ("! rm -rf x", Some("rm-recursive")),
            ("(rm -rf x)", Some("rm-recursive")),
            ("{ rm -rf x; }", Some("rm-recursive")),
            ("if true; then rm -rf x; fi", Some("rm-recursive")),
            (
                "for d in a b; do rm -rf \"$d\"; done",
                Some("floor-dynamic-argument"),
            ),
            ("while false; do git push -f; done", Some("git-rewrite")),
            ("bash -c 'rm -rf x'", Some("rm-recursive")),
            ("sh -c \"git push -f\"", Some("git-rewrite")),
            ("eval \"rm -rf x\"", Some("rm-recursive")),
            ("xargs rm -rf", Some("rm-recursive")),
            (
                "find . -name t -exec rm -rf {} +",
                Some("floor-dynamic-argument"),
            ),
            ("for f in *.rs; do echo $f; done", None),
            ("bash -c 'echo hi'", None),
            ("find . -name '*.rs'", None),
            ("rtk run 'rm -rf x'", Some("rm-recursive")),
            ("rtk run -c 'echo a; rm -rf x'", Some("rm-recursive")),
            ("rtk run -c 'nohup rm -rf x'", Some("rm-recursive")),
            ("rtk proxy nohup rm -rf x", Some("rm-recursive")),
            ("rtk -vv git push -f origin main", Some("git-rewrite")),
            ("bash <<'EOF'\nrm -rf x\nEOF", Some("rm-recursive")),
            (
                "cat <<EOF > notes.md\ngit push --force was refused\nEOF",
                None,
            ),
            ("git clean -n --exclude=*.pdf", None),
            ("git -P push -f", Some("floor-opaque-command")),
            ("git --bare push --force", Some("floor-opaque-command")),
            ("git push --bogus origin main", Some("floor-opaque-command")),
            ("time -p -- rm -rf /etc", Some("floor-opaque-command")),
            ("nice -5 rm -rf /etc", Some("floor-opaque-command")),
            (
                "timeout --bogus 5 rm -rf /etc",
                Some("floor-opaque-command"),
            ),
            ("timeout -k5 10 rm -rf x", Some("floor-opaque-command")),
            ("rm --force fichier.txt", None),
            ("cargo build 2>&1 | tail", None),
            ("FOO=1 cargo test", None),
            ("python x.py", None),
            ("rm fichier.txt", None),
            ("git push", None),
            ("git push -u origin branche", None),
            (
                "gh pr create --body \"$(cat <<'EOF'\nSummary\n\n- item\nEOF\n)\"",
                None,
            ),
            ("cd /tmp && ls", None),
            ("echo \\\" ; rm -rf /etc ; echo \\\"", Some("rm-recursive")),
            ("echo \\\' ; rm -rf /etc ; echo \\\'", Some("rm-recursive")),
            ("cat <<EOF; rm -rf /etc\nbody\nEOF", Some("rm-recursive")),
            ("cat <<'EOF' | sh\nrm -rf /etc\nEOF", Some("rm-recursive")),
            ("cat <<'EOF' | bash\nrm -rf /etc\nEOF", Some("rm-recursive")),
            ("# <<'EOF'\nrm -rf /etc\nEOF", Some("rm-recursive")),
            (
                "sudo -- bash <<'EOF'\nrm -rf /etc\nEOF",
                Some("rm-recursive"),
            ),
            ("FOO=1 bash <<'EOF'\nrm -rf /etc\nEOF", Some("rm-recursive")),
            ("echo 'rm -rf /etc' | sh", Some("rm-recursive")),
            ("cat <(rm -rf /etc)", Some("rm-recursive")),
            ("diff <(git push -f origin main) x", Some("git-rewrite")),
            ("if rm -rf /etc; then :; fi", Some("rm-recursive")),
            ("while git push -f; do :; done", Some("git-rewrite")),
            (
                "if false; then :; else rm -rf /etc; fi",
                Some("rm-recursive"),
            ),
            ("case x in x) rm -rf /etc;; esac", Some("rm-recursive")),
            ("f() { rm -rf /etc; }; f", Some("rm-recursive")),
            ("function f { rm -rf /etc; }; f", Some("rm-recursive")),
            ("coproc rm -rf /etc", Some("rm-recursive")),
            ("/usr/bin/r? -rf /etc", Some("floor-opaque-command")),
            ("/usr/bin/r[m] -rf /etc", Some("floor-opaque-command")),
            (
                "git -c alias.p='push --force' p origin main",
                Some("floor-opaque-command"),
            ),
            ("grep foo <<< \"$x\"", None),
            ("cat <<< hello", None),
            ("rm -v a.txt", None),
            ("rm -i a.txt", None),
            ("git commit -m \"msg\" && git push", None),
            ("echo \"$((1+2))\"", None),
            ("ls # a comment; rm -rf x", None),
            ("git status; echo done", None),
            (
                "cat <<EOF && git push -f origin main\nbody\nEOF",
                Some("git-rewrite"),
            ),
            ("git commit -F - <<EOF\nsubject\nEOF", None),
            ("nohup rtk run 'rm -rf /etc'", Some("rm-recursive")),
            ("time rtk run 'rm -rf /etc'", Some("rm-recursive")),
            (
                "timeout 5 rtk run -c 'echo a; rm -rf /etc'",
                Some("rm-recursive"),
            ),
            ("rtk -v run 'rm -rf /etc'", Some("rm-recursive")),
            ("RM -rf /x", Some("rm-recursive")),
            ("r\\m -rf /x", Some("rm-recursive")),
            ("\"rm\" -rf /x", Some("rm-recursive")),
            ("rm -r -f /x", Some("rm-recursive")),
            ("rm --recursive --force /x", Some("rm-recursive")),
            ("rm -rf \"/etc\"", Some("rm-recursive")),
            ("rm -rf /tmp /etc", Some("rm-recursive")),
            ("git push origin +main", Some("git-rewrite")),
            ("git push -qf", Some("git-rewrite")),
            ("git push origin :main", Some("git-rewrite")),
            ("git push --delete origin x", Some("git-rewrite")),
            ("git -C x push -f", Some("git-rewrite")),
            ("nohup rm -rf x", Some("rm-recursive")),
            ("timeout 5 rm -rf x", Some("rm-recursive")),
            ("rtk proxy rm -rf x", Some("rm-recursive")),
            ("echo \"$(rm -rf /tmp/x)\"", Some("rm-recursive")),
            ("`rm -rf x`", Some("rm-recursive")),
            ("echo \"<<'X'\"\nrm -rf x", Some("rm-recursive")),
            (
                "\"C:\\Program Files\\Git\\usr\\bin\\rm.exe\" -rf x",
                Some("rm-recursive"),
            ),
            (
                r##"echo don\'t $(rm -rf x) \"##,
                Some("floor-opaque-command"),
            ),
            ("rm -rf x 2>/dev/null", Some("rm-recursive")),
            ("cd /tmp && rm -rf build", Some("rm-recursive")),
            ("rm -r inexistant", Some("rm-recursive")),
            ("git commit -m \"fix a & b\"", None),
            ("git log --format=\"%h|%s\" -5", None),
            ("git commit -m \"title\n\nbody line\"", None),
            ("grep -E \"foo|bar\" src/git", None),
            ("rg \"a|b\" kleos-sh/src", None),
            ("git clean -n --exclude=*.conf", None),
            ("rm -f a.txt", None),
            ("git status && git diff | head", None),
            ("echo $(unterminated", Some("floor-opaque-command")),
            (
                "git commit -F - <<EOF\nsubject\n\ngit push --force note\nEOF",
                None,
            ),
            ("ls ~/.kleos", None),
            (
                "git commit -m \"$(cat <<'EOF'\nfeat: add thing\nEOF\n)\" && git push",
                None,
            ),
            ("python - <<'EOF'\nprint(1)\nEOF", None),
            (
                "gh pr create --title t --body \"$(cat <<'EOF'\nSummary\nEOF\n)\"",
                None,
            ),
            ("git tag -a v1 -m \"release\nnotes\"", None),
            ("rm -rf", Some("rm-recursive")),
            ("rm -R x", Some("rm-recursive")),
            ("git push --force-with-lease", Some("git-rewrite")),
            ("git push origin main --no-verify", None),
            ("git reset --hard", Some("git-reset-hard")),
            ("git reset HEAD --hard", Some("git-reset-hard")),
            ("git clean -fd", Some("git-clean-force")),
            ("git clean -xn", None),
            ("sudo -u root ls ; rm -rf /etc", Some("rm-recursive")),
            ("FOO=bar rm -rf /etc", Some("rm-recursive")),
            ("git push origin main --mirror", Some("git-rewrite")),
            ("python -c \"print(1)\"", None),
            ("echo 'x;y' && git push -f origin main", Some("git-rewrite")),
            ("git reset --bogus --hard", Some("floor-opaque-command")),
        ];
        for (command, expected_reason) in cases {
            let expected = expected_reason.map_or(FloorDecision::Allow, |reason| {
                if matches!(reason, "floor-opaque-command" | "floor-dynamic-argument") {
                    FloorDecision::Warn(reason.to_string())
                } else {
                    FloorDecision::Deny(reason.to_string())
                }
            });
            assert_eq!(policy().evaluate(command), expected, "{command}");
        }

        let floor = Path::new("C:/gate/floor.toml");
        let binary = Path::new("C:/bin/kleos-sh.exe");
        let payload_cases: &[(&str, Option<&str>)] = &[
            (
                r#"{"tool_name":"Bash","tool_input":{"command":"rm -rf x"}}"#,
                Some("floor-invalid-payload"),
            ),
            (
                r#"{"hook_event_name":"PostToolUse","tool_name":"Bash","tool_input":{"command":"ls"}}"#,
                Some("floor-invalid-payload"),
            ),
            (
                r#"{"hook_event_name":"PreToolUse","tool_name":"Write","tool_input":{"command":"ls"}}"#,
                Some("floor-invalid-payload"),
            ),
            (
                r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"ls"}}"#,
                None,
            ),
            (
                r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"cat C:/gate/floor.toml"}}"#,
                Some("floor-self-protection"),
            ),
            (
                r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"echo x 2>C:/gate/floor.toml"}}"#,
                Some("floor-self-protection"),
            ),
            (
                r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"echo $(cat C:/gate/floor.toml)"}}"#,
                Some("floor-self-protection"),
            ),
            (
                r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"cp x C:/bin/kleos-sh.exe"}}"#,
                Some("floor-self-protection"),
            ),
        ];
        for (payload, expected_reason) in payload_cases {
            let expected = expected_reason.map_or(FloorDecision::Allow, |reason| {
                FloorDecision::Deny(reason.to_string())
            });
            assert_eq!(
                evaluate_payload(&policy(), payload, floor, binary),
                expected,
                "{payload}"
            );
        }
    }

    #[test]
    fn leaves_dangerous_command_mentions_as_data() {
        for command in [
            "echo 'rm -rf x'",
            "grep -rn 'git push -f' src",
            "kleos-cli store 'rm -rf x'",
        ] {
            assert_eq!(
                policy().evaluate(command),
                FloorDecision::Allow,
                "{command}"
            );
        }

        let unicode_prefix = "é".repeat(30);
        assert_eq!(
            policy().evaluate(&format!("echo '{unicode_prefix} rm'")),
            FloorDecision::Allow
        );
        assert!(matches!(
            policy().evaluate(&format!("echo {unicode_prefix}; echo $(rm -rf build)")),
            FloorDecision::Deny(_)
        ));
    }

    #[test]
    fn recognition_dequotes_dangers_and_abstains_on_mentions_or_dynamic_arguments() {
        for command in [
            "rm \"-rf\" x",
            "rm '-rf' x",
            "rm \\-rf x",
            "rm $'-rf' x",
            "git 'push' -f",
            "git push origin '+main'",
            "git reset '--hard'",
            "git clean '-fd'",
        ] {
            assert_eq!(
                policy().evaluate(command),
                FloorDecision::Deny(match command {
                    value if value.starts_with("rm") => "rm-recursive".into(),
                    value if value.starts_with("git push") || value == "git 'push' -f" => {
                        "git-rewrite".into()
                    }
                    value if value.starts_with("git reset") => "git-reset-hard".into(),
                    _ => "git-clean-force".into(),
                }),
                "{command}"
            );
        }
        for (command, expected) in [
            ("git rebase -x 'rm -rf x' main", "rm-recursive"),
            ("git submodule foreach 'git push -f'", "git-rewrite"),
            ("git bisect run rm -rf x", "rm-recursive"),
            ("git difftool --extcmd='rm -rf x'", "rm-recursive"),
        ] {
            assert_eq!(
                policy().evaluate(command),
                FloorDecision::Deny(expected.into()),
                "{command}"
            );
        }
        for command in [
            "echo \"rm -rf\"",
            "grep -rn \"rm -rf\" src",
            "kleos-cli store \"git push --force\"",
        ] {
            assert_eq!(
                policy().evaluate(command),
                FloorDecision::Allow,
                "{command}"
            );
        }
        assert_eq!(
            policy().evaluate("F=-rf; rm $F x"),
            FloorDecision::Warn("floor-dynamic-argument".into())
        );
    }

    #[test]
    #[ignore = "requires FLOOR_CORPUS_JSONL"]
    fn corpus_refusal_rate_without_targeted_verbs_is_bounded() {
        let corpus = std::env::var("FLOOR_CORPUS_JSONL").expect("FLOOR_CORPUS_JSONL");
        let input = std::fs::read_to_string(corpus).expect("read corpus");
        let mut eligible = 0usize;
        let mut denial_count = 0usize;
        let mut denied = Vec::new();
        for line in input.lines() {
            let command: String = serde_json::from_str(line).expect("JSON string");
            let lowercase = command.to_ascii_lowercase();
            if lowercase.contains("rm") || lowercase.contains("git") {
                continue;
            }
            eligible += 1;
            if let FloorDecision::Deny(reason) = policy().evaluate(&command) {
                denial_count += 1;
                if denied.len() < 20 {
                    denied.push(format!("{reason}: {command}"));
                }
            }
        }
        println!("eligible={eligible} denied={denial_count} samples={denied:?}");
        assert!(
            denial_count * 100 <= eligible,
            "{denial_count} refus sur {eligible}; exemples: {denied:?}"
        );
    }
}
