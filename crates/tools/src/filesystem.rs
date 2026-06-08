//! Filesystem tools exposed to the agent.
//!
//! This module provides six tools that let the model observe and mutate the
//! local filesystem:
//!
//! * [`ReadFileTool`] — read a file with optional line offset / limit.
//! * [`ListDirectoryTool`] — list a directory with optional recursion.
//! * [`WriteFileTool`] — write a file, creating parent directories.
//! * [`EditFileTool`] — perform a targeted search-and-replace.
//!
//! Read tools are read-only and parallel-safe; write tools mutate the
//! environment and are dispatched serially by the runner.
//!
//! All synchronous filesystem traversals (via [`walkdir`] and [`ignore`]) are
//! executed on Tokio's blocking thread pool so they never stall the runtime,
//! while pointwise reads and writes use [`tokio::fs`].

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use walkdir::WalkDir;

use crate::types::ToolError;
use crate::Tool;
use crate::ToolContext;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum size (in bytes) a file may have before [`ReadFileTool`] requires
/// a `limit` argument. Defaults to 1 MiB so the model is forced to be
/// explicit about partial reads on large files.
const MAX_FILE_SIZE: u64 = 1024 * 1024;

/// Maximum number of entries [`ListDirectoryTool`] will return before
/// truncating with a notice.
const MAX_LIST_ENTRIES: usize = 500;

/// Hard cap on the recursion depth accepted by [`ListDirectoryTool`].
/// Chosen to keep traversals fast on large monorepos.
const MAX_RECURSIVE_DEPTH: usize = 6;

/// Directory names that are always pruned during traversal. These are the
/// usual suspects for noisy build / dependency caches across ecosystems.
const IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    "__pycache__",
    ".venv",
    "venv",
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Deserialize JSON arguments into a tool-specific args struct, mapping
/// failures onto [`ToolError::InvalidArgs`] with a stable shape.
fn parse_args<T: serde::de::DeserializeOwned>(
    tool_name: &str,
    args: Value,
) -> Result<T, ToolError> {
    serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs {
        tool: tool_name.to_string(),
        message: e.to_string(),
    })
}

/// Convert a `JoinError` from [`tokio::task::spawn_blocking`] into a
/// [`ToolError::ExecutionFailed`] so panics inside blocking work surface to
/// the model rather than aborting the runtime.
fn join_error_to_tool_error(err: tokio::task::JoinError) -> ToolError {
    ToolError::ExecutionFailed(format!("blocking task failed: {}", err))
}

/// Whether a directory entry's file name is on the [`IGNORED_DIRS`] list.
fn is_ignored_dir_name(name: &std::ffi::OsStr) -> bool {
    let s = name.to_string_lossy();
    IGNORED_DIRS.iter().any(|d| *d == s.as_ref())
}

/// Record a file's current content into the rewind sink before mutating it
/// (P2). No-op when the context has no sink (tests / standalone).
async fn snapshot_for_rewind(ctx: &ToolContext, path: &str) {
    let (Some(sink), Some(tid), Some(seq)) = (
        ctx.file_history.as_ref(),
        ctx.thread_id.as_deref(),
        ctx.message_seq,
    ) else {
        return;
    };
    let before = tokio::fs::read_to_string(path).await.ok();
    sink.record(tid, seq, path, before);
}

// ---------------------------------------------------------------------------
// ReadFileTool
// ---------------------------------------------------------------------------

/// Arguments accepted by [`ReadFileTool`].
#[derive(Debug, Deserialize)]
struct ReadFileArgs {
    /// Path of the file to read (absolute or relative to CWD).
    path: String,
    /// 0-indexed line to start reading from. Defaults to 0.
    #[serde(default)]
    offset: Option<usize>,
    /// Maximum number of lines to return. When omitted the entire file is
    /// returned, subject to the [`MAX_FILE_SIZE`] safety cap.
    #[serde(default)]
    limit: Option<usize>,
}

/// Maximum number of lines [`ReadFileTool`] returns by default before the
/// caller must page with `offset`. Mirrors Claude's `MAX_LINES_TO_READ`.
const MAX_LINES_TO_READ: usize = 2000;

/// Stub returned when a file is re-read unchanged since the last read, to
/// avoid re-sending identical content. Mirrors Claude's `FILE_UNCHANGED_STUB`.
const FILE_UNCHANGED_STUB: &str =
    "File unchanged since last read. The content from the earlier read is still current — refer to that instead of re-reading.";

/// Format file content with `cat -n`-style line numbers, 1-indexed.
///
/// Each line is prefixed with a 6-wide right-aligned line number followed by
/// a tab. `start_line` is the 1-indexed number of the first line in `lines`.
fn add_line_numbers(lines: &[&str], start_line: usize) -> String {
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:>6}\t{}", start_line + i, line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract a file's modification time as unix milliseconds.
///
/// Falls back to `0` when the platform doesn't expose mtime or the value
/// predates the epoch — both treated as "unknown / always-stale" so freshness
/// checks err on the safe side (forcing a re-read rather than allowing a
/// possibly-stale edit).
fn mtime_to_ms(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Produce a compact unified-diff-style snippet of the change with line
/// numbers, for the model to confirm the edit landed correctly.
fn format_edit_diff(old_content: &str, new_content: &str) -> String {
    use similar::{ChangeTag, TextDiff};
    let diff = TextDiff::from_lines(old_content, new_content);
    let mut out = String::new();
    let mut old_ln = 1usize;
    let mut new_ln = 1usize;
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                old_ln += 1;
                new_ln += 1;
            }
            ChangeTag::Delete => {
                out.push_str(&format!("{:>6} - {}", old_ln, change.value()));
                if !change.value().ends_with('\n') {
                    out.push('\n');
                }
                old_ln += 1;
            }
            ChangeTag::Insert => {
                out.push_str(&format!("{:>6} + {}", new_ln, change.value()));
                if !change.value().ends_with('\n') {
                    out.push('\n');
                }
                new_ln += 1;
            }
        }
    }
    if out.is_empty() {
        out.push_str("(no textual changes)");
    }
    out
}

/// Read a UTF-8 text file with optional line-based slicing.
///
/// Refuses to load files larger than [`MAX_FILE_SIZE`] unless the caller
/// provides `limit`, ensuring the model is explicit about partial reads of
/// large files. Output is prefixed with a header listing the visible line
/// range so the model can correlate results with subsequent calls.
pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(10)
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

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, ToolError> {
        let args: ReadFileArgs = parse_args(self.name(), args)?;
        let path = PathBuf::from(&args.path);
        let cwd = ctx.cwd.as_deref();

        let metadata = tokio::fs::metadata(&path).await.map_err(|e| {
            ToolError::ExecutionFailed(format!("Failed to stat {}: {}", args.path, e))
        })?;

        if metadata.len() > MAX_FILE_SIZE && args.limit.is_none() {
            return Err(ToolError::OutputTooLarge {
                actual: metadata.len() as usize,
                limit: MAX_FILE_SIZE as usize,
            });
        }

        let mtime_ms = mtime_to_ms(&metadata);

        // Dedup: if we've read this exact range before and the file hasn't
        // changed on disk, return a stub instead of re-sending the content.
        {
            let cache = ctx.file_state.lock();
            if let Some(state) = cache.get(&args.path, cwd) {
                if state.offset == args.offset
                    && state.limit == args.limit
                    && state.timestamp == mtime_ms
                {
                    return Ok(FILE_UNCHANGED_STUB.to_string());
                }
            }
        }

        let content = tokio::fs::read_to_string(&path).await.map_err(|e| {
            // A non-UTF-8 (binary, GBK, UTF-16, …) file surfaces as
            // `InvalidData` from read_to_string. Give the model a clear,
            // actionable message instead of the opaque "stream did not contain
            // valid UTF-8" so it doesn't retry blindly.
            if e.kind() == std::io::ErrorKind::InvalidData {
                ToolError::ExecutionFailed(format!(
                    "{} is not a UTF-8 text file (binary or non-UTF-8 encoding). \
                     This tool only reads UTF-8 text; it cannot display this file.",
                    args.path
                ))
            } else {
                ToolError::ExecutionFailed(format!("Failed to read {}: {}", args.path, e))
            }
        })?;

        // Empty file: return a system reminder rather than an empty string so
        // the model knows the read succeeded but there's nothing to show.
        if content.is_empty() {
            ctx.file_state.lock().record_read(
                &args.path,
                cwd,
                content,
                mtime_ms,
                args.offset,
                args.limit,
            );
            return Ok(format!(
                "<system-reminder>File {} exists but is empty.</system-reminder>",
                args.path
            ));
        }

        // Normalize CRLF to LF for consistent line handling + cache storage.
        let normalized = content.replace("\r\n", "\n");
        let lines: Vec<&str> = normalized.split('\n').collect();
        let total = lines.len();
        let start = args.offset.unwrap_or(0);

        if start >= total {
            return Ok(format!(
                "<system-reminder>Offset {} is past end of file ({} lines).</system-reminder>",
                start, total
            ));
        }

        // Default cap at MAX_LINES_TO_READ when no explicit limit.
        let effective_limit = args.limit.unwrap_or(MAX_LINES_TO_READ);
        let end = start.saturating_add(effective_limit).min(total);

        let body = add_line_numbers(&lines[start..end], start + 1);
        let truncated_note = if end < total {
            format!(
                "\n<system-reminder>Showing lines {}-{} of {}. Use offset to read more.</system-reminder>",
                start + 1,
                end,
                total
            )
        } else {
            String::new()
        };

        // Record the read in the file-state cache for read-before-write.
        ctx.file_state.lock().record_read(
            &args.path,
            cwd,
            normalized,
            mtime_ms,
            args.offset,
            args.limit,
        );

        Ok(format!("{}{}", body, truncated_note))
    }
}

// ---------------------------------------------------------------------------
// ListDirectoryTool
// ---------------------------------------------------------------------------

/// Arguments accepted by [`ListDirectoryTool`].
#[derive(Debug, Deserialize)]
struct ListDirectoryArgs {
    /// Directory to list.
    path: String,
    /// When `true`, descend into subdirectories up to `max_depth`.
    #[serde(default)]
    recursive: bool,
    /// Maximum recursion depth. Defaults to 3, capped at
    /// [`MAX_RECURSIVE_DEPTH`].
    #[serde(default)]
    max_depth: Option<usize>,
}

/// List the contents of a directory.
///
/// Always prunes the well-known noisy directories in [`IGNORED_DIRS`] so the
/// model isn't flooded with `node_modules` or `target` entries. Recursive
/// listings are bounded by [`MAX_RECURSIVE_DEPTH`] and capped at
/// [`MAX_LIST_ENTRIES`] to keep responses focused.
pub struct ListDirectoryTool;

#[async_trait]
impl Tool for ListDirectoryTool {
    fn name(&self) -> &str {
        "list_directory"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(10)
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

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, ToolError> {
        let args: ListDirectoryArgs = parse_args(self.name(), args)?;
        let root = PathBuf::from(&args.path);
        let display_path = args.path.clone();
        let depth = if args.recursive {
            args.max_depth.unwrap_or(3).min(MAX_RECURSIVE_DEPTH)
        } else {
            1
        };

        let result = tokio::task::spawn_blocking(move || -> Result<String, ToolError> {
            if !root.is_dir() {
                return Err(ToolError::ExecutionFailed(format!(
                    "Not a directory: {}",
                    display_path
                )));
            }

            let mut entries: Vec<String> = Vec::new();
            let walker = WalkDir::new(&root)
                .max_depth(depth)
                .into_iter()
                .filter_entry(|entry| {
                    // Always allow the root itself; otherwise prune ignored dirs.
                    if entry.depth() == 0 {
                        return true;
                    }
                    if entry.file_type().is_dir() {
                        !is_ignored_dir_name(entry.file_name())
                    } else {
                        true
                    }
                });

            let mut truncated = false;
            for entry in walker.filter_map(|e| e.ok()) {
                if entry.depth() == 0 {
                    continue;
                }
                if entries.len() >= MAX_LIST_ENTRIES {
                    truncated = true;
                    break;
                }
                let kind = if entry.file_type().is_dir() {
                    "dir"
                } else {
                    "file"
                };
                let rel = entry
                    .path()
                    .strip_prefix(&root)
                    .unwrap_or_else(|_| entry.path());
                entries.push(format!("{:<5} {}", kind, rel.display()));
            }

            if entries.is_empty() {
                return Ok(format!("(empty directory: {})", display_path));
            }

            if truncated {
                entries.push(format!("[truncated at {} entries]", MAX_LIST_ENTRIES));
            }

            Ok(entries.join("\n"))
        })
        .await
        .map_err(join_error_to_tool_error)?;

        result
    }
}

// ---------------------------------------------------------------------------
// WriteFileTool
// ---------------------------------------------------------------------------

/// Arguments accepted by [`WriteFileTool`].
#[derive(Debug, Deserialize)]
struct WriteFileArgs {
    /// Destination path. Parent directories are created as needed.
    path: String,
    /// File contents as a UTF-8 string.
    content: String,
}

/// Write a UTF-8 text file, creating any missing parent directories.
///
/// Overwrites the destination atomically on most filesystems via
/// [`tokio::fs::write`]. Mutating tool — never executed in parallel.
pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn is_read_only(&self) -> bool {
        false
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(10)
    }

    fn is_destructive(&self, _input: &Value) -> bool {
        true
    }

    fn interrupt_behavior(&self) -> crate::InterruptBehavior {
        crate::InterruptBehavior::Block
    }

    fn get_path(&self, input: &Value) -> Option<String> {
        input.get("path").and_then(|v| v.as_str()).map(String::from)
    }

    async fn check_permissions(
        &self,
        input: &Value,
        mode: crate::permission::PermissionMode,
    ) -> crate::permission::PermissionResult {
        use crate::permission::*;
        use crate::safety::check_path_safety;

        // Safety check (bypass-immune) — sensitive paths always ask
        if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
            let safety = check_path_safety(path);
            if !safety.safe {
                return PermissionResult::Ask {
                    message: safety.message,
                    decision_reason: Some(DecisionReason::SafetyCheck {
                        reason: format!("write_file: {}", path),
                        classifier_approvable: safety.classifier_approvable,
                    }),
                    suggestions: vec![],
                };
            }
        }

        if mode == PermissionMode::Plan {
            return PermissionResult::Ask {
                message: format!("Plan mode: {} would mutate the filesystem.", self.name()),
                decision_reason: Some(DecisionReason::Mode {
                    mode: PermissionMode::Plan,
                }),
                suggestions: vec![],
            };
        }
        PermissionResult::Passthrough {
            message: format!("{} requires write permission", self.name()),
        }
    }

    async fn validate_input(&self, input: &Value) -> Result<(), String> {
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("");
        if path.is_empty() {
            return Err("'path' is required and must not be empty".into());
        }
        if input.get("content").is_none() {
            return Err("'content' is required".into());
        }
        Ok(())
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, ToolError> {
        let args: WriteFileArgs = parse_args(self.name(), args)?;
        let cwd = ctx.cwd.as_deref();

        // write_file is a wholesale overwrite — no read-before-write check.
        // This matches Claude Code's FileWriteTool: it always succeeds for
        // both new and existing files. Only edit_file requires reading first.
        // The model is told in the tool description to prefer edit_file for
        // modifications, so write_file landing on an existing file is an
        // intentional full replacement.
        let existing = tokio::fs::metadata(&args.path).await.ok().is_some();

        // Snapshot pre-change content for rewind (P2) before we overwrite.
        snapshot_for_rewind(ctx, &args.path).await;

        // Ensure the parent directory exists.
        if let Some(parent) = Path::new(&args.path).parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    ToolError::ExecutionFailed(format!(
                        "Failed to create parent directories for {}: {}",
                        args.path, e
                    ))
                })?;
            }
        }

        let bytes = args.content.len();
        tokio::fs::write(&args.path, &args.content)
            .await
            .map_err(|e| {
                ToolError::ExecutionFailed(format!("Failed to write {}: {}", args.path, e))
            })?;

        // Record the write so subsequent edits don't trip read-before-write.
        if let Ok(md) = tokio::fs::metadata(&args.path).await {
            ctx.file_state.lock().record_write(
                &args.path,
                cwd,
                args.content.replace("\r\n", "\n"),
                mtime_to_ms(&md),
            );
        }

        // Turn diff tracker (Codex-aligned).
        if let Some(ref diff) = ctx.turn_diff {
            if existing {
                diff.record_modify(&args.path);
            } else {
                diff.record_create(&args.path);
            }
        }

        let verb = if existing { "Updated" } else { "Created" };
        Ok(format!("{} {} ({} bytes)", verb, args.path, bytes))
    }
}

// ---------------------------------------------------------------------------
// EditFileTool
// ---------------------------------------------------------------------------

/// Arguments accepted by [`EditFileTool`].
#[derive(Debug, Deserialize)]
struct EditFileArgs {
    /// Path to the file to edit.
    path: String,
    /// Exact text to find (must uniquely identify the location unless replace_all).
    old_string: String,
    /// Replacement text.
    new_string: String,
    /// Replace every occurrence instead of erroring on ambiguity.
    #[serde(default)]
    replace_all: bool,
}

/// Targeted search-and-replace edit on a UTF-8 text file.
///
/// Refuses to operate when the search string is not present, and refuses to
/// blindly replace the first of several matches. The model must either pass
/// `replace_all=true` or supply additional context to disambiguate. Mutating
/// tool — never executed in parallel.
pub struct EditFileTool;

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn is_read_only(&self) -> bool {
        false
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(10)
    }

    fn is_destructive(&self, _input: &Value) -> bool {
        true
    }

    fn interrupt_behavior(&self) -> crate::InterruptBehavior {
        crate::InterruptBehavior::Block
    }

    fn get_path(&self, input: &Value) -> Option<String> {
        input.get("path").and_then(|v| v.as_str()).map(String::from)
    }

    async fn check_permissions(
        &self,
        input: &Value,
        mode: crate::permission::PermissionMode,
    ) -> crate::permission::PermissionResult {
        use crate::permission::*;
        use crate::safety::check_path_safety;

        // Safety check (bypass-immune) — sensitive paths always ask
        if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
            let safety = check_path_safety(path);
            if !safety.safe {
                return PermissionResult::Ask {
                    message: safety.message,
                    decision_reason: Some(DecisionReason::SafetyCheck {
                        reason: format!("edit_file: {}", path),
                        classifier_approvable: safety.classifier_approvable,
                    }),
                    suggestions: vec![],
                };
            }
        }

        if mode == PermissionMode::Plan {
            return PermissionResult::Ask {
                message: format!("Plan mode: {} would mutate the filesystem.", self.name()),
                decision_reason: Some(DecisionReason::Mode {
                    mode: PermissionMode::Plan,
                }),
                suggestions: vec![],
            };
        }
        PermissionResult::Passthrough {
            message: format!("{} requires write permission", self.name()),
        }
    }

    async fn validate_input(&self, input: &Value) -> Result<(), String> {
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("");
        if path.is_empty() {
            return Err("'path' is required and must not be empty".into());
        }
        if input.get("old_string").is_none() {
            return Err("'old_string' is required".into());
        }
        if input.get("new_string").is_none() {
            return Err("'new_string' is required".into());
        }
        Ok(())
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, ToolError> {
        let args: EditFileArgs = parse_args(self.name(), args)?;
        let cwd = ctx.cwd.as_deref();

        // Snapshot pre-change content for rewind (P2) before any write path.
        snapshot_for_rewind(ctx, &args.path).await;

        // Empty old_string = new file creation (only if file doesn't exist).
        let file_exists = tokio::fs::metadata(&args.path).await.is_ok();
        if args.old_string.is_empty() {
            if file_exists {
                // Reject only if file has content.
                let existing = tokio::fs::read_to_string(&args.path)
                    .await
                    .unwrap_or_default();
                if !existing.trim().is_empty() {
                    return Err(ToolError::ExecutionFailed(
                        "Cannot create new file — file already exists with content.".into(),
                    ));
                }
            }
            // Create/overwrite-empty with new_string.
            if let Some(parent) = Path::new(&args.path).parent() {
                if !parent.as_os_str().is_empty() {
                    tokio::fs::create_dir_all(parent).await.map_err(|e| {
                        ToolError::ExecutionFailed(format!("Failed to create dirs: {e}"))
                    })?;
                }
            }
            tokio::fs::write(&args.path, &args.new_string)
                .await
                .map_err(|e| {
                    ToolError::ExecutionFailed(format!("Failed to write {}: {e}", args.path))
                })?;
            // Record write.
            if let Ok(md) = tokio::fs::metadata(&args.path).await {
                ctx.file_state.lock().record_write(
                    &args.path,
                    cwd,
                    args.new_string.clone(),
                    mtime_to_ms(&md),
                );
            }
            return Ok(format!("Created {}", args.path));
        }

        // Existing-file edit path.
        let metadata = tokio::fs::metadata(&args.path).await.map_err(|e| {
            ToolError::ExecutionFailed(format!("Failed to stat {}: {e}", args.path))
        })?;
        let current_mtime = mtime_to_ms(&metadata);

        // Read-before-write: must have a full-read state.
        //
        // Extract the recorded state (cloned) and release the lock before any
        // `.await` — `parking_lot`'s guard is not `Send` and cannot be held
        // across an await point.
        let recorded = {
            let cache = ctx.file_state.lock();
            match cache.get(&args.path, cwd) {
                None => {
                    return Err(ToolError::ExecutionFailed(
                        "File has not been read yet. Read it first before editing.".into(),
                    ));
                }
                Some(state) => {
                    if !state.is_full_read() {
                        return Err(ToolError::ExecutionFailed(
                            "File was only partially read. Read the full file before editing."
                                .into(),
                        ));
                    }
                    (state.timestamp, state.content.clone())
                }
            }
        };
        let (recorded_ts, recorded_content) = recorded;

        // Freshness: if file changed on disk since read, reject. Windows mtime
        // can change without content change, so for full reads compare content
        // as a fallback.
        if current_mtime > recorded_ts {
            let disk = tokio::fs::read_to_string(&args.path)
                .await
                .map(|c| c.replace("\r\n", "\n"))
                .unwrap_or_default();
            if disk != recorded_content {
                return Err(ToolError::ExecutionFailed(
                    "File has been modified since read, either by the user or a linter. Read it again before editing.".into(),
                ));
            }
        }

        let content = tokio::fs::read_to_string(&args.path)
            .await
            .map(|c| c.replace("\r\n", "\n"))
            .map_err(|e| {
                ToolError::ExecutionFailed(format!("Failed to read {}: {e}", args.path))
            })?;

        // Robust match.
        let actual_old = crate::edit_match::find_actual_string(&content, &args.old_string)
            .ok_or_else(|| {
                ToolError::ExecutionFailed(format!(
                    "String to replace not found in file.\nString: {}",
                    args.old_string
                ))
            })?;

        let count = content.matches(&actual_old).count();
        if count > 1 && !args.replace_all {
            return Err(ToolError::ExecutionFailed(format!(
                "Found {count} matches of the string to replace, but replace_all is false. \
                 To replace all occurrences set replace_all to true. To replace only one, \
                 provide more surrounding context to uniquely identify the instance.\nString: {}",
                args.old_string
            )));
        }

        let new_content = if args.replace_all {
            content.replace(&actual_old, &args.new_string)
        } else {
            content.replacen(&actual_old, &args.new_string, 1)
        };

        tokio::fs::write(&args.path, &new_content)
            .await
            .map_err(|e| {
                ToolError::ExecutionFailed(format!("Failed to write {}: {e}", args.path))
            })?;

        // Record the write (invalidates stale-write detection).
        if let Ok(md) = tokio::fs::metadata(&args.path).await {
            ctx.file_state.lock().record_write(
                &args.path,
                cwd,
                new_content.clone(),
                mtime_to_ms(&md),
            );
        }

        // Turn diff tracker (Codex-aligned).
        if let Some(ref diff) = ctx.turn_diff {
            diff.record_modify(&args.path);
        }

        // Structured diff output (sub-task 3.4).
        let diff = format_edit_diff(&content, &new_content);
        Ok(format!(
            "The file {} has been updated. Here's a snippet of the changes:\n{}",
            args.path, diff
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    /// Convenience: build a JSON `Value` from a Rust expression.
    fn args(v: serde_json::Value) -> Value {
        v
    }

    // ---- ReadFileTool ----------------------------------------------------

    #[tokio::test]
    async fn test_read_file_basic() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("hello.txt");
        tokio::fs::write(&path, "line one\nline two\nline three")
            .await
            .expect("write");

        let tool = ReadFileTool;
        let out = tool
            .execute(
                args(json!({ "path": path.to_string_lossy() })),
                &ToolContext::standalone(),
            )
            .await
            .expect("read");

        assert!(out.contains("line one"));
        assert!(out.contains("line two"));
        assert!(out.contains("line three"));
        // cat -n style line numbers, 1-indexed.
        assert!(out.contains("     1\tline one"));
        assert!(out.contains("     3\tline three"));
    }

    #[tokio::test]
    async fn test_read_file_with_offset_limit() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("multi.txt");
        let content: String = (1..=10)
            .map(|i| format!("line{}", i))
            .collect::<Vec<_>>()
            .join("\n");
        tokio::fs::write(&path, content).await.expect("write");

        let tool = ReadFileTool;
        let out = tool
            .execute(
                args(json!({
                    "path": path.to_string_lossy(),
                    "offset": 2,
                    "limit": 3
                })),
                &ToolContext::standalone(),
            )
            .await
            .expect("read");

        assert!(out.contains("line3"));
        assert!(out.contains("line4"));
        assert!(out.contains("line5"));
        assert!(!out.contains("line6"), "should stop before line6");
        // Line numbers reflect true file position (offset 2 → line 3).
        assert!(out.contains("     3\tline3"));
        assert!(out.contains("     5\tline5"));
        assert!(out.contains("Showing lines 3-5 of 10"));
    }

    #[tokio::test]
    async fn test_read_file_non_utf8_gives_clear_message() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("blob.bin");
        // Invalid UTF-8 bytes (lone 0xFF/0xFE, not a valid sequence).
        tokio::fs::write(&path, [0xFF, 0xFE, 0x00, 0x80, 0x81])
            .await
            .expect("write");

        let tool = ReadFileTool;
        let err = tool
            .execute(
                args(json!({ "path": path.to_string_lossy() })),
                &ToolContext::standalone(),
            )
            .await
            .expect_err("non-utf8 file should error");
        let msg = err.to_string();
        assert!(
            msg.contains("not a UTF-8 text file"),
            "expected clear non-utf8 message, got: {msg}"
        );
    }

    #[tokio::test]
    async fn test_read_file_too_large_without_limit_errors() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("big.bin");
        // 2 MB of 'x' — comfortably above the 1 MB threshold.
        let payload = vec![b'x'; 2_000_000];
        tokio::fs::write(&path, payload).await.expect("write");

        let tool = ReadFileTool;
        let err = tool
            .execute(
                args(json!({ "path": path.to_string_lossy() })),
                &ToolContext::standalone(),
            )
            .await
            .expect_err("should refuse oversized read");

        match err {
            ToolError::OutputTooLarge { actual, limit } => {
                assert!(actual >= 2_000_000);
                assert_eq!(limit, MAX_FILE_SIZE as usize);
            }
            other => panic!("expected OutputTooLarge, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_read_file_offset_past_end_returns_empty() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("short.txt");
        tokio::fs::write(&path, "only one line")
            .await
            .expect("write");

        let tool = ReadFileTool;
        let out = tool
            .execute(
                args(json!({
                    "path": path.to_string_lossy(),
                    "offset": 50
                })),
                &ToolContext::standalone(),
            )
            .await
            .expect("read");

        assert!(out.contains("past end of file"));
    }

    #[tokio::test]
    async fn test_read_file_records_file_state() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("tracked.txt");
        tokio::fs::write(&path, "abc\ndef").await.expect("write");
        let path_str = path.to_string_lossy().to_string();

        let tool = ReadFileTool;
        let ctx = ToolContext::standalone();
        tool.execute(args(json!({ "path": &path_str })), &ctx)
            .await
            .expect("read");

        let cache = ctx.file_state.lock();
        let state = cache.get(&path_str, None).expect("state recorded");
        assert_eq!(state.content, "abc\ndef");
        assert!(state.timestamp > 0);
    }

    #[tokio::test]
    async fn test_read_file_dedup_stub_when_unchanged() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("dedup.txt");
        tokio::fs::write(&path, "stable content")
            .await
            .expect("write");
        let path_str = path.to_string_lossy().to_string();

        let tool = ReadFileTool;
        let ctx = ToolContext::standalone();

        // First read returns content.
        let first = tool
            .execute(args(json!({ "path": &path_str })), &ctx)
            .await
            .expect("first read");
        assert!(first.contains("stable content"));

        // Second read of the same unchanged file returns the dedup stub.
        let second = tool
            .execute(args(json!({ "path": &path_str })), &ctx)
            .await
            .expect("second read");
        assert!(
            second.contains("unchanged since last read"),
            "expected dedup stub, got: {second}"
        );
    }

    #[tokio::test]
    async fn test_read_file_empty_returns_reminder() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("empty.txt");
        tokio::fs::write(&path, "").await.expect("write");

        let tool = ReadFileTool;
        let out = tool
            .execute(
                args(json!({ "path": path.to_string_lossy() })),
                &ToolContext::standalone(),
            )
            .await
            .expect("read");
        assert!(out.contains("empty"));
    }

    // ---- ListDirectoryTool ----------------------------------------------

    #[tokio::test]
    async fn test_list_directory_flat() {
        let dir = tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join("a.txt"), "a")
            .await
            .expect("a");
        tokio::fs::write(dir.path().join("b.txt"), "b")
            .await
            .expect("b");
        tokio::fs::create_dir(dir.path().join("sub"))
            .await
            .expect("sub");

        let tool = ListDirectoryTool;
        let out = tool
            .execute(
                args(json!({ "path": dir.path().to_string_lossy() })),
                &ToolContext::standalone(),
            )
            .await
            .expect("list");

        assert!(out.contains("a.txt"));
        assert!(out.contains("b.txt"));
        assert!(out.contains("sub"));
        assert!(out.contains("file "));
        assert!(out.contains("dir  "));
    }

    #[tokio::test]
    async fn test_list_directory_recursive() {
        let dir = tempdir().expect("tempdir");
        tokio::fs::create_dir(dir.path().join("inner"))
            .await
            .expect("inner");
        tokio::fs::write(dir.path().join("inner").join("nested.txt"), "x")
            .await
            .expect("nested");

        let tool = ListDirectoryTool;
        let out = tool
            .execute(
                args(json!({
                    "path": dir.path().to_string_lossy(),
                    "recursive": true
                })),
                &ToolContext::standalone(),
            )
            .await
            .expect("list");

        assert!(out.contains("inner"));
        assert!(out.contains("nested.txt"));
    }

    #[tokio::test]
    async fn test_list_directory_skips_node_modules() {
        let dir = tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join("keep.txt"), "k")
            .await
            .expect("keep");
        tokio::fs::create_dir(dir.path().join("node_modules"))
            .await
            .expect("nm");
        tokio::fs::write(dir.path().join("node_modules").join("dep.js"), "x")
            .await
            .expect("dep");

        let tool = ListDirectoryTool;
        let out = tool
            .execute(
                args(json!({
                    "path": dir.path().to_string_lossy(),
                    "recursive": true
                })),
                &ToolContext::standalone(),
            )
            .await
            .expect("list");

        assert!(out.contains("keep.txt"));
        assert!(
            !out.contains("node_modules"),
            "ignored dir leaked into output: {out}"
        );
        assert!(!out.contains("dep.js"));
    }

    #[tokio::test]
    async fn test_list_directory_empty() {
        let dir = tempdir().expect("tempdir");

        let tool = ListDirectoryTool;
        let out = tool
            .execute(
                args(json!({ "path": dir.path().to_string_lossy() })),
                &ToolContext::standalone(),
            )
            .await
            .expect("list");

        assert!(out.starts_with("(empty directory:"));
    }

    // ---- WriteFileTool --------------------------------------------------

    #[tokio::test]
    async fn test_write_file_creates_parent_dirs() {
        let dir = tempdir().expect("tempdir");
        let nested = dir.path().join("a").join("b").join("c.txt");

        let tool = WriteFileTool;
        let out = tool
            .execute(
                args(json!({
                    "path": nested.to_string_lossy(),
                    "content": "deep content"
                })),
                &ToolContext::standalone(),
            )
            .await
            .expect("write");

        assert!(out.starts_with("Created "));
        assert!(out.contains("c.txt"));
        let read = tokio::fs::read_to_string(&nested).await.expect("read back");
        assert_eq!(read, "deep content");
    }

    #[tokio::test]
    async fn write_new_file_skips_read_check() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("brand_new.txt");
        // No prior read; new file should write fine.
        let out = WriteFileTool
            .execute(
                args(json!({
                    "path": path.to_string_lossy(),
                    "content": "hello"
                })),
                &ToolContext::standalone(),
            )
            .await
            .expect("write new");
        assert!(out.starts_with("Created "));
    }

    #[tokio::test]
    async fn write_existing_file_overwrites_without_read() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("existing.txt");
        tokio::fs::write(&path, "original").await.expect("seed");

        // write_file is a wholesale overwrite and does NOT require a prior
        // read (aligns with Claude Code's FileWriteTool — only edit_file
        // enforces read-before-write). Overwriting an existing file with no
        // prior Read must succeed.
        let out = WriteFileTool
            .execute(
                args(json!({
                    "path": path.to_string_lossy(),
                    "content": "clobber"
                })),
                &ToolContext::standalone(),
            )
            .await
            .expect("overwrite should succeed without read");
        assert!(out.starts_with("Updated "), "got: {out}");
        let after = tokio::fs::read_to_string(&path).await.expect("read");
        assert_eq!(after, "clobber");
    }

    #[tokio::test]
    async fn write_existing_file_succeeds_after_read() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("existing.txt");
        tokio::fs::write(&path, "original").await.expect("seed");
        let path_str = path.to_string_lossy().to_string();

        let ctx = ToolContext::standalone();
        ReadFileTool
            .execute(args(json!({ "path": &path_str })), &ctx)
            .await
            .expect("read");
        let out = WriteFileTool
            .execute(
                args(json!({ "path": &path_str, "content": "replaced" })),
                &ctx,
            )
            .await
            .expect("write");
        assert!(out.starts_with("Updated "), "got: {out}");
        let after = tokio::fs::read_to_string(&path).await.expect("read");
        assert_eq!(after, "replaced");
    }

    #[tokio::test]
    async fn write_existing_file_overwrites_even_if_stale() {
        use std::time::Duration;
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("existing.txt");
        tokio::fs::write(&path, "original").await.expect("seed");
        let path_str = path.to_string_lossy().to_string();

        let ctx = ToolContext::standalone();
        ReadFileTool
            .execute(args(json!({ "path": &path_str })), &ctx)
            .await
            .expect("read");

        // External modification with different content + bumped mtime.
        tokio::time::sleep(Duration::from_millis(20)).await;
        tokio::fs::write(&path, "externally changed")
            .await
            .expect("external write");

        // write_file is a wholesale overwrite: it does NOT do freshness
        // checking (that's edit_file's job). A stale state must NOT block an
        // overwrite — the model explicitly chose to replace the whole file.
        let out = WriteFileTool
            .execute(
                args(json!({ "path": &path_str, "content": "my version" })),
                &ctx,
            )
            .await
            .expect("overwrite should succeed even when stale");
        assert!(out.starts_with("Updated "), "got: {out}");
        let after = tokio::fs::read_to_string(&path).await.expect("read");
        assert_eq!(after, "my version");
    }

    // ---- EditFileTool ---------------------------------------------------

    type SinkRec = (String, i64, String, Option<String>);
    struct VecSink(std::sync::Mutex<Vec<SinkRec>>);
    impl crate::context::FileHistorySink for VecSink {
        fn record(&self, t: &str, s: i64, p: &str, b: Option<String>) {
            self.0.lock().unwrap().push((t.into(), s, p.into(), b));
        }
    }

    #[tokio::test]
    async fn write_records_history_before_overwrite() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("f.txt");
        tokio::fs::write(&path, "old").await.expect("seed");
        let sink = std::sync::Arc::new(VecSink(Default::default()));
        let mut ctx = ToolContext::standalone();
        ctx.thread_id = Some("t".into());
        ctx.message_seq = Some(3);
        ctx.file_history = Some(sink.clone());
        WriteFileTool
            .execute(
                json!({"path": path.to_str().unwrap(), "content": "new"}),
                &ctx,
            )
            .await
            .expect("write");
        // Snapshot out of the lock before any await (clippy: no guard across await).
        let (count, first_before) = {
            let recs = sink.0.lock().unwrap();
            (recs.len(), recs.first().map(|r| r.3.clone()))
        };
        assert_eq!(count, 1);
        assert_eq!(
            first_before.flatten().as_deref(),
            Some("old"),
            "before captured"
        );
        assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), "new");
    }

    #[tokio::test]
    async fn test_edit_file_single_unique_match() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("code.txt");
        tokio::fs::write(&path, "let x = 1;\nlet y = 2;\n")
            .await
            .expect("write");
        let path_str = path.to_string_lossy().to_string();

        let ctx = ToolContext::standalone();
        ReadFileTool
            .execute(args(json!({ "path": &path_str })), &ctx)
            .await
            .expect("read");

        let out = EditFileTool
            .execute(
                args(json!({
                    "path": &path_str,
                    "old_string": "x = 1",
                    "new_string": "x = 42"
                })),
                &ctx,
            )
            .await
            .expect("edit");

        assert!(out.contains("has been updated"), "got: {out}");
        let after = tokio::fs::read_to_string(&path).await.expect("read");
        assert!(after.contains("x = 42"));
        assert!(!after.contains("x = 1"));
    }

    #[tokio::test]
    async fn edit_rejects_unread_file() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("code.txt");
        tokio::fs::write(&path, "let x = 1;").await.expect("write");

        // No read first → must be rejected.
        let err = EditFileTool
            .execute(
                args(json!({
                    "path": path.to_string_lossy(),
                    "old_string": "x = 1",
                    "new_string": "x = 2"
                })),
                &ToolContext::standalone(),
            )
            .await
            .expect_err("should fail");

        match err {
            ToolError::ExecutionFailed(msg) => {
                assert!(msg.contains("has not been read"), "got: {msg}");
            }
            other => panic!("expected ExecutionFailed, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn edit_succeeds_after_read() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("code.txt");
        tokio::fs::write(&path, "foo\nbar\n").await.expect("write");
        let path_str = path.to_string_lossy().to_string();

        let ctx = ToolContext::standalone();
        ReadFileTool
            .execute(args(json!({ "path": &path_str })), &ctx)
            .await
            .expect("read");
        let out = EditFileTool
            .execute(
                args(json!({
                    "path": &path_str,
                    "old_string": "foo",
                    "new_string": "baz"
                })),
                &ctx,
            )
            .await
            .expect("edit");

        assert!(out.contains("has been updated"), "got: {out}");
        let after = tokio::fs::read_to_string(&path).await.expect("read");
        assert_eq!(after, "baz\nbar\n");
    }

    #[tokio::test]
    async fn edit_rejects_stale_file() {
        use std::time::Duration;
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("code.txt");
        tokio::fs::write(&path, "original content\n")
            .await
            .expect("write");
        let path_str = path.to_string_lossy().to_string();

        let ctx = ToolContext::standalone();
        ReadFileTool
            .execute(args(json!({ "path": &path_str })), &ctx)
            .await
            .expect("read");

        // Externally modify the file with genuinely different content and bump
        // mtime so both the timestamp check and content-fallback trip.
        tokio::time::sleep(Duration::from_millis(20)).await;
        tokio::fs::write(&path, "totally different content now\n")
            .await
            .expect("external write");

        let err = EditFileTool
            .execute(
                args(json!({
                    "path": &path_str,
                    "old_string": "different",
                    "new_string": "changed"
                })),
                &ctx,
            )
            .await
            .expect_err("should fail");

        match err {
            ToolError::ExecutionFailed(msg) => {
                assert!(msg.contains("modified since read"), "got: {msg}");
            }
            other => panic!("expected ExecutionFailed, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn edit_string_not_found() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("code.txt");
        tokio::fs::write(&path, "let x = 1;").await.expect("write");
        let path_str = path.to_string_lossy().to_string();

        let ctx = ToolContext::standalone();
        ReadFileTool
            .execute(args(json!({ "path": &path_str })), &ctx)
            .await
            .expect("read");

        let err = EditFileTool
            .execute(
                args(json!({
                    "path": &path_str,
                    "old_string": "missing",
                    "new_string": "anything"
                })),
                &ctx,
            )
            .await
            .expect_err("should fail");

        match err {
            ToolError::ExecutionFailed(msg) => {
                assert!(msg.contains("not found"), "got: {msg}");
            }
            other => panic!("expected ExecutionFailed, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn edit_multiple_matches_without_replace_all_errors() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("code.txt");
        tokio::fs::write(&path, "foo bar foo bar foo")
            .await
            .expect("write");
        let path_str = path.to_string_lossy().to_string();

        let ctx = ToolContext::standalone();
        ReadFileTool
            .execute(args(json!({ "path": &path_str })), &ctx)
            .await
            .expect("read");

        let err = EditFileTool
            .execute(
                args(json!({
                    "path": &path_str,
                    "old_string": "foo",
                    "new_string": "baz"
                })),
                &ctx,
            )
            .await
            .expect_err("should fail");

        match err {
            ToolError::ExecutionFailed(msg) => {
                assert!(msg.contains("Found 3 matches"), "got: {msg}");
                assert!(msg.contains("replace_all"));
            }
            other => panic!("expected ExecutionFailed, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn edit_replace_all() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("code.txt");
        tokio::fs::write(&path, "foo foo foo").await.expect("write");
        let path_str = path.to_string_lossy().to_string();

        let ctx = ToolContext::standalone();
        ReadFileTool
            .execute(args(json!({ "path": &path_str })), &ctx)
            .await
            .expect("read");

        let out = EditFileTool
            .execute(
                args(json!({
                    "path": &path_str,
                    "old_string": "foo",
                    "new_string": "bar",
                    "replace_all": true
                })),
                &ctx,
            )
            .await
            .expect("edit");

        assert!(out.contains("has been updated"), "got: {out}");
        let after = tokio::fs::read_to_string(&path).await.expect("read");
        assert_eq!(after, "bar bar bar");
    }

    #[tokio::test]
    async fn edit_empty_old_string_creates_file() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("new").join("created.txt");
        let path_str = path.to_string_lossy().to_string();

        let ctx = ToolContext::standalone();
        let out = EditFileTool
            .execute(
                args(json!({
                    "path": &path_str,
                    "old_string": "",
                    "new_string": "brand new file body\n"
                })),
                &ctx,
            )
            .await
            .expect("create");

        assert!(out.contains("Created"), "got: {out}");
        let after = tokio::fs::read_to_string(&path).await.expect("read");
        assert_eq!(after, "brand new file body\n");
    }

    #[tokio::test]
    async fn edit_output_contains_diff() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("code.txt");
        tokio::fs::write(&path, "line one\nline two\nline three\n")
            .await
            .expect("write");
        let path_str = path.to_string_lossy().to_string();

        let ctx = ToolContext::standalone();
        ReadFileTool
            .execute(args(json!({ "path": &path_str })), &ctx)
            .await
            .expect("read");

        let out = EditFileTool
            .execute(
                args(json!({
                    "path": &path_str,
                    "old_string": "line two",
                    "new_string": "line TWO changed"
                })),
                &ctx,
            )
            .await
            .expect("edit");

        // Diff snippet should contain both a deletion and an insertion marker.
        assert!(out.contains(" - "), "missing delete marker in: {out}");
        assert!(out.contains(" + "), "missing insert marker in: {out}");
        assert!(out.contains("line TWO changed"), "got: {out}");
    }

    // ---- P4: permission/interrupt metadata ------------------------------

    #[tokio::test]
    async fn read_file_check_permissions_returns_passthrough() {
        use crate::permission::*;
        let t = ReadFileTool;
        let r = t
            .check_permissions(&json!({"path": "x"}), PermissionMode::Default)
            .await;
        assert!(matches!(r, PermissionResult::Passthrough { .. }));
    }

    #[tokio::test]
    async fn read_file_check_permissions_passthrough_in_plan_mode() {
        // Read tools should NOT block in plan mode — the runner relies on
        // them to inspect the world before proposing edits.
        use crate::permission::*;
        let t = ReadFileTool;
        let r = t
            .check_permissions(&json!({"path": "x"}), PermissionMode::Plan)
            .await;
        assert!(matches!(r, PermissionResult::Passthrough { .. }));
    }

    #[test]
    fn read_file_metadata_flags() {
        let t = ReadFileTool;
        assert!(t.is_read_only());
        assert!(t.is_parallel_safe());
        assert!(!t.is_destructive(&json!({})));
        assert_eq!(t.interrupt_behavior(), crate::InterruptBehavior::Cancel);
        assert_eq!(t.get_path(&json!({"path": "/a"})).as_deref(), Some("/a"));
        assert!(t.get_path(&json!({})).is_none());
    }

    #[test]
    fn list_directory_metadata_flags() {
        let t = ListDirectoryTool;
        assert!(t.is_read_only());
        assert_eq!(t.interrupt_behavior(), crate::InterruptBehavior::Cancel);
        assert_eq!(t.get_path(&json!({"path": "/a"})).as_deref(), Some("/a"));
    }

    #[tokio::test]
    async fn write_file_in_plan_mode_returns_ask() {
        use crate::permission::*;
        let t = WriteFileTool;
        let r = t
            .check_permissions(&json!({"path": "x"}), PermissionMode::Plan)
            .await;
        match r {
            PermissionResult::Ask {
                decision_reason:
                    Some(DecisionReason::Mode {
                        mode: PermissionMode::Plan,
                    }),
                ..
            } => {}
            other => panic!("expected ask via plan mode, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn write_file_in_default_mode_returns_passthrough() {
        use crate::permission::*;
        let t = WriteFileTool;
        let r = t
            .check_permissions(&json!({"path": "x"}), PermissionMode::Default)
            .await;
        assert!(matches!(r, PermissionResult::Passthrough { .. }));
    }

    #[test]
    fn write_file_metadata_flags() {
        let t = WriteFileTool;
        assert!(!t.is_read_only());
        assert!(t.is_destructive(&json!({})));
        assert_eq!(t.interrupt_behavior(), crate::InterruptBehavior::Block);
        assert_eq!(t.get_path(&json!({"path": "/a"})).as_deref(), Some("/a"));
    }

    #[tokio::test]
    async fn edit_file_in_plan_mode_returns_ask() {
        use crate::permission::*;
        let t = EditFileTool;
        let r = t
            .check_permissions(&json!({"path": "x"}), PermissionMode::Plan)
            .await;
        assert!(matches!(r, PermissionResult::Ask { .. }));
    }

    #[test]
    fn edit_file_metadata_flags() {
        let t = EditFileTool;
        assert!(!t.is_read_only());
        assert!(t.is_destructive(&json!({})));
        assert_eq!(t.interrupt_behavior(), crate::InterruptBehavior::Block);
    }
}
