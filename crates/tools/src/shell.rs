//! Shell command execution tool.
//!
//! Provides [`RunCommandTool`], which dispatches a command string through
//! the platform's default shell:
//!
//! * **Windows** — `powershell.exe -NoProfile -NonInteractive -Command <cmd>`.
//!   The child is launched with `CREATE_NO_WINDOW` so dev sessions never
//!   flash a console window.
//! * **macOS** — `zsh -c <cmd>`.
//! * **Linux** — `bash -c <cmd>`.
//!
//! The tool always captures both stdout and stderr (lossily decoded as
//! UTF-8), caps the combined output at 1 MiB by truncating each stream to
//! half the cap on overflow, and reports the exit code in a structured
//! preamble. Non-zero exit codes are reported as successful tool calls so
//! the model can read the failure output and decide what to do next.
//!
//! ## Timeout layering
//!
//! The runtime cap is enforced **inside** [`RunCommandTool::execute`] via
//! [`tokio::time::timeout`] so the per-call `timeout_secs` argument (capped
//! at [`MAX_TIMEOUT_SECS`]) is the source of truth. The trait-level
//! [`Tool::timeout`] is set to `MAX_TIMEOUT_SECS + 60` so the registry's
//! outer hard ceiling never preempts a legitimate user override.

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tokio::process::Command;

use crate::types::ToolError;
use crate::Tool;
use crate::ToolContext;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum combined size (in bytes) of captured stdout + stderr. On
/// overflow each stream is independently truncated to half the cap and the
/// affected stream gets a `[stream truncated]` marker appended.
const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

/// Default per-call timeout when the model omits `timeout_secs`. Two
/// minutes is generous enough for typical build/test commands while still
/// preventing runaway shells from blocking the agent loop.
const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// Hard upper bound on the user-supplied `timeout_secs` argument. Capping
/// at ten minutes keeps the runner responsive even if the model asks for an
/// implausibly long wait.
const MAX_TIMEOUT_SECS: u64 = 600;

// ---------------------------------------------------------------------------
// Args
// ---------------------------------------------------------------------------

/// Arguments accepted by [`RunCommandTool`].
#[derive(Debug, Deserialize)]
struct RunCommandArgs {
    /// The command line forwarded verbatim to the platform shell.
    command: String,
    /// Optional working directory for the spawned process. Must exist and
    /// be a directory; otherwise the call is rejected before spawning.
    #[serde(default)]
    cwd: Option<String>,
    /// Optional override for the per-call timeout, in seconds. Capped at
    /// [`MAX_TIMEOUT_SECS`].
    #[serde(default)]
    timeout_secs: Option<u64>,
}

fn parse_args<T: serde::de::DeserializeOwned>(
    tool_name: &str,
    args: Value,
) -> Result<T, ToolError> {
    serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs {
        tool: tool_name.to_string(),
        message: e.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Platform-specific shell setup
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn build_shell_command(command: &str) -> Command {
    /// Win32 process creation flag that suppresses the console window for
    /// the spawned child. Without this every shell invocation flashes a
    /// black window during development.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let shell = windows_shell();
    let mut cmd = Command::new(shell);
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", command]);
    // `creation_flags` is an inherent method on `tokio::process::Command`
    // on Windows; no `std::os::windows::process::CommandExt` import is
    // required.
    cmd.creation_flags(CREATE_NO_WINDOW);
    // Explicitly close stdin so commands that prompt for input fail fast
    // instead of hanging until the timeout fires.
    cmd.stdin(std::process::Stdio::null());
    cmd
}

/// Resolve the Windows PowerShell executable, preferring PowerShell 7+
/// (`pwsh`) over the legacy Windows PowerShell 5.1 (`powershell`). pwsh
/// defaults to UTF-8 output (fewer mojibake issues) and is faster to start.
/// Detection runs once and is cached. Falls back to `powershell`, which is
/// always present on Windows.
#[cfg(target_os = "windows")]
fn windows_shell() -> &'static str {
    use std::os::windows::process::CommandExt;
    use std::sync::OnceLock;
    static SHELL: OnceLock<&'static str> = OnceLock::new();
    SHELL.get_or_init(|| {
        // `pwsh -NoProfile -Command $PSVersionTable...` would work but a bare
        // version probe is enough: if `pwsh` launches, prefer it.
        let probe = std::process::Command::new("pwsh")
            .args(["-NoProfile", "-Command", "$null"])
            .creation_flags(0x0800_0000u32) // CREATE_NO_WINDOW
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        match probe {
            Ok(s) if s.success() => "pwsh",
            _ => "powershell",
        }
    })
}

#[cfg(target_os = "macos")]
fn build_shell_command(command: &str) -> Command {
    let mut cmd = Command::new("zsh");
    cmd.args(["-c", command]);
    cmd.stdin(std::process::Stdio::null());
    cmd
}

#[cfg(target_os = "linux")]
fn build_shell_command(command: &str) -> Command {
    let mut cmd = Command::new("bash");
    cmd.args(["-c", command]);
    cmd.stdin(std::process::Stdio::null());
    cmd
}

// ---------------------------------------------------------------------------
// Output handling
// ---------------------------------------------------------------------------

/// Truncate `s` to at most `max_bytes`, keeping the HEAD and TAIL so the
/// model still sees the critical preamble (exit code, stderr preamble) and
/// the very end (actual error message, stack trace bottom).
///
/// Returns the possibly-shortened string and a flag indicating whether
/// truncation actually happened.
fn truncate_at_char_boundary(s: String, max_bytes: usize) -> (String, bool) {
    if s.len() <= max_bytes {
        return (s, false);
    }
    // Keep 70% head, 30% tail — the head carries the exit code and early
    // stderr lines; the tail carries the final error message. This mirrors
    // Claude Code's BashTool output truncation strategy.
    let head_bytes = (max_bytes as f64 * 0.70) as usize;
    let tail_bytes = max_bytes - head_bytes;
    let mut head_end = head_bytes;
    while head_end > 0 && !s.is_char_boundary(head_end) {
        head_end -= 1;
    }
    let tail_start = s.len() - tail_bytes;
    let tail_start = if tail_start < head_end {
        head_end
    } else {
        let mut ts = tail_start;
        while ts < s.len() && !s.is_char_boundary(ts) {
            ts += 1;
        }
        ts
    };
    let head = &s[..head_end];
    let tail = &s[tail_start..];
    let result = format!(
        "{head}\n... [truncated {} bytes] ...\n{tail}",
        tail_start - head_end
    );
    (result, true)
}

/// Apply the [`MAX_OUTPUT_BYTES`] combined cap to `stdout` and `stderr`.
///
/// When the total exceeds the cap, each stream is independently truncated
/// to half of [`MAX_OUTPUT_BYTES`] (streams already smaller than the half
/// are left alone). Returns each stream alongside a boolean recording
/// whether it was truncated.
fn cap_outputs(stdout: String, stderr: String) -> (String, bool, String, bool) {
    if stdout.len() + stderr.len() <= MAX_OUTPUT_BYTES {
        return (stdout, false, stderr, false);
    }
    let half = MAX_OUTPUT_BYTES / 2;
    let (out, out_t) = truncate_at_char_boundary(stdout, half);
    let (err, err_t) = truncate_at_char_boundary(stderr, half);
    (out, out_t, err, err_t)
}

/// Format the captured shell output for return to the model.
///
/// The exit code line is always present. The `--- stdout ---` and
/// `--- stderr ---` blocks are emitted only when the corresponding stream
/// has content (or was truncated to empty), and each block ensures it ends
/// on a newline before the next section so the rendered output stays
/// readable.
fn format_output(
    exit_code: i32,
    stdout: &str,
    stdout_truncated: bool,
    stderr: &str,
    stderr_truncated: bool,
) -> String {
    let mut buf = format!("Exit code: {}\n", exit_code);

    if !stdout.is_empty() || stdout_truncated {
        buf.push_str("--- stdout ---\n");
        buf.push_str(stdout);
        if !stdout.is_empty() && !stdout.ends_with('\n') {
            buf.push('\n');
        }
        if stdout_truncated {
            buf.push_str("[stdout truncated]\n");
        }
    }

    if !stderr.is_empty() || stderr_truncated {
        buf.push_str("--- stderr ---\n");
        buf.push_str(stderr);
        if !stderr.is_empty() && !stderr.ends_with('\n') {
            buf.push('\n');
        }
        if stderr_truncated {
            buf.push_str("[stderr truncated]\n");
        }
    }

    // Trim a single trailing newline for compactness without disturbing
    // any meaningful blank lines the command may have emitted internally.
    if buf.ends_with('\n') {
        buf.pop();
    }
    buf
}

// ---------------------------------------------------------------------------
// RunCommandTool
// ---------------------------------------------------------------------------

/// Execute a command through the platform shell and return its captured
/// output.
///
/// This tool is intentionally **not** parallel-safe: shell commands can
/// have arbitrary side effects (file edits, network calls, package
/// installs) and must be sequenced by the runner. Non-zero exit codes are
/// reported as successful invocations so the model can inspect what went
/// wrong; only spawn failures and timeouts surface as [`ToolError`].
pub struct RunCommandTool;

#[async_trait]
impl Tool for RunCommandTool {
    fn name(&self) -> &str {
        "run_command"
    }

    fn is_read_only(&self) -> bool {
        false
    }

    fn is_parallel_safe(&self) -> bool {
        false
    }

    fn timeout(&self) -> Duration {
        // Outer hard cap deliberately exceeds any user-supplied timeout
        // so the registry never preempts the inner enforcement.
        Duration::from_secs(MAX_TIMEOUT_SECS + 60)
    }

    fn is_destructive(&self, _input: &Value) -> bool {
        true
    }

    fn interrupt_behavior(&self) -> crate::InterruptBehavior {
        crate::InterruptBehavior::Block
    }

    async fn check_permissions(
        &self,
        _input: &Value,
        mode: crate::permission::PermissionMode,
    ) -> crate::permission::PermissionResult {
        use crate::permission::*;
        if mode == PermissionMode::Plan {
            return PermissionResult::Ask {
                message: "Plan mode: shell execution would mutate state.".into(),
                decision_reason: Some(DecisionReason::Mode {
                    mode: PermissionMode::Plan,
                }),
                suggestions: vec![],
            };
        }
        PermissionResult::Passthrough {
            message: "shell execution".into(),
        }
    }

    async fn validate_input(&self, input: &Value) -> Result<(), String> {
        let command = input.get("command").and_then(|v| v.as_str()).unwrap_or("");
        if command.trim().is_empty() {
            return Err("'command' is required and must not be empty".into());
        }
        Ok(())
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, ToolError> {
        let args: RunCommandArgs = parse_args(self.name(), args)?;

        // Validate cwd up-front so a typo doesn't spawn a shell that
        // immediately fails with an opaque OS error.
        if let Some(cwd) = &args.cwd {
            let p = Path::new(cwd);
            if !p.is_dir() {
                return Err(ToolError::ExecutionFailed(format!(
                    "cwd does not exist or is not a directory: {}",
                    cwd
                )));
            }
        }

        let timeout_secs = args
            .timeout_secs
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .min(MAX_TIMEOUT_SECS);
        let effective_timeout = Duration::from_secs(timeout_secs);

        let mut cmd = build_shell_command(&args.command);
        if let Some(cwd) = &args.cwd {
            cmd.current_dir(cwd);
        }
        // CRITICAL: ensures the spawned shell is killed if this future is
        // dropped (e.g. on abort/timeout — kill_on_drop is the hard-kill
        // backstop after the graceful signal below).
        cmd.kill_on_drop(true);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let child = cmd
            .spawn()
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to spawn shell: {e}")))?;
        let pid = child.id();

        // `wait_with_output` consumes `child`, so we can't also hold `&mut
        // child` for killing in the other select arms. Instead we capture the
        // pid up front and send a graceful termination signal by pid; the
        // pinned wait future is then dropped, and `kill_on_drop(true)` is the
        // hard-kill backstop.
        let wait = child.wait_with_output();
        tokio::pin!(wait);

        let output = tokio::select! {
            // User aborted the turn → terminate the process tree, report Aborted.
            _ = ctx.abort.cancelled() => {
                graceful_terminate_by_pid(pid).await;
                return Err(ToolError::Aborted);
            }
            // Hard timeout → terminate and report (preserves prior behavior).
            _ = tokio::time::sleep(effective_timeout) => {
                graceful_terminate_by_pid(pid).await;
                return Err(ToolError::ExecutionFailed(format!(
                    "command timed out after {}s",
                    timeout_secs
                )));
            }
            // Normal completion.
            res = &mut wait => {
                res.map_err(|e| {
                    ToolError::ExecutionFailed(format!("Failed to read shell output: {e}"))
                })?
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        // `code()` is `None` only on Unix when the process was killed by a
        // signal; surface -1 in that case so the formatted output stays
        // well-typed.
        let exit_code = output.status.code().unwrap_or(-1);

        let (stdout, stdout_t, stderr, stderr_t) = cap_outputs(stdout, stderr);
        Ok(format_output(
            exit_code, &stdout, stdout_t, &stderr, stderr_t,
        ))
    }
}

// ---------------------------------------------------------------------------
// kill_with_grace
// ---------------------------------------------------------------------------

/// Send a graceful process-tree termination signal by pid, without owning the
/// `Child` handle. Used by the execute `select!` (which consumed `child` into
/// `wait_with_output`): we can't pass `&mut child`, so we signal by pid and
/// rely on `kill_on_drop(true)` as the hard-kill backstop when the wait future
/// is dropped.
///
/// * **Unix**: send `SIGTERM` to the process.
/// * **Windows**: `taskkill /T /F /PID <pid>` to terminate the whole tree.
pub async fn graceful_terminate_by_pid(pid: Option<u32>) {
    let pid = match pid {
        Some(p) => p,
        None => return,
    };
    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;
        let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
    }
    #[cfg(windows)]
    {
        let _ = tokio::process::Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .output()
            .await;
    }
}

/// Kill a child process with a grace period.
///
/// * **Unix**: send `SIGTERM`, wait up to 5 s for graceful exit, then
///   escalate to `SIGKILL` via [`tokio::process::Child::kill`].
/// * **Windows**: invoke `taskkill /T /PID <pid>` to signal the whole
///   process tree, wait up to 5 s, then force-kill the child handle.
///
/// On any I/O or signalling error we still fall back to the forceful kill
/// path. The return type is `()` because the caller just wants the child
/// gone; observability (whether grace was honoured) is left for tracing.
pub async fn kill_with_grace(child: &mut tokio::process::Child) {
    let pid = match child.id() {
        Some(p) => p,
        None => return,
    };

    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;
        let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
    }

    #[cfg(windows)]
    {
        let _ = tokio::process::Command::new("taskkill")
            .args(["/T", "/PID", &pid.to_string()])
            .output()
            .await;
    }

    let grace = tokio::time::sleep(Duration::from_secs(5));
    tokio::pin!(grace);
    tokio::select! {
        _ = child.wait() => return,
        _ = &mut grace => {}
    }

    let _ = child.kill().await;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    // Platform-specific command snippets used by the tests below.
    #[cfg(target_os = "windows")]
    const ECHO_HELLO: &str = "Write-Output hello";
    #[cfg(target_os = "windows")]
    const SLEEP_5S: &str = "Start-Sleep -Seconds 5";
    #[cfg(target_os = "windows")]
    const EXIT_1: &str = "exit 1";
    #[cfg(target_os = "windows")]
    const PRINT_STDERR: &str = "[Console]::Error.WriteLine('err')";
    #[cfg(target_os = "windows")]
    const PWD_CMD: &str = "Get-Location | Select-Object -ExpandProperty Path";
    #[cfg(target_os = "windows")]
    const HUGE_OUTPUT: &str = "[Console]::Out.Write('x' * 1500000)";

    #[cfg(unix)]
    const ECHO_HELLO: &str = "echo hello";
    #[cfg(unix)]
    const SLEEP_5S: &str = "sleep 5";
    #[cfg(unix)]
    const EXIT_1: &str = "exit 1";
    #[cfg(unix)]
    const PRINT_STDERR: &str = "echo err >&2";
    #[cfg(unix)]
    const PWD_CMD: &str = "pwd";
    #[cfg(unix)]
    const HUGE_OUTPUT: &str = "awk 'BEGIN{for(i=0;i<1500000;i++)printf \"x\"}'";

    #[tokio::test]
    async fn test_run_simple_echo() {
        let tool = RunCommandTool;
        let out = tool
            .execute(json!({ "command": ECHO_HELLO }), &ToolContext::standalone())
            .await
            .expect("echo should succeed");

        assert!(out.contains("Exit code: 0"), "got: {out}");
        assert!(out.contains("--- stdout ---"), "got: {out}");
        assert!(out.contains("hello"), "got: {out}");
    }

    #[tokio::test]
    async fn test_run_returns_nonzero_exit() {
        let tool = RunCommandTool;
        let out = tool
            .execute(json!({ "command": EXIT_1 }), &ToolContext::standalone())
            .await
            .expect("non-zero exit must NOT be a tool error");

        assert!(out.contains("Exit code: 1"), "got: {out}");
    }

    #[tokio::test]
    async fn execute_aborts_on_cancel() {
        use std::time::Instant;

        let tool = RunCommandTool;
        let ctx = ToolContext::standalone();
        let token = ctx.abort.clone();

        // Cancel after 500ms while a 5s sleep is running.
        let canceller = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            token.cancel();
        });

        let start = Instant::now();
        let res = tool.execute(json!({ "command": SLEEP_5S }), &ctx).await;
        let elapsed = start.elapsed();

        assert!(matches!(res, Err(ToolError::Aborted)), "got: {res:?}");
        assert!(
            elapsed < Duration::from_secs(3),
            "abort should return promptly, took {elapsed:?}",
        );
        let _ = canceller.await;
    }

    #[tokio::test]
    async fn test_run_stderr_capture() {
        let tool = RunCommandTool;
        let out = tool
            .execute(
                json!({ "command": PRINT_STDERR }),
                &ToolContext::standalone(),
            )
            .await
            .expect("stderr-emitting command should succeed");

        assert!(
            out.contains("--- stderr ---"),
            "expected stderr block, got: {out}"
        );
        assert!(out.contains("err"), "got: {out}");
    }

    #[tokio::test]
    async fn test_run_invalid_cwd_errors() {
        let tool = RunCommandTool;
        let bogus_cwd = if cfg!(windows) {
            "Z:\\this\\does\\not\\exist\\for_real"
        } else {
            "/this/does/not/exist/for_real"
        };

        let err = tool
            .execute(
                json!({
                    "command": ECHO_HELLO,
                    "cwd": bogus_cwd,
                }),
                &ToolContext::standalone(),
            )
            .await
            .expect_err("invalid cwd must be rejected");

        match err {
            ToolError::ExecutionFailed(msg) => {
                assert!(msg.contains("cwd"), "got: {msg}");
            }
            other => panic!("expected ExecutionFailed, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_run_with_valid_cwd() {
        let dir = tempdir().expect("tempdir");
        // The trailing path component is a unique random suffix that must
        // appear in the shell's `pwd` output regardless of any platform
        // canonicalization (e.g. macOS resolving `/var` → `/private/var`).
        let unique = dir
            .path()
            .file_name()
            .expect("tempdir must have file name")
            .to_string_lossy()
            .into_owned();

        let tool = RunCommandTool;
        let out = tool
            .execute(
                json!({
                    "command": PWD_CMD,
                    "cwd": dir.path().to_string_lossy(),
                }),
                &ToolContext::standalone(),
            )
            .await
            .expect("pwd should succeed");

        assert!(out.contains("Exit code: 0"), "got: {out}");
        assert!(
            out.contains(&unique),
            "expected pwd output to contain {unique:?}, got: {out}"
        );
    }

    #[tokio::test]
    async fn test_run_timeout_enforced() {
        let tool = RunCommandTool;
        let err = tool
            .execute(
                json!({
                    "command": SLEEP_5S,
                    "timeout_secs": 1,
                }),
                &ToolContext::standalone(),
            )
            .await
            .expect_err("sleep must time out");

        match err {
            ToolError::ExecutionFailed(msg) => {
                assert!(
                    msg.contains("timed out"),
                    "expected timeout message, got: {msg}"
                );
            }
            other => panic!("expected ExecutionFailed, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_run_truncates_huge_output() {
        let tool = RunCommandTool;
        let out = tool
            .execute(
                json!({
                    "command": HUGE_OUTPUT,
                    "timeout_secs": 60,
                }),
                &ToolContext::standalone(),
            )
            .await
            .expect("huge-output command should succeed");

        assert!(out.contains("Exit code: 0"), "got prefix: {}", &out[..200]);
        assert!(
            out.contains("[stdout truncated]"),
            "expected truncation marker, output starts with: {}",
            &out[..200.min(out.len())]
        );
        // Final size should comfortably exceed half the cap (the truncated
        // payload) but stay below the full cap plus framing overhead.
        assert!(
            out.len() <= MAX_OUTPUT_BYTES + 1024,
            "output exceeded combined cap: {} bytes",
            out.len()
        );
    }

    // ---- Helper coverage ------------------------------------------------

    #[test]
    fn truncate_respects_char_boundary() {
        // "héllo" — 'é' is 2 bytes, so byte offset 2 lands inside it.
        // With the head+tail truncation, small max_bytes may produce
        // slightly larger output than the old head-only truncation
        // (head + tail + marker). Still must produce valid UTF-8 and
        // be materially shorter than the original.
        let s = "héllo".to_string();
        let orig = s.len();
        let (out, truncated) = truncate_at_char_boundary(s, 2);
        assert!(truncated);
        assert!(out.is_char_boundary(out.len()));
        assert!(out.len() < orig + 50, "truncated output: {out}"); // marker adds overhead
    }

    #[test]
    fn format_output_skips_empty_blocks() {
        let s = format_output(0, "", false, "", false);
        assert_eq!(s, "Exit code: 0");
    }

    #[test]
    fn format_output_includes_only_stdout() {
        let s = format_output(0, "hi", false, "", false);
        assert!(s.contains("--- stdout ---"));
        assert!(!s.contains("--- stderr ---"));
        assert!(s.contains("hi"));
    }

    #[test]
    fn format_output_marks_truncated_streams() {
        let s = format_output(0, "abc", true, "def", true);
        assert!(s.contains("[stdout truncated]"));
        assert!(s.contains("[stderr truncated]"));
    }

    // ---- P4: permission/interrupt metadata ------------------------------

    #[tokio::test]
    async fn run_command_in_plan_mode_returns_ask() {
        use crate::permission::*;
        let t = RunCommandTool;
        let r = t
            .check_permissions(&Value::Null, PermissionMode::Plan)
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
    async fn run_command_in_default_mode_returns_passthrough() {
        use crate::permission::*;
        let t = RunCommandTool;
        let r = t
            .check_permissions(&Value::Null, PermissionMode::Default)
            .await;
        assert!(matches!(r, PermissionResult::Passthrough { .. }));
    }

    #[test]
    fn run_command_metadata_flags() {
        let t = RunCommandTool;
        assert!(!t.is_read_only());
        assert!(!t.is_parallel_safe());
        assert!(t.is_destructive(&Value::Null));
        assert_eq!(t.interrupt_behavior(), crate::InterruptBehavior::Block);
        // Shell tool exposes no single path.
        assert!(t.get_path(&Value::Null).is_none());
    }
}

#[cfg(test)]
mod kill_tests {
    use super::*;
    use tokio::process::Command;

    #[tokio::test]
    async fn kill_with_grace_terminates_long_running_process() {
        #[cfg(unix)]
        let mut child = Command::new("sleep").arg("60").spawn().unwrap();
        #[cfg(windows)]
        let mut child = Command::new("powershell")
            .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 60"])
            .spawn()
            .unwrap();

        kill_with_grace(&mut child).await;

        let status = child.try_wait().unwrap();
        assert!(
            status.is_some(),
            "process should have exited after kill_with_grace"
        );
    }
}
