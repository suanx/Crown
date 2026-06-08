//! Project/global memory (AGENTS.md), rules, and output-style loading +
//! system-prompt composition. All cross-agent open-spec aligned: AGENTS.md
//! (agents.md) is preferred, CLAUDE.md is read for migration compatibility.

use std::path::{Path, PathBuf};

use parking_lot::RwLock;

/// Override preamble emitted once before any memory / rules block. Aligned to
/// Claude Code's `MEMORY_INSTRUCTION_PROMPT`: it promotes user/project
/// instructions to the highest priority so the model follows them exactly
/// rather than treating them as background context.
const MEMORY_OVERRIDE_PREAMBLE: &str =
    "Codebase and user instructions are shown below. Be sure to adhere to these \
     instructions. IMPORTANT: These instructions OVERRIDE any default behavior and \
     you MUST follow them exactly as written.";

/// Project memory read-chain (first existing, non-empty wins):
/// `<cwd>/AGENTS.md` → `<cwd>/CLAUDE.md` → `<cwd>/.claude/CLAUDE.md`.
pub fn read_project_memory(cwd: &Path) -> Option<String> {
    const CANDIDATES: &[&str] = &["AGENTS.md", "CLAUDE.md", ".claude/CLAUDE.md"];
    for rel in CANDIDATES {
        let path = cwd.join(rel);
        if let Ok(content) = std::fs::read_to_string(&path) {
            if !content.trim().is_empty() {
                return Some(content);
            }
        }
    }
    None
}

/// Global memory: `<data_root>/AGENTS.md` (first existing, non-empty).
pub fn read_global_memory(data_root: &Path) -> Option<String> {
    let path = data_root.join("AGENTS.md");
    std::fs::read_to_string(&path)
        .ok()
        .filter(|c| !c.trim().is_empty())
}

/// Read all `<dir>/*.md` rule files, sorted by filename. Returns
/// `(name, body)` pairs; skips empty / unreadable files.
pub fn read_rules(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "md").unwrap_or(false))
        .collect();
    files.sort();
    for path in files {
        if let Ok(body) = std::fs::read_to_string(&path) {
            if !body.trim().is_empty() {
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                out.push((name, body));
            }
        }
    }
    out
}

/// Read the body of an output-style by name: `<dir>/<name>.md`.
pub fn read_output_style(dir: &Path, name: &str) -> Option<String> {
    if name.trim().is_empty() {
        return None;
    }
    let path = dir.join(format!("{name}.md"));
    std::fs::read_to_string(&path)
        .ok()
        .filter(|c| !c.trim().is_empty())
}

/// Compose the full system prompt. `base` is the static 7-section prompt
/// (incl. optional install block); `env` is the environment block (last so
/// the static prefix stays cache-stable up to the dynamic parts).
pub fn compose_prompt(
    base: &str,
    global_memory: Option<&str>,
    project_memory: Option<&str>,
    rules: &[(String, String)],
    output_style: Option<&str>,
    env: &str,
) -> String {
    let mut s = String::with_capacity(base.len() + env.len() + 2048);
    s.push_str(base);
    if !base.ends_with('\n') {
        s.push('\n');
    }
    // Override preamble (Claude-aligned `MEMORY_INSTRUCTION_PROMPT`): emitted
    // once before any memory/rules block so the model treats user instructions
    // as the highest-priority directives, overriding default behavior. Only
    // emitted when at least one memory/rules block follows.
    let has_instructions = global_memory.is_some() || project_memory.is_some() || !rules.is_empty();
    if has_instructions {
        s.push('\n');
        s.push_str(MEMORY_OVERRIDE_PREAMBLE);
        s.push('\n');
    }
    if let Some(g) = global_memory {
        s.push_str("\n# User memory (global AGENTS.md)\n");
        s.push_str(g.trim_end());
        s.push('\n');
    }
    if let Some(p) = project_memory {
        s.push_str("\n# Project memory (AGENTS.md)\n");
        s.push_str(p.trim_end());
        s.push('\n');
    }
    for (name, body) in rules {
        s.push_str(&format!("\n# Rule: {name}\n"));
        s.push_str(body.trim_end());
        s.push('\n');
    }
    if let Some(style) = output_style {
        s.push_str("\n# Output style\n");
        s.push_str(style.trim_end());
        s.push('\n');
    }
    s.push('\n');
    s.push_str(env);
    s
}

/// Runtime prompt augmentation source. Holds the data root (for global
/// memory / rules / output-styles) and the currently-active output-style
/// name (changeable from settings at runtime). Composes the full per-thread
/// prompt from a base template + environment block.
///
/// `from_static()` (no data root) makes [`compose`](Self::compose) a no-op
/// passthrough — used by tests / non-Tauri contexts so behavior is unchanged.
pub struct PromptAugment {
    data_root: Option<PathBuf>,
    output_style: RwLock<Option<String>>,
}

impl PromptAugment {
    /// Augment backed by a real data root (global files live here).
    pub fn new(data_root: PathBuf) -> Self {
        Self {
            data_root: Some(data_root),
            output_style: RwLock::new(None),
        }
    }

    /// No-op augment: `compose` returns `base + "\n" + env` unchanged.
    pub fn from_static() -> Self {
        Self {
            data_root: None,
            output_style: RwLock::new(None),
        }
    }

    /// Set (or clear) the active output-style name. Takes effect on threads
    /// composed after this call.
    pub fn set_output_style(&self, name: Option<String>) {
        *self.output_style.write() = name;
    }

    /// The active output-style name, if any.
    pub fn output_style(&self) -> Option<String> {
        self.output_style.read().clone()
    }

    /// Compose the full prompt for a thread with optional `cwd`.
    pub fn compose(&self, base: &str, env: &str, cwd: Option<&Path>) -> String {
        let Some(root) = &self.data_root else {
            // Static passthrough: base + blank line + env.
            let mut s = String::with_capacity(base.len() + env.len() + 2);
            s.push_str(base);
            if !base.ends_with('\n') {
                s.push('\n');
            }
            s.push('\n');
            s.push_str(env);
            return s;
        };
        let global = read_global_memory(root);
        let project = cwd.and_then(read_project_memory);
        let mut rules = read_rules(&root.join("rules"));
        if let Some(c) = cwd {
            rules.extend(read_rules(&c.join(".crown").join("rules")));
        }
        let style_name = self.output_style.read().clone();
        let style = style_name
            .as_deref()
            .and_then(|n| read_output_style(&root.join("output-styles"), n));
        compose_prompt(
            base,
            global.as_deref(),
            project.as_deref(),
            &rules,
            style.as_deref(),
            env,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn project_memory_prefers_agents_md() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("AGENTS.md"), "agents wins").unwrap();
        fs::write(tmp.path().join("CLAUDE.md"), "claude loses").unwrap();
        let got = read_project_memory(tmp.path());
        assert_eq!(got.as_deref(), Some("agents wins"));
    }

    #[test]
    fn project_memory_falls_back_to_claude_md() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("CLAUDE.md"), "claude content").unwrap();
        assert_eq!(
            read_project_memory(tmp.path()).as_deref(),
            Some("claude content")
        );
    }

    #[test]
    fn project_memory_falls_back_to_dot_claude() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join(".claude")).unwrap();
        fs::write(tmp.path().join(".claude").join("CLAUDE.md"), "nested").unwrap();
        assert_eq!(read_project_memory(tmp.path()).as_deref(), Some("nested"));
    }

    #[test]
    fn project_memory_none_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read_project_memory(tmp.path()).is_none());
    }

    #[test]
    fn empty_memory_file_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("AGENTS.md"), "   \n  ").unwrap();
        assert!(read_project_memory(tmp.path()).is_none());
    }

    #[test]
    fn compose_includes_all_parts_in_order() {
        let out = compose_prompt(
            "BASE",
            Some("GLOBAL_MEM"),
            Some("PROJECT_MEM"),
            &[("testing".into(), "RULE_BODY".into())],
            Some("STYLE_BODY"),
            "ENV",
        );
        let i_base = out.find("BASE").unwrap();
        let i_global = out.find("GLOBAL_MEM").unwrap();
        let i_project = out.find("PROJECT_MEM").unwrap();
        let i_rule = out.find("RULE_BODY").unwrap();
        let i_style = out.find("STYLE_BODY").unwrap();
        let i_env = out.find("ENV").unwrap();
        assert!(i_base < i_global && i_global < i_project);
        assert!(i_project < i_rule && i_rule < i_style && i_style < i_env);
    }

    #[test]
    fn compose_omits_absent_parts() {
        let out = compose_prompt("BASE", None, None, &[], None, "ENV");
        assert!(out.contains("BASE") && out.contains("ENV"));
        assert!(!out.contains("# Project memory"));
        assert!(!out.contains("# Output style"));
        // No instructions → no override preamble.
        assert!(!out.contains("OVERRIDE any default behavior"));
    }

    #[test]
    fn compose_emits_override_preamble_before_memory() {
        let out = compose_prompt("BASE", None, Some("PROJECT_MEM"), &[], None, "ENV");
        let i_preamble = out.find("OVERRIDE any default behavior").unwrap();
        let i_project = out.find("PROJECT_MEM").unwrap();
        assert!(
            i_preamble < i_project,
            "override preamble must precede the memory block"
        );
    }

    #[test]
    fn compose_preamble_emitted_for_rules_only() {
        let out = compose_prompt(
            "BASE",
            None,
            None,
            &[("testing".into(), "RULE_BODY".into())],
            None,
            "ENV",
        );
        assert!(out.contains("OVERRIDE any default behavior"));
    }

    #[test]
    fn compose_no_preamble_for_output_style_only() {
        // Output style is a formatting directive, not user "instructions" —
        // it doesn't get the override preamble.
        let out = compose_prompt("BASE", None, None, &[], Some("STYLE"), "ENV");
        assert!(!out.contains("OVERRIDE any default behavior"));
        assert!(out.contains("STYLE"));
    }

    #[test]
    fn augment_static_returns_base_unchanged() {
        let a = PromptAugment::from_static();
        let got = a.compose("BASE\n", "# Environment\nx\n", None);
        assert_eq!(got, "BASE\n\n# Environment\nx\n");
    }

    #[test]
    fn augment_injects_project_memory() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("AGENTS.md"), "PROJECT RULES HERE").unwrap();
        let data = tempfile::tempdir().unwrap();
        let a = PromptAugment::new(data.path().to_path_buf());
        let got = a.compose("BASE\n", "ENV\n", Some(tmp.path()));
        assert!(got.contains("PROJECT RULES HERE"));
        assert!(got.contains("# Project memory"));
    }

    #[test]
    fn augment_active_output_style_roundtrip() {
        let data = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(data.path().join("output-styles")).unwrap();
        std::fs::write(
            data.path().join("output-styles").join("terse.md"),
            "Be extremely terse.",
        )
        .unwrap();
        let a = PromptAugment::new(data.path().to_path_buf());
        a.set_output_style(Some("terse".into()));
        let got = a.compose("BASE\n", "ENV\n", None);
        assert!(got.contains("Be extremely terse."));
        assert!(got.contains("# Output style"));
    }
}
