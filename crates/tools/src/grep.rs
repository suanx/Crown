//! Regex content search tool (ripgrep-grade).
//!
//! Mirrors Claude Code's GrepTool. Built on the `grep-regex` + `grep-searcher`
//! crates (the libraries ripgrep itself uses) so there's no external binary
//! dependency. Traversal uses `ignore::WalkBuilder` to honour .gitignore and
//! skip VCS directories.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use globset::Glob;
use grep_regex::RegexMatcher;
use grep_searcher::sinks::UTF8;
use grep_searcher::SearcherBuilder;
use ignore::WalkBuilder;
use serde::Deserialize;
use serde_json::Value;

use crate::types::ToolError;
use crate::{Tool, ToolContext};

/// Default cap on grep results (lines or files) when head_limit unspecified.
const DEFAULT_HEAD_LIMIT: usize = 250;
/// Files larger than this are skipped.
const MAX_GREP_FILE_SIZE: u64 = 5 * 1024 * 1024;
/// VCS dirs always excluded from search.
const VCS_DIRS: &[&str] = &[".git", ".svn", ".hg", ".bzr", ".jj"];

#[derive(Debug, Deserialize)]
struct GrepArgs {
    /// Regex pattern.
    pattern: String,
    /// Search root (default cwd or ".").
    #[serde(default)]
    path: Option<String>,
    /// Glob filter on file paths (e.g. "*.rs", "**/*.ts").
    #[serde(default)]
    glob: Option<String>,
    /// Output mode: "content" | "files_with_matches" | "count".
    #[serde(default)]
    output_mode: Option<String>,
    /// Lines of context around each match (content mode only).
    #[serde(default)]
    context: Option<usize>,
    /// Case-insensitive search.
    #[serde(default)]
    case_insensitive: bool,
    /// Max results returned (default 250).
    #[serde(default)]
    head_limit: Option<usize>,
}

/// Regex content search across a directory tree.
pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
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
        // Validate regex compiles.
        RegexMatcher::new(p).map_err(|e| format!("invalid regex: {e}"))?;
        Ok(())
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, ToolError> {
        let args: GrepArgs = serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs {
            tool: self.name().into(),
            message: e.to_string(),
        })?;

        let root = args
            .path
            .clone()
            .or_else(|| ctx.cwd.as_ref().map(|p| p.to_string_lossy().into_owned()))
            .unwrap_or_else(|| ".".into());
        let root = PathBuf::from(root);
        let mode = args
            .output_mode
            .clone()
            .unwrap_or_else(|| "files_with_matches".into());
        let head_limit = args.head_limit.unwrap_or(DEFAULT_HEAD_LIMIT);
        let pattern = args.pattern.clone();
        let case_insensitive = args.case_insensitive;
        let context = args.context.unwrap_or(0);
        let glob_pat = args.glob.clone();

        let result = tokio::task::spawn_blocking(move || -> Result<String, ToolError> {
            // Build matcher.
            let matcher = grep_regex::RegexMatcherBuilder::new()
                .case_insensitive(case_insensitive)
                .build(&pattern)
                .map_err(|e| ToolError::InvalidArgs {
                    tool: "grep".into(),
                    message: format!("invalid regex: {e}"),
                })?;

            let glob_matcher = match &glob_pat {
                Some(g) => Some(
                    Glob::new(g)
                        .map_err(|e| ToolError::InvalidArgs {
                            tool: "grep".into(),
                            message: format!("invalid glob: {e}"),
                        })?
                        .compile_matcher(),
                ),
                None => None,
            };

            // Collect (path, line_no, line) matches.
            struct Match {
                path: String,
                line_no: u64,
                line: String,
            }
            let mut matches: Vec<Match> = Vec::new();
            let mut file_set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            // Count of files skipped because their search errored (permission,
            // non-UTF8, transient IO). Logged once after the walk instead of
            // silently swallowed, so a search that quietly misses files is
            // diagnosable.
            let mut skipped_files: usize = 0;
            let mut count_map: std::collections::BTreeMap<String, usize> =
                std::collections::BTreeMap::new();

            let mut builder = SearcherBuilder::new();
            builder.line_number(true);
            if context > 0 {
                builder.before_context(context);
                builder.after_context(context);
            }

            'walk: for entry in WalkBuilder::new(&root)
                .hidden(false)
                .build()
                .filter_map(|e| e.ok())
            {
                let is_file = entry.file_type().map(|t| t.is_file()).unwrap_or(false);
                if !is_file {
                    continue;
                }
                let path = entry.path();

                // Skip VCS dirs (defense; WalkBuilder respects gitignore but .git is tracked-ignored differently).
                if path.components().any(|c| {
                    c.as_os_str()
                        .to_str()
                        .map(|s| VCS_DIRS.contains(&s))
                        .unwrap_or(false)
                }) {
                    continue;
                }

                // Size cap.
                if let Ok(md) = entry.metadata() {
                    if md.len() > MAX_GREP_FILE_SIZE {
                        continue;
                    }
                }

                // Glob filter (on path relative to root, forward slashes).
                if let Some(gm) = &glob_matcher {
                    let rel = path.strip_prefix(&root).unwrap_or(path);
                    let cand = rel.to_string_lossy().replace('\\', "/");
                    if !gm.is_match(cand.as_str()) {
                        continue;
                    }
                }

                let rel_display = path
                    .strip_prefix(&root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/");

                let mut searcher = builder.build();
                let rel_for_sink = rel_display.clone();
                let search_result = searcher.search_path(
                    &matcher,
                    path,
                    UTF8(|line_no, line| {
                        // record
                        match mode.as_str() {
                            "files_with_matches" => {
                                file_set.insert(rel_for_sink.clone());
                            }
                            "count" => {
                                *count_map.entry(rel_for_sink.clone()).or_insert(0) += 1;
                            }
                            _ => {
                                matches.push(Match {
                                    path: rel_for_sink.clone(),
                                    line_no,
                                    line: line.trim_end().to_string(),
                                });
                            }
                        }
                        Ok(true)
                    }),
                );
                if search_result.is_err() {
                    // One unreadable file must not abort the whole search; tally
                    // and continue (summarized after the walk).
                    skipped_files += 1;
                }

                // Early exit if we've hit head_limit for file-modes.
                if mode == "files_with_matches" && file_set.len() >= head_limit {
                    break 'walk;
                }
                if mode == "content" && matches.len() >= head_limit {
                    break 'walk;
                }
            }

            if skipped_files > 0 {
                tracing::debug!(
                    skipped_files,
                    pattern = %pattern,
                    "grep: some files could not be searched (permission/encoding/IO) and were skipped"
                );
            }

            // Format output per mode.
            let out = match mode.as_str() {
                "files_with_matches" => {
                    if file_set.is_empty() {
                        return Ok("No files found".into());
                    }
                    let mut files: Vec<String> = file_set.into_iter().take(head_limit).collect();
                    files.sort();
                    format!("Found {} file(s):\n{}", files.len(), files.join("\n"))
                }
                "count" => {
                    if count_map.is_empty() {
                        return Ok("No matches found".into());
                    }
                    let lines: Vec<String> = count_map
                        .iter()
                        .take(head_limit)
                        .map(|(f, c)| format!("{f}:{c}"))
                        .collect();
                    lines.join("\n")
                }
                _ => {
                    if matches.is_empty() {
                        return Ok("No matches found".into());
                    }
                    let lines: Vec<String> = matches
                        .iter()
                        .take(head_limit)
                        .map(|m| format!("{}:{}:{}", m.path, m.line_no, m.line))
                        .collect();
                    let truncated = matches.len() > head_limit;
                    let mut s = lines.join("\n");
                    if truncated {
                        s.push_str(&format!("\n[truncated at {head_limit} matches]"));
                    }
                    s
                }
            };
            Ok(out)
        })
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("grep task failed: {e}")))?;

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    async fn run(args: Value) -> Result<String, ToolError> {
        GrepTool.execute(args, &ToolContext::standalone()).await
    }

    #[tokio::test]
    async fn content_mode_finds_matches() {
        let dir = tempdir().unwrap();
        tokio::fs::write(dir.path().join("a.rs"), "fn main() {\n    let x = 42;\n}")
            .await
            .unwrap();
        let out = run(json!({"pattern": "let \\w+", "path": dir.path().to_string_lossy(), "output_mode": "content"})).await.unwrap();
        assert!(out.contains("let x = 42"), "got: {out}");
        assert!(out.contains("a.rs:2:"), "got: {out}");
    }

    #[tokio::test]
    async fn files_with_matches_mode() {
        let dir = tempdir().unwrap();
        tokio::fs::write(dir.path().join("a.rs"), "TODO: fix")
            .await
            .unwrap();
        tokio::fs::write(dir.path().join("b.rs"), "nothing")
            .await
            .unwrap();
        let out = run(json!({"pattern": "TODO", "path": dir.path().to_string_lossy()}))
            .await
            .unwrap();
        assert!(out.contains("a.rs"), "got: {out}");
        assert!(!out.contains("b.rs"), "got: {out}");
    }

    #[tokio::test]
    async fn count_mode() {
        let dir = tempdir().unwrap();
        tokio::fs::write(dir.path().join("a.rs"), "x\nx\nx")
            .await
            .unwrap();
        let out = run(
            json!({"pattern": "x", "path": dir.path().to_string_lossy(), "output_mode": "count"}),
        )
        .await
        .unwrap();
        assert!(out.contains("a.rs:3"), "got: {out}");
    }

    #[tokio::test]
    async fn glob_filter() {
        let dir = tempdir().unwrap();
        tokio::fs::write(dir.path().join("a.rs"), "match")
            .await
            .unwrap();
        tokio::fs::write(dir.path().join("b.py"), "match")
            .await
            .unwrap();
        let out =
            run(json!({"pattern": "match", "path": dir.path().to_string_lossy(), "glob": "*.rs"}))
                .await
                .unwrap();
        assert!(out.contains("a.rs"), "got: {out}");
        assert!(!out.contains("b.py"), "got: {out}");
    }

    #[tokio::test]
    async fn case_insensitive() {
        let dir = tempdir().unwrap();
        tokio::fs::write(dir.path().join("a.rs"), "HELLO world")
            .await
            .unwrap();
        let out = run(json!({"pattern": "hello", "path": dir.path().to_string_lossy(), "output_mode": "content", "case_insensitive": true})).await.unwrap();
        assert!(out.contains("HELLO"), "got: {out}");
    }

    #[tokio::test]
    async fn no_matches() {
        let dir = tempdir().unwrap();
        tokio::fs::write(dir.path().join("a.rs"), "nothing here")
            .await
            .unwrap();
        let out = run(json!({"pattern": "zzz", "path": dir.path().to_string_lossy()}))
            .await
            .unwrap();
        assert!(
            out.contains("No files found") || out.contains("No matches"),
            "got: {out}"
        );
    }

    #[tokio::test]
    async fn invalid_regex_rejected() {
        let r = GrepTool.validate_input(&json!({"pattern": "["})).await;
        assert!(r.is_err());
    }
}
