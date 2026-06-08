//! Shell command permission rule matching.
//!
//! Mirrors Claude Code's `src/utils/permissions/shellRuleMatching.ts`.
//!
//! Permission rules for shell tools (run_command / Bash) can specify:
//! - Exact commands: `"npm run build"` — matches only that exact command
//! - Prefix rules: `"git:*"` (legacy :* suffix) — matches commands starting with "git"
//! - Wildcard patterns: `"npm * --save"` — glob-style matching with * as wildcard
//!
//! The matching is case-sensitive (shell commands are case-sensitive on all platforms).

/// Parsed shell permission rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellPermissionRule {
    /// Exact match — command must equal the rule text.
    Exact { command: String },
    /// Prefix match — command must start with the prefix.
    /// Legacy `:*` syntax (e.g., `"git:*"` matches "git", "git add", etc.)
    Prefix { prefix: String },
    /// Wildcard pattern — `*` matches any sequence of characters.
    /// Use `\*` to match a literal asterisk.
    Wildcard { pattern: String },
}

/// Parse a rule content string into a structured rule.
///
/// # Classification logic:
/// 1. If ends with `:*` → Prefix rule (legacy syntax)
/// 2. If contains unescaped `*` (and not legacy `:*`) → Wildcard rule
/// 3. Otherwise → Exact match
pub fn parse_shell_rule(content: &str) -> ShellPermissionRule {
    // Check for legacy :* prefix syntax
    if let Some(prefix) = extract_prefix(content) {
        return ShellPermissionRule::Prefix { prefix };
    }

    // Check for wildcard (* that isn't escaped)
    if has_unescaped_wildcards(content) {
        return ShellPermissionRule::Wildcard {
            pattern: content.to_string(),
        };
    }

    // Otherwise exact match
    ShellPermissionRule::Exact {
        command: content.to_string(),
    }
}

/// Match a command against a parsed shell permission rule.
pub fn matches_shell_rule(rule: &ShellPermissionRule, command: &str) -> bool {
    match rule {
        ShellPermissionRule::Exact { command: rule_cmd } => command == rule_cmd,
        ShellPermissionRule::Prefix { prefix } => {
            // "git" matches "git", "git add", "git push origin main"
            // but NOT "gitfoo" (must be followed by space or end)
            command == prefix || command.starts_with(&format!("{} ", prefix))
        }
        ShellPermissionRule::Wildcard { pattern } => match_wildcard_pattern(pattern, command),
    }
}

/// Extract prefix from legacy `:*` syntax. Returns `Some("git")` for `"git:*"`.
fn extract_prefix(rule: &str) -> Option<String> {
    if rule.ends_with(":*") && rule.len() > 2 {
        Some(rule[..rule.len() - 2].to_string())
    } else {
        None
    }
}

/// Check if a pattern contains unescaped wildcards (not legacy :* syntax).
fn has_unescaped_wildcards(pattern: &str) -> bool {
    if pattern.ends_with(":*") {
        return false;
    }
    let bytes = pattern.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'*' {
            // Count preceding backslashes
            let mut backslash_count = 0;
            let mut j = i as isize - 1;
            while j >= 0 && bytes[j as usize] == b'\\' {
                backslash_count += 1;
                j -= 1;
            }
            // Even number of backslashes = unescaped star
            if backslash_count % 2 == 0 {
                return true;
            }
        }
    }
    false
}

/// Match a command against a wildcard pattern.
///
/// `*` matches any sequence of characters (including newlines).
/// `\*` matches a literal asterisk.
/// `\\` matches a literal backslash.
///
/// Special behavior: when a pattern ends with ` *` (space + single unescaped
/// wildcard) and it's the ONLY unescaped wildcard, the trailing space+args
/// become optional. So `git *` matches both `git add` and bare `git`.
fn match_wildcard_pattern(pattern: &str, command: &str) -> bool {
    let trimmed = pattern.trim();

    // Process the pattern: handle escape sequences, build regex
    let mut processed = String::new();
    let mut i = 0;
    let chars: Vec<char> = trimmed.chars().collect();

    while i < chars.len() {
        let ch = chars[i];
        if ch == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if next == '*' {
                processed.push('\x01'); // placeholder for literal *
                i += 2;
                continue;
            } else if next == '\\' {
                processed.push('\x02'); // placeholder for literal \
                i += 2;
                continue;
            }
        }
        processed.push(ch);
        i += 1;
    }

    // Escape regex special characters except *
    let mut regex_str = String::new();
    for ch in processed.chars() {
        match ch {
            '*' => regex_str.push_str(".*"),
            '\x01' => regex_str.push_str("\\*"),  // literal *
            '\x02' => regex_str.push_str("\\\\"), // literal \
            '.' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' => {
                regex_str.push('\\');
                regex_str.push(ch);
            }
            _ => regex_str.push(ch),
        }
    }

    // Special: if pattern ends with ' *' and only one unescaped wildcard,
    // make the trailing " .*" optional so "git *" matches bare "git"
    let unescaped_star_count = processed.chars().filter(|&c| c == '*').count();
    if regex_str.ends_with(" .*") && unescaped_star_count == 1 {
        let base = &regex_str[..regex_str.len() - 3];
        regex_str = format!("{}( .*)?", base);
    }

    // Build regex matching the entire string
    let full_regex = format!("(?s)^{}$", regex_str);
    match regex::Regex::new(&full_regex) {
        Ok(re) => re.is_match(command),
        Err(_) => false, // malformed pattern → no match (safe fallback)
    }
}

/// Split a shell command line into its constituent sub-commands for
/// permission matching.
///
/// Splits on the shell operators `&&`, `||`, `|`, `;`, and newlines. This is
/// the anti-bypass step for allow rules: without it, an allow rule for
/// `git status` would also green-light `git status && rm -rf /`. Each
/// sub-command is trimmed; empty fragments are dropped.
///
/// Command substitution (`$( … )` and backticks) is flagged separately via
/// [`has_command_substitution`] because the substituted text is itself a
/// command that our simple splitter can't safely decompose — callers treat
/// its presence as "cannot prove safe" and fall back to asking.
///
/// This is a pragmatic lexer, not a full POSIX shell parser: it respects
/// single/double quotes so operators inside quotes are not treated as
/// separators, but it does not attempt to model every shell construct. When
/// in doubt it errs toward MORE fragments (safer for allow-matching, which
/// requires every fragment to be covered).
pub fn split_shell_command(command: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = command.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;

    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double => {
                in_single = !in_single;
                current.push(c);
            }
            '"' if !in_single => {
                in_double = !in_double;
                current.push(c);
            }
            _ if in_single || in_double => current.push(c),
            // `&&` / `&`
            '&' => {
                if chars.peek() == Some(&'&') {
                    chars.next();
                }
                push_trimmed(&mut parts, &mut current);
            }
            // `||` / `|`
            '|' => {
                if chars.peek() == Some(&'|') {
                    chars.next();
                }
                push_trimmed(&mut parts, &mut current);
            }
            ';' | '\n' => push_trimmed(&mut parts, &mut current),
            _ => current.push(c),
        }
    }
    push_trimmed(&mut parts, &mut current);
    parts
}

fn push_trimmed(parts: &mut Vec<String>, current: &mut String) {
    let t = current.trim();
    if !t.is_empty() {
        parts.push(t.to_string());
    }
    current.clear();
}

/// Detect shell command substitution (`$(…)` or backticks) outside single
/// quotes. Its presence means the command launches *another* command whose
/// text we can't statically match, so permission matching must not treat the
/// outer command as fully covered by an allow rule.
pub fn has_command_substitution(command: &str) -> bool {
    let mut in_single = false;
    let mut prev = '\0';
    let mut chars = command.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => in_single = !in_single,
            '`' if !in_single => return true,
            '$' if !in_single && chars.peek() == Some(&'(') => return true,
            _ => {}
        }
        prev = c;
    }
    let _ = prev;
    false
}

#[cfg(test)]
mod split_tests {
    use super::*;

    #[test]
    fn single_command_one_part() {
        assert_eq!(split_shell_command("git status"), vec!["git status"]);
    }

    #[test]
    fn splits_on_and_and() {
        assert_eq!(
            split_shell_command("git status && rm -rf /"),
            vec!["git status", "rm -rf /"]
        );
    }

    #[test]
    fn splits_on_pipe_semicolon_or() {
        assert_eq!(
            split_shell_command("a | b ; c || d"),
            vec!["a", "b", "c", "d"]
        );
    }

    #[test]
    fn does_not_split_inside_quotes() {
        assert_eq!(
            split_shell_command("echo 'a && b' ; ls"),
            vec!["echo 'a && b'", "ls"]
        );
        assert_eq!(
            split_shell_command(r#"echo "x | y""#),
            vec![r#"echo "x | y""#]
        );
    }

    #[test]
    fn detects_command_substitution() {
        assert!(has_command_substitution("echo $(whoami)"));
        assert!(has_command_substitution("echo `id`"));
        assert!(!has_command_substitution("echo hello"));
        assert!(!has_command_substitution("echo '$(not real)'"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_shell_rule ---

    #[test]
    fn parse_exact_rule() {
        assert_eq!(
            parse_shell_rule("npm run build"),
            ShellPermissionRule::Exact {
                command: "npm run build".into()
            }
        );
    }

    #[test]
    fn parse_prefix_rule() {
        assert_eq!(
            parse_shell_rule("git:*"),
            ShellPermissionRule::Prefix {
                prefix: "git".into()
            }
        );
    }

    #[test]
    fn parse_wildcard_rule() {
        assert_eq!(
            parse_shell_rule("npm * --save"),
            ShellPermissionRule::Wildcard {
                pattern: "npm * --save".into()
            }
        );
    }

    #[test]
    fn parse_single_star_is_wildcard() {
        assert_eq!(
            parse_shell_rule("cargo *"),
            ShellPermissionRule::Wildcard {
                pattern: "cargo *".into()
            }
        );
    }

    #[test]
    fn parse_escaped_star_is_exact() {
        // \* is escaped, so no unescaped wildcard → exact
        assert_eq!(
            parse_shell_rule(r"echo \*"),
            ShellPermissionRule::Exact {
                command: r"echo \*".into()
            }
        );
    }

    // --- matches_shell_rule ---

    #[test]
    fn exact_matches_itself() {
        let rule = parse_shell_rule("npm run build");
        assert!(matches_shell_rule(&rule, "npm run build"));
    }

    #[test]
    fn exact_does_not_match_different() {
        let rule = parse_shell_rule("npm run build");
        assert!(!matches_shell_rule(&rule, "npm run test"));
    }

    #[test]
    fn prefix_matches_command_with_args() {
        let rule = parse_shell_rule("git:*");
        assert!(matches_shell_rule(&rule, "git status"));
        assert!(matches_shell_rule(&rule, "git push origin main"));
    }

    #[test]
    fn prefix_matches_bare_command() {
        let rule = parse_shell_rule("git:*");
        assert!(matches_shell_rule(&rule, "git"));
    }

    #[test]
    fn prefix_does_not_match_substring() {
        let rule = parse_shell_rule("git:*");
        assert!(!matches_shell_rule(&rule, "gitfoo"));
        assert!(!matches_shell_rule(&rule, "github"));
    }

    #[test]
    fn wildcard_matches_any_args() {
        let rule = parse_shell_rule("npm * --save");
        assert!(matches_shell_rule(&rule, "npm install lodash --save"));
        assert!(matches_shell_rule(&rule, "npm add react --save"));
    }

    #[test]
    fn wildcard_does_not_match_without_suffix() {
        let rule = parse_shell_rule("npm * --save");
        assert!(!matches_shell_rule(&rule, "npm install lodash"));
    }

    #[test]
    fn trailing_wildcard_matches_bare() {
        // "git *" should match both "git add" and bare "git"
        let rule = parse_shell_rule("git *");
        assert!(matches_shell_rule(&rule, "git add"));
        assert!(matches_shell_rule(&rule, "git"));
        assert!(matches_shell_rule(&rule, "git push origin main"));
    }

    #[test]
    fn wildcard_escaped_star_matches_literal() {
        // Pattern: echo \* and * — has both escaped and unescaped stars
        let rule = parse_shell_rule(r"echo \* and *");
        assert!(matches_shell_rule(&rule, "echo * and anything"));
        assert!(!matches_shell_rule(&rule, "echo x and anything"));
    }

    #[test]
    fn multiple_wildcards() {
        let rule = parse_shell_rule("* run *");
        assert!(matches_shell_rule(&rule, "npm run build"));
        assert!(matches_shell_rule(&rule, "yarn run test"));
        assert!(!matches_shell_rule(&rule, "npm build"));
    }
}
