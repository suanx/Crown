//! The `skill` tool — invokes a discovered skill by name, loading its full
//! SKILL.md body into the conversation (progressive disclosure level 2).
//!
//! Skills are discovered from the global + project directories (resolved
//! against `ctx.cwd`). The model sees only `name + description` in a
//! system-reminder; calling this tool pulls in the full instructions.

use async_trait::async_trait;
use serde_json::Value;

use crate::context::ToolContext;
use crate::types::ToolError;
use crate::Tool;

/// Marker prefix the engine uses to recognize skill-body tool results.
pub const SKILL_RESULT_MARKER: &str = "[skill:";

pub struct SkillTool;

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        "skill"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_parallel_safe(&self) -> bool {
        // Only one skill expands at a time — its body must be processed before
        // continuing (mirrors Claude Code's SkillTool).
        false
    }

    async fn validate_input(&self, input: &Value) -> Result<(), String> {
        let name = input.get("skill").and_then(|v| v.as_str()).unwrap_or("");
        if name.trim().is_empty() {
            return Err("`skill` argument is required and must be a non-empty string".into());
        }
        Ok(())
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, ToolError> {
        let name = args
            .get("skill")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().trim_start_matches('/'))
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::InvalidArgs {
                tool: "skill".into(),
                message: "`skill` argument is required".into(),
            })?;
        let skill_args = args.get("args").and_then(|v| v.as_str());

        let metas = deepseek_skill::discovery::discover_all(ctx.cwd.as_deref());
        let meta = metas.iter().find(|m| m.name == name).ok_or_else(|| {
            let available: Vec<&str> = metas.iter().map(|m| m.name.as_str()).collect();
            ToolError::ExecutionFailed(format!(
                "unknown skill '{name}'. Available skills: {}",
                if available.is_empty() {
                    "(none)".to_string()
                } else {
                    available.join(", ")
                }
            ))
        })?;

        let body = deepseek_skill::loader::load_skill_body(meta, skill_args).map_err(|e| {
            ToolError::ExecutionFailed(format!("failed to load skill '{name}': {e}"))
        })?;

        Ok(format!("{SKILL_RESULT_MARKER}{name}]\n{body}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ctx_with_cwd(cwd: PathBuf) -> ToolContext {
        ToolContext {
            cwd: Some(cwd),
            ..ToolContext::standalone()
        }
    }

    #[tokio::test]
    async fn loads_named_skill_from_project_dir() {
        let tmp = tempfile::tempdir().unwrap();
        // project skill at <cwd>/.crown/skills/greet/SKILL.md
        let sk = tmp.path().join(".crown").join("skills").join("greet");
        std::fs::create_dir_all(&sk).unwrap();
        std::fs::write(
            sk.join("SKILL.md"),
            "---\nname: greet\ndescription: greet the user\n---\nSay hello to $ARGUMENTS politely.",
        )
        .unwrap();

        let ctx = ctx_with_cwd(tmp.path().to_path_buf());
        let out = SkillTool
            .execute(
                serde_json::json!({ "skill": "greet", "args": "Alice" }),
                &ctx,
            )
            .await
            .expect("skill loads");
        assert!(out.starts_with("[skill:greet]"), "got: {out}");
        assert!(out.contains("Say hello to Alice politely."), "got: {out}");
    }

    #[tokio::test]
    async fn unknown_skill_errors_with_available_list() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_with_cwd(tmp.path().to_path_buf());
        let err = SkillTool
            .execute(serde_json::json!({ "skill": "nope" }), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::ExecutionFailed(_)));
    }

    #[tokio::test]
    async fn empty_skill_name_is_invalid() {
        assert!(SkillTool
            .validate_input(&serde_json::json!({ "skill": "" }))
            .await
            .is_err());
    }
}
