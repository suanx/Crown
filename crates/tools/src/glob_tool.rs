//! Fast file-finder tool with mtime-sorted results.
//!
//! Mirrors Claude Code's GlobTool. Uses `ignore::WalkBuilder` (respects
//! .gitignore) + `globset` matching, returns paths sorted by modification
//! time (most recent first), with head_limit pagination.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use globset::Glob;
use ignore::WalkBuilder;
use serde::Deserialize;
use serde_json::Value;

use crate::types::ToolError;
use crate::{Tool, ToolContext};

const DEFAULT_GLOB_LIMIT: usize = 250;

#[derive(Debug, Deserialize)]
struct GlobArgs {
    /// Glob pattern (e.g. "**/*.rs").
    pattern: String,
    /// Search root (default cwd or ".").
    #[serde(default)]
    path: Option<String>,
    /// Max results (default 250).
    #[serde(default)]
    head_limit: Option<usize>,
}

/// Find files by glob pattern, newest first.
pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }
    fn is_read_only(&self) -> bool {
        true
    }
    fn timeout(&self) -> Duration {
        Duration::from_secs(15)
    }
    fn interrupt_behavior(&self) -> crate::InterruptBehavior {
        crate::InterruptBehavior::Cancel
    }
    fn get_path(&self, input: &Value) -> Option<String> {
        input.get("path").and_then(|v| v.as_str()).map(String::from)
    }
    async fn check_permissions(
        &self,
        _input: &Value,
        _mode: crate::permission::PermissionMode,
    ) -> crate::permission::PermissionResult {
        crate::permission::PermissionResult::Passthrough {
            message: "read access".into(),
        }
    }
    async fn validate_input(&self, input: &Value) -> Result<(), String> {
        let p = input.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
        if p.is_empty() {
            return Err("'pattern' is required".into());
        }
        Glob::new(p).map_err(|e| format!("invalid glob: {e}"))?;
        Ok(())
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, ToolError> {
        let args: GlobArgs = serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs {
            tool: self.name().into(),
            message: e.to_string(),
        })?;
        let root = args
            .path
            .clone()
            .or_else(|| ctx.cwd.as_ref().map(|p| p.to_string_lossy().into_owned()))
            .unwrap_or_else(|| ".".into());
        let root = PathBuf::from(root);
        let head_limit = args.head_limit.unwrap_or(DEFAULT_GLOB_LIMIT);
        let pattern = args.pattern.clone();

        let result = tokio::task::spawn_blocking(move || -> Result<String, ToolError> {
            let glob = Glob::new(&pattern)
                .map_err(|e| ToolError::InvalidArgs {
                    tool: "glob".into(),
                    message: format!("invalid glob: {e}"),
                })?
                .compile_matcher();

            let mut hits: Vec<(String, SystemTime)> = Vec::new();
            for entry in WalkBuilder::new(&root)
                .hidden(false)
                .build()
                .filter_map(|e| e.ok())
            {
                if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    continue;
                }
                let path = entry.path();
                let rel = path.strip_prefix(&root).unwrap_or(path);
                let cand = rel.to_string_lossy().replace('\\', "/");
                if !glob.is_match(cand.as_str()) {
                    continue;
                }
                let mtime = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                hits.push((cand, mtime));
            }

            if hits.is_empty() {
                return Ok(format!("No files match pattern: {pattern}"));
            }
            // Sort newest first.
            hits.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            let truncated = hits.len() > head_limit;
            let files: Vec<String> = hits.into_iter().take(head_limit).map(|(p, _)| p).collect();
            let mut out = format!("Found {} file(s):\n{}", files.len(), files.join("\n"));
            if truncated {
                out.push_str(&format!("\n[truncated at {head_limit}]"));
            }
            Ok(out)
        })
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("glob task failed: {e}")))?;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[tokio::test]
    async fn glob_finds_rs_files() {
        let dir = tempdir().unwrap();
        tokio::fs::write(dir.path().join("a.rs"), "x")
            .await
            .unwrap();
        tokio::fs::create_dir(dir.path().join("sub")).await.unwrap();
        tokio::fs::write(dir.path().join("sub").join("b.rs"), "y")
            .await
            .unwrap();
        tokio::fs::write(dir.path().join("c.py"), "z")
            .await
            .unwrap();
        let out = GlobTool
            .execute(
                json!({"pattern": "**/*.rs", "path": dir.path().to_string_lossy()}),
                &ToolContext::standalone(),
            )
            .await
            .unwrap();
        assert!(out.contains("a.rs"), "got: {out}");
        assert!(out.contains("b.rs"), "got: {out}");
        assert!(!out.contains("c.py"), "got: {out}");
    }

    #[tokio::test]
    async fn glob_no_match() {
        let dir = tempdir().unwrap();
        tokio::fs::write(dir.path().join("a.rs"), "x")
            .await
            .unwrap();
        let out = GlobTool
            .execute(
                json!({"pattern": "*.zzz", "path": dir.path().to_string_lossy()}),
                &ToolContext::standalone(),
            )
            .await
            .unwrap();
        assert!(out.contains("No files match"), "got: {out}");
    }

    #[tokio::test]
    async fn glob_invalid_rejected() {
        let r = GlobTool.validate_input(&json!({"pattern": ""})).await;
        assert!(r.is_err());
    }
}
