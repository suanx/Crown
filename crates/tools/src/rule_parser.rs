//! Permission rule value parser.
//!
//! Mirrors Claude Code's `src/utils/permissions/permissionRuleParser.ts`.
//!
//! Permission rules are stored as strings in settings.json with format:
//!   `"ToolName"` — tool-wide rule (matches any input)
//!   `"ToolName(content)"` — content-specific rule
//!
//! Content may contain escaped parentheses: `\(` and `\)`.
//! The parser handles escape sequences correctly.

use crate::permission::PermissionRuleValue;

/// Escape special characters in rule content for safe storage.
///
/// Permission rules use format "Tool(content)", so parentheses must be escaped.
/// Escaping order: backslashes first, then parentheses.
///
/// # Examples
/// ```
/// # use deepseek_tools::rule_parser::escape_rule_content;
/// assert_eq!(escape_rule_content("print(1)"), r"print\(1\)");
/// assert_eq!(escape_rule_content(r"test\nvalue"), r"test\\nvalue");
/// ```
pub fn escape_rule_content(content: &str) -> String {
    content
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

/// Unescape special characters after parsing from permission rules.
/// Reverses `escape_rule_content`.
pub fn unescape_rule_content(content: &str) -> String {
    content
        .replace("\\(", "(")
        .replace("\\)", ")")
        .replace("\\\\", "\\")
}

/// Parse a permission rule string into a `PermissionRuleValue`.
///
/// Format: `"ToolName"` or `"ToolName(content)"`.
///
/// # Examples
/// ```
/// # use deepseek_tools::rule_parser::parse_rule_value;
/// let r = parse_rule_value("Bash");
/// assert_eq!(r.tool_name, "Bash");
/// assert_eq!(r.rule_content, None);
///
/// let r = parse_rule_value("Bash(npm install)");
/// assert_eq!(r.tool_name, "Bash");
/// assert_eq!(r.rule_content, Some("npm install".to_string()));
/// ```
pub fn parse_rule_value(rule_string: &str) -> PermissionRuleValue {
    // Find the first unescaped opening parenthesis
    let open_paren = find_first_unescaped(rule_string, '(');

    let Some(open_idx) = open_paren else {
        // No parenthesis — just a tool name
        return PermissionRuleValue {
            tool_name: rule_string.to_string(),
            rule_content: None,
        };
    };

    // Find the last unescaped closing parenthesis
    let close_paren = find_last_unescaped(rule_string, ')');

    let Some(close_idx) = close_paren else {
        // No matching closing paren — treat whole string as tool name
        return PermissionRuleValue {
            tool_name: rule_string.to_string(),
            rule_content: None,
        };
    };

    // Closing must be after opening and at the end of the string
    if close_idx <= open_idx || close_idx != rule_string.len() - 1 {
        return PermissionRuleValue {
            tool_name: rule_string.to_string(),
            rule_content: None,
        };
    }

    let tool_name = &rule_string[..open_idx];
    let raw_content = &rule_string[open_idx + 1..close_idx];

    // Missing tool name (e.g., "(foo)") — malformed
    if tool_name.is_empty() {
        return PermissionRuleValue {
            tool_name: rule_string.to_string(),
            rule_content: None,
        };
    }

    // Empty content "Bash()" or standalone wildcard "Bash(*)" → tool-wide rule
    if raw_content.is_empty() || raw_content == "*" {
        return PermissionRuleValue {
            tool_name: tool_name.to_string(),
            rule_content: None,
        };
    }

    // Unescape the content
    let unescaped = unescape_rule_content(raw_content);

    PermissionRuleValue {
        tool_name: tool_name.to_string(),
        rule_content: Some(unescaped),
    }
}

/// Format a `PermissionRuleValue` back to its string representation.
///
/// # Examples
/// ```
/// # use deepseek_tools::rule_parser::format_rule_value;
/// # use deepseek_tools::permission::PermissionRuleValue;
/// let rv = PermissionRuleValue { tool_name: "Bash".into(), rule_content: None };
/// assert_eq!(format_rule_value(&rv), "Bash");
///
/// let rv = PermissionRuleValue { tool_name: "Bash".into(), rule_content: Some("npm install".into()) };
/// assert_eq!(format_rule_value(&rv), "Bash(npm install)");
/// ```
pub fn format_rule_value(rv: &PermissionRuleValue) -> String {
    match &rv.rule_content {
        None => rv.tool_name.clone(),
        Some(content) => {
            let escaped = escape_rule_content(content);
            format!("{}({})", rv.tool_name, escaped)
        }
    }
}

/// Find the index of the first unescaped occurrence of `ch`.
/// A character is "escaped" if preceded by an odd number of backslashes.
fn find_first_unescaped(s: &str, ch: char) -> Option<usize> {
    let bytes = s.as_bytes();
    let target = ch as u8;
    for i in 0..bytes.len() {
        if bytes[i] == target {
            let mut backslash_count = 0;
            let mut j = i as isize - 1;
            while j >= 0 && bytes[j as usize] == b'\\' {
                backslash_count += 1;
                j -= 1;
            }
            if backslash_count % 2 == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// Find the index of the last unescaped occurrence of `ch`.
fn find_last_unescaped(s: &str, ch: char) -> Option<usize> {
    let bytes = s.as_bytes();
    let target = ch as u8;
    for i in (0..bytes.len()).rev() {
        if bytes[i] == target {
            let mut backslash_count = 0;
            let mut j = i as isize - 1;
            while j >= 0 && bytes[j as usize] == b'\\' {
                backslash_count += 1;
                j -= 1;
            }
            if backslash_count % 2 == 0 {
                return Some(i);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tool_name_only() {
        let r = parse_rule_value("Bash");
        assert_eq!(r.tool_name, "Bash");
        assert_eq!(r.rule_content, None);
    }

    #[test]
    fn parse_tool_with_simple_content() {
        let r = parse_rule_value("Bash(npm install)");
        assert_eq!(r.tool_name, "Bash");
        assert_eq!(r.rule_content, Some("npm install".to_string()));
    }

    #[test]
    fn parse_tool_with_prefix_content() {
        let r = parse_rule_value("Bash(git:*)");
        assert_eq!(r.tool_name, "Bash");
        assert_eq!(r.rule_content, Some("git:*".to_string()));
    }

    #[test]
    fn parse_empty_parens_is_tool_wide() {
        let r = parse_rule_value("Bash()");
        assert_eq!(r.tool_name, "Bash");
        assert_eq!(r.rule_content, None);
    }

    #[test]
    fn parse_wildcard_star_is_tool_wide() {
        let r = parse_rule_value("Bash(*)");
        assert_eq!(r.tool_name, "Bash");
        assert_eq!(r.rule_content, None);
    }

    #[test]
    fn parse_escaped_parens_in_content() {
        // Input: Bash(python -c \"print\(1\)\")
        // The last ) is the unescaped closing paren of the rule.
        // Content between outer parens: python -c \"print\(1\)\"
        // After unescape: \( → (, \) → ), \\ → \ (no \\ present here)
        // \" is not a recognized escape, so it stays as \"
        let r = parse_rule_value(r#"Bash(python -c \"print\(1\)\")"#);
        assert_eq!(r.tool_name, "Bash");
        assert_eq!(
            r.rule_content,
            Some(r#"python -c \"print(1)\""#.to_string())
        );
    }

    #[test]
    fn parse_file_glob_content() {
        let r = parse_rule_value("Edit(/src/**/*.rs)");
        assert_eq!(r.tool_name, "Edit");
        assert_eq!(r.rule_content, Some("/src/**/*.rs".to_string()));
    }

    #[test]
    fn parse_no_closing_paren_is_tool_name() {
        let r = parse_rule_value("Bash(oops");
        assert_eq!(r.tool_name, "Bash(oops");
        assert_eq!(r.rule_content, None);
    }

    #[test]
    fn parse_leading_paren_malformed() {
        let r = parse_rule_value("(foo)");
        assert_eq!(r.tool_name, "(foo)");
        assert_eq!(r.rule_content, None);
    }

    #[test]
    fn format_tool_only() {
        let rv = PermissionRuleValue {
            tool_name: "read_file".into(),
            rule_content: None,
        };
        assert_eq!(format_rule_value(&rv), "read_file");
    }

    #[test]
    fn format_with_content() {
        let rv = PermissionRuleValue {
            tool_name: "run_command".into(),
            rule_content: Some("git:*".into()),
        };
        assert_eq!(format_rule_value(&rv), "run_command(git:*)");
    }

    #[test]
    fn format_escapes_parens_in_content() {
        let rv = PermissionRuleValue {
            tool_name: "Bash".into(),
            rule_content: Some("print(1)".into()),
        };
        assert_eq!(format_rule_value(&rv), r"Bash(print\(1\))");
    }

    #[test]
    fn roundtrip_simple() {
        let original = "Bash(npm install)";
        let parsed = parse_rule_value(original);
        let formatted = format_rule_value(&parsed);
        assert_eq!(formatted, original);
    }

    #[test]
    fn roundtrip_with_escaped_content() {
        let rv = PermissionRuleValue {
            tool_name: "Bash".into(),
            rule_content: Some("python -c \"print(1)\"".into()),
        };
        let formatted = format_rule_value(&rv);
        let reparsed = parse_rule_value(&formatted);
        assert_eq!(reparsed, rv);
    }

    #[test]
    fn escape_unescape_roundtrip() {
        let original = r#"test\with(parens)and\backslash"#;
        let escaped = escape_rule_content(original);
        let unescaped = unescape_rule_content(&escaped);
        assert_eq!(unescaped, original);
    }
}
