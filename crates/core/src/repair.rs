//! Best-effort repair of malformed tool-call argument JSON.
//!
//! Models occasionally emit slightly-malformed JSON for tool arguments
//! (trailing commas, unclosed braces/brackets, smart quotes). Rather than
//! silently degrading to `{}` — which strips the model's intent and causes a
//! confusing downstream error — we try a few deterministic repairs first.

use serde_json::Value;

/// Attempt to parse tool-call argument JSON, applying light repairs if the
/// raw text doesn't parse as-is.
///
/// Returns `Ok(value)` on success (possibly after repair), or `Err(reason)`
/// if even the repaired text won't parse. The caller decides whether to feed
/// the error back to the model (preferred) or fall back to `{}`.
pub fn parse_tool_args(raw: &str) -> Result<Value, String> {
    let trimmed = raw.trim();
    // Empty args → empty object (common for no-arg tools).
    if trimmed.is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    // Fast path: already valid.
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return Ok(v);
    }
    // Repair attempts, applied in sequence.
    let repaired = repair_json(trimmed);
    serde_json::from_str::<Value>(&repaired)
        .map_err(|e| format!("tool arguments are not valid JSON even after repair: {e}"))
}

/// Apply deterministic textual repairs: strip trailing commas and balance
/// unclosed braces/brackets.
fn repair_json(s: &str) -> String {
    let mut out = strip_trailing_commas(s);
    balance_brackets(&mut out);
    out
}

/// Remove commas that immediately precede a closing `}` or `]` (allowing
/// whitespace between). e.g. `{"a":1,}` → `{"a":1}`.
fn strip_trailing_commas(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_string = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c == ',' {
            // Look ahead past whitespace for a closing bracket.
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j] as char).is_whitespace() {
                j += 1;
            }
            if j < bytes.len() && (bytes[j] == b'}' || bytes[j] == b']') {
                // Skip this comma.
                i += 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Append missing closing brackets to balance the JSON. Counts unmatched
/// `{`/`[` outside of strings and appends the matching closers in reverse
/// order. Does nothing if already balanced or over-closed.
fn balance_brackets(s: &mut String) {
    let mut stack: Vec<char> = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    for c in s.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => stack.push('}'),
            '[' => stack.push(']'),
            '}' | ']' => {
                stack.pop();
            }
            _ => {}
        }
    }
    while let Some(closer) = stack.pop() {
        s.push(closer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_json_passes_through() {
        let v = parse_tool_args(r#"{"path": "/tmp"}"#).unwrap();
        assert_eq!(v["path"], "/tmp");
    }

    #[test]
    fn empty_becomes_empty_object() {
        let v = parse_tool_args("").unwrap();
        assert!(v.is_object());
        assert_eq!(v.as_object().unwrap().len(), 0);
    }

    #[test]
    fn trailing_comma_repaired() {
        let v = parse_tool_args(r#"{"a": 1, "b": 2,}"#).unwrap();
        assert_eq!(v["a"], 1);
        assert_eq!(v["b"], 2);
    }

    #[test]
    fn unclosed_brace_repaired() {
        let v = parse_tool_args(r#"{"path": "/tmp""#).unwrap();
        assert_eq!(v["path"], "/tmp");
    }

    #[test]
    fn unclosed_nested_repaired() {
        let v = parse_tool_args(r#"{"a": {"b": 1"#).unwrap();
        assert_eq!(v["a"]["b"], 1);
    }

    #[test]
    fn comma_inside_string_preserved() {
        let v = parse_tool_args(r#"{"msg": "a, b, c"}"#).unwrap();
        assert_eq!(v["msg"], "a, b, c");
    }

    #[test]
    fn truly_broken_returns_err() {
        let r = parse_tool_args("this is not json at all %%%");
        assert!(r.is_err());
    }
}
