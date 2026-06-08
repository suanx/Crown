//! MCP tool name namespacing + sanitization.
//!
//! MCP tools are exposed to the model as `mcp__<server>__<tool>` so they
//! never collide with built-in tool names, mirroring Claude Code's
//! `normalization.ts`. Server/tool names containing characters outside
//! `[A-Za-z0-9_-]` are sanitized to `_`.

/// Replace any character outside `[A-Za-z0-9_-]` with `_`.
pub fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Build the namespaced tool name `mcp__<server>__<tool>`.
pub fn mcp_tool_name(server: &str, tool: &str) -> String {
    format!("mcp__{}__{}", sanitize(server), sanitize(tool))
}

/// Parse a namespaced MCP tool name back into `(server, tool)`. Returns
/// `None` for names that are not MCP-namespaced (e.g. built-in tools).
pub fn parse_mcp_tool_name(name: &str) -> Option<(String, String)> {
    let rest = name.strip_prefix("mcp__")?;
    let (server, tool) = rest.split_once("__")?;
    if server.is_empty() || tool.is_empty() {
        return None;
    }
    Some((server.to_string(), tool.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaced_tool_name() {
        assert_eq!(
            mcp_tool_name("github", "create_pr"),
            "mcp__github__create_pr"
        );
    }

    #[test]
    fn sanitizes_illegal_chars() {
        assert_eq!(sanitize("my.server"), "my_server");
        assert_eq!(sanitize("a b-c"), "a_b-c");
        assert_eq!(sanitize("ok_name-1"), "ok_name-1");
    }

    #[test]
    fn parse_namespaced_roundtrip() {
        let n = mcp_tool_name("gh", "x");
        assert_eq!(parse_mcp_tool_name(&n), Some(("gh".into(), "x".into())));
        assert_eq!(parse_mcp_tool_name("read_file"), None);
    }

    #[test]
    fn parse_rejects_malformed() {
        assert_eq!(parse_mcp_tool_name("mcp__only"), None);
        assert_eq!(parse_mcp_tool_name("mcp____"), None);
    }
}
