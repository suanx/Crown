use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::debug;

const DEFAULT_HOOK_TIMEOUT_SECS: u64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    UserPromptSubmit,
    Stop,
    StopFailure,
    SessionStart,
    SessionEnd,
    SubagentStart,
    SubagentStop,
    Notification,
    PreCompact,
    PostCompact,
    PermissionDenied,
    PermissionRequest,
    CwdChanged,
    FileChanged,
    InstructionsLoaded,
    TaskCreated,
    TaskCompleted,
    ConfigChange,
    Setup,
}

impl HookEvent {
    pub fn all() -> &'static [HookEvent] {
        &[
            HookEvent::PreToolUse,
            HookEvent::PostToolUse,
            HookEvent::PostToolUseFailure,
            HookEvent::UserPromptSubmit,
            HookEvent::Stop,
            HookEvent::StopFailure,
            HookEvent::SessionStart,
            HookEvent::SessionEnd,
            HookEvent::SubagentStart,
            HookEvent::SubagentStop,
            HookEvent::Notification,
            HookEvent::PreCompact,
            HookEvent::PostCompact,
            HookEvent::PermissionDenied,
            HookEvent::PermissionRequest,
            HookEvent::CwdChanged,
            HookEvent::FileChanged,
            HookEvent::InstructionsLoaded,
            HookEvent::TaskCreated,
            HookEvent::TaskCompleted,
            HookEvent::ConfigChange,
            HookEvent::Setup,
        ]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            HookEvent::PreToolUse => "PreToolUse",
            HookEvent::PostToolUse => "PostToolUse",
            HookEvent::PostToolUseFailure => "PostToolUseFailure",
            HookEvent::UserPromptSubmit => "UserPromptSubmit",
            HookEvent::Stop => "Stop",
            HookEvent::StopFailure => "StopFailure",
            HookEvent::SessionStart => "SessionStart",
            HookEvent::SessionEnd => "SessionEnd",
            HookEvent::SubagentStart => "SubagentStart",
            HookEvent::SubagentStop => "SubagentStop",
            HookEvent::Notification => "Notification",
            HookEvent::PreCompact => "PreCompact",
            HookEvent::PostCompact => "PostCompact",
            HookEvent::PermissionDenied => "PermissionDenied",
            HookEvent::PermissionRequest => "PermissionRequest",
            HookEvent::CwdChanged => "CwdChanged",
            HookEvent::FileChanged => "FileChanged",
            HookEvent::InstructionsLoaded => "InstructionsLoaded",
            HookEvent::TaskCreated => "TaskCreated",
            HookEvent::TaskCompleted => "TaskCompleted",
            HookEvent::ConfigChange => "ConfigChange",
            HookEvent::Setup => "Setup",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::all()
            .iter()
            .copied()
            .find(|event| event.as_str().eq_ignore_ascii_case(name))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookConfigFile {
    #[serde(default)]
    pub disable_all_hooks: bool,
    #[serde(default)]
    pub trusted_projects: Vec<String>,
    #[serde(default)]
    pub hooks: HashMap<String, Vec<HookMatcherConfig>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookMatcherConfig {
    #[serde(default)]
    pub matcher: Option<String>,
    #[serde(default)]
    pub hooks: Vec<HookCommandConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookCommandConfig {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub r#type: HookCommandType,
    pub command: String,
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HookCommandType {
    #[default]
    Command,
    Prompt,
    Agent,
    Http,
}

#[derive(Debug, Clone)]
pub struct MatchedHook {
    pub source: HookSource,
    pub matcher: Option<String>,
    pub hook: HookCommandConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HookSource {
    Global,
    Project,
}

#[derive(Debug, Clone, Default)]
pub struct AggregatedHookResult {
    pub blocking_error: Option<String>,
    pub permission_behavior: Option<HookPermissionBehavior>,
    pub permission_decision_reason: Option<String>,
    pub updated_input: Option<Value>,
    pub additional_contexts: Vec<String>,
    pub traces: Vec<HookTraceEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookPermissionBehavior {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookTraceEntry {
    pub event: String,
    pub hook_id: Option<String>,
    pub source: HookSource,
    pub matcher: Option<String>,
    pub command: String,
    pub outcome: String,
    pub duration_ms: u64,
    pub exit_code: Option<i32>,
    pub blocking_reason: Option<String>,
    pub stdout: String,
    pub stderr: String,
}

pub struct HookRunner {
    global: HookConfigFile,
    project: Option<HookConfigFile>,
}

impl HookRunner {
    pub fn from_configs(global: HookConfigFile, project: Option<HookConfigFile>) -> Self {
        Self { global, project }
    }

    pub fn load(cwd: Option<&Path>) -> Self {
        let global = read_global_config();
        let project = if global.disable_all_hooks {
            None
        } else {
            read_project_config_if_trusted(cwd, &global)
        };
        Self { global, project }
    }

    pub fn is_disabled(&self) -> bool {
        self.global.disable_all_hooks
    }

    pub async fn run(
        &self,
        event: HookEvent,
        input: Value,
        matcher_query: Option<&str>,
        cwd: Option<&Path>,
        abort: &CancellationToken,
    ) -> AggregatedHookResult {
        let hooks = self.matching_hooks(event, matcher_query);
        let mut aggregate = AggregatedHookResult::default();
        if self.is_disabled() || hooks.is_empty() {
            return aggregate;
        }

        for matched in hooks {
            if abort.is_cancelled() {
                break;
            }
            if matched.hook.r#type != HookCommandType::Command {
                aggregate.traces.push(HookTraceEntry {
                    event: event.as_str().to_string(),
                    hook_id: matched.hook.id.clone(),
                    source: matched.source,
                    matcher: matched.matcher.clone(),
                    command: matched.hook.command.clone(),
                    outcome: "unsupported".into(),
                    duration_ms: 0,
                    exit_code: None,
                    blocking_reason: None,
                    stdout: String::new(),
                    stderr: format!("unsupported hook type: {:?}", matched.hook.r#type),
                });
                continue;
            }
            let run = run_command_hook(event, &matched, input.clone(), cwd, abort).await;
            apply_hook_output(event, &run, &mut aggregate);
            aggregate.traces.push(run);
            if aggregate.blocking_error.is_some()
                || matches!(
                    aggregate.permission_behavior,
                    Some(HookPermissionBehavior::Deny)
                )
            {
                break;
            }
        }
        aggregate
    }

    fn matching_hooks(&self, event: HookEvent, matcher_query: Option<&str>) -> Vec<MatchedHook> {
        let mut out = Vec::new();
        collect_matching_hooks(
            &mut out,
            HookSource::Global,
            &self.global,
            event,
            matcher_query,
        );
        if let Some(project) = &self.project {
            collect_matching_hooks(&mut out, HookSource::Project, project, event, matcher_query);
        }
        out
    }
}

pub async fn run_single_hook_for_test(
    event: HookEvent,
    hook: HookCommandConfig,
    input: Value,
    matcher: Option<String>,
    cwd: Option<&Path>,
    abort: &CancellationToken,
) -> HookTraceEntry {
    let matched = MatchedHook {
        source: HookSource::Global,
        matcher,
        hook,
    };
    if matched.hook.r#type != HookCommandType::Command {
        return HookTraceEntry {
            event: event.as_str().to_string(),
            hook_id: matched.hook.id.clone(),
            source: matched.source,
            matcher: matched.matcher.clone(),
            command: matched.hook.command.clone(),
            outcome: "unsupported".into(),
            duration_ms: 0,
            exit_code: None,
            blocking_reason: None,
            stdout: String::new(),
            stderr: format!("unsupported hook type: {:?}", matched.hook.r#type),
        };
    }
    run_command_hook(event, &matched, input, cwd, abort).await
}

fn collect_matching_hooks(
    out: &mut Vec<MatchedHook>,
    source: HookSource,
    config: &HookConfigFile,
    event: HookEvent,
    matcher_query: Option<&str>,
) {
    let Some(groups) = config.hooks.get(event.as_str()) else {
        return;
    };
    for group in groups {
        if !matcher_matches(group.matcher.as_deref(), matcher_query) {
            continue;
        }
        for hook in &group.hooks {
            if hook.enabled {
                out.push(MatchedHook {
                    source,
                    matcher: group.matcher.clone(),
                    hook: hook.clone(),
                });
            }
        }
    }
}

fn default_enabled() -> bool {
    true
}

fn read_global_config() -> HookConfigFile {
    let Some(path) = dirs::config_dir().map(|dir| dir.join("crown").join("config.json")) else {
        return HookConfigFile::default();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return HookConfigFile::default();
    };
    serde_json::from_str::<HookConfigFile>(&text).unwrap_or_default()
}

fn read_project_config_if_trusted(
    cwd: Option<&Path>,
    global: &HookConfigFile,
) -> Option<HookConfigFile> {
    let cwd = cwd?;
    if !project_is_trusted(cwd, global) {
        debug!(cwd = %cwd.display(), "project hooks skipped because project is not trusted");
        return None;
    }
    let path = cwd.join(".crown").join("hooks.json");
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<HookConfigFile>(&text).ok()
}

fn project_is_trusted(cwd: &Path, global: &HookConfigFile) -> bool {
    let cwd = normalize_path(cwd);
    global
        .trusted_projects
        .iter()
        .map(PathBuf::from)
        .map(|p| normalize_path(&p))
        .any(|p| p == cwd)
}

fn normalize_path(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase()
}

fn matcher_matches(matcher: Option<&str>, query: Option<&str>) -> bool {
    let matcher = matcher.unwrap_or("").trim();
    if matcher.is_empty() || matcher == "*" {
        return true;
    }
    let Some(query) = query else {
        return false;
    };
    matcher.eq_ignore_ascii_case(query)
}

async fn run_command_hook(
    event: HookEvent,
    matched: &MatchedHook,
    input: Value,
    cwd: Option<&Path>,
    abort: &CancellationToken,
) -> HookTraceEntry {
    let start = Instant::now();
    let command_text = matched.hook.command.clone();
    let timeout_secs = matched
        .hook
        .timeout
        .unwrap_or(DEFAULT_HOOK_TIMEOUT_SECS)
        .max(1);
    let input_text = serde_json::to_vec(&input).unwrap_or_default();
    let shell = matched.hook.shell.as_deref();

    let mut command = shell_command(shell, &command_text);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .env("CROWN_HOOK_EVENT", event.as_str());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            return HookTraceEntry {
                event: event.as_str().to_string(),
                hook_id: matched.hook.id.clone(),
                source: matched.source,
                matcher: matched.matcher.clone(),
                command: command_text,
                outcome: "error".into(),
                duration_ms: start.elapsed().as_millis() as u64,
                exit_code: None,
                blocking_reason: None,
                stdout: String::new(),
                stderr: err.to_string(),
            };
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&input_text).await;
    }
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let stdout_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(out) = stdout.as_mut() {
            let _ = out.read_to_end(&mut buf).await;
        }
        buf
    });
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(err) = stderr.as_mut() {
            let _ = err.read_to_end(&mut buf).await;
        }
        buf
    });

    let status = tokio::select! {
        _ = abort.cancelled() => {
            let _ = child.kill().await;
            None
        }
        result = timeout(Duration::from_secs(timeout_secs), child.wait()) => {
            match result {
                Ok(Ok(status)) => Some(status),
                Ok(Err(err)) => {
                    return HookTraceEntry {
                        event: event.as_str().to_string(),
                        hook_id: matched.hook.id.clone(),
                        source: matched.source,
                        matcher: matched.matcher.clone(),
                        command: command_text,
                        outcome: "error".into(),
                        duration_ms: start.elapsed().as_millis() as u64,
                        exit_code: None,
                        blocking_reason: None,
                        stdout: String::new(),
                        stderr: err.to_string(),
                    };
                }
                Err(_) => None,
            }
        }
    };

    let Some(status) = status else {
        let _ = child.kill().await;
        return HookTraceEntry {
            event: event.as_str().to_string(),
            hook_id: matched.hook.id.clone(),
            source: matched.source,
            matcher: matched.matcher.clone(),
            command: command_text,
            outcome: "timeout_or_aborted".into(),
            duration_ms: start.elapsed().as_millis() as u64,
            exit_code: None,
            blocking_reason: None,
            stdout: String::new(),
            stderr: format!("hook timed out or aborted after {timeout_secs}s"),
        };
    };

    let stdout = stdout_task.await.unwrap_or_default();
    let stderr = stderr_task.await.unwrap_or_default();
    let exit_code = status.code();
    let stdout = String::from_utf8_lossy(&stdout).to_string();
    let stderr = String::from_utf8_lossy(&stderr).to_string();
    let parsed = parse_hook_stdout(&stdout);
    let blocking_reason = hook_blocking_reason(exit_code, parsed.as_ref(), &stderr);
    HookTraceEntry {
        event: event.as_str().to_string(),
        hook_id: matched.hook.id.clone(),
        source: matched.source,
        matcher: matched.matcher.clone(),
        command: command_text,
        outcome: if blocking_reason.is_some() {
            "blocking".into()
        } else if exit_code == Some(0) {
            "success".into()
        } else {
            "non_blocking_error".into()
        },
        duration_ms: start.elapsed().as_millis() as u64,
        exit_code,
        blocking_reason,
        stdout,
        stderr,
    }
}

fn shell_command(shell: Option<&str>, command_text: &str) -> Command {
    let shell = shell.unwrap_or(default_shell());
    let mut cmd = Command::new(shell);
    match shell.to_ascii_lowercase().as_str() {
        "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe" => {
            cmd.arg("-NoProfile")
                .arg("-NonInteractive")
                .arg("-Command")
                .arg(command_text);
        }
        "cmd" | "cmd.exe" => {
            cmd.arg("/C").arg(command_text);
        }
        _ => {
            cmd.arg("-lc").arg(command_text);
        }
    }
    cmd
}

fn default_shell() -> &'static str {
    if cfg!(windows) {
        "powershell"
    } else {
        "sh"
    }
}

fn parse_hook_stdout(stdout: &str) -> Option<Value> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() || !trimmed.starts_with('{') {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

fn hook_blocking_reason(
    exit_code: Option<i32>,
    parsed: Option<&Value>,
    stderr: &str,
) -> Option<String> {
    if exit_code == Some(2) {
        return Some(if stderr.trim().is_empty() {
            "blocked by hook".into()
        } else {
            stderr.trim().to_string()
        });
    }
    if parsed
        .and_then(|v| v.get("continue"))
        .and_then(Value::as_bool)
        == Some(false)
    {
        return parsed
            .and_then(|v| v.get("stopReason").or_else(|| v.get("reason")))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| Some("blocked by hook".into()));
    }
    if parsed
        .and_then(|v| v.get("decision"))
        .and_then(Value::as_str)
        == Some("block")
    {
        return parsed
            .and_then(|v| v.get("reason"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| Some("blocked by hook".into()));
    }
    None
}

fn apply_hook_output(
    event: HookEvent,
    trace: &HookTraceEntry,
    aggregate: &mut AggregatedHookResult,
) {
    if let Some(reason) = &trace.blocking_reason {
        aggregate.blocking_error = Some(reason.clone());
    }
    let parsed = parse_hook_stdout(&trace.stdout);
    let Some(parsed) = parsed else {
        return;
    };
    if let Some(ctx) = parsed
        .get("hookSpecificOutput")
        .and_then(|v| v.get("additionalContext"))
        .and_then(Value::as_str)
    {
        aggregate.additional_contexts.push(ctx.to_string());
    }
    if let Some(ctx) = parsed.get("additionalContext").and_then(Value::as_str) {
        aggregate.additional_contexts.push(ctx.to_string());
    }
    if event == HookEvent::PreToolUse {
        let specific = parsed.get("hookSpecificOutput");
        let decision = specific
            .and_then(|v| v.get("permissionDecision"))
            .or_else(|| parsed.get("permissionDecision"))
            .and_then(Value::as_str);
        aggregate.permission_behavior = match decision {
            Some("allow") => Some(HookPermissionBehavior::Allow),
            Some("deny") => Some(HookPermissionBehavior::Deny),
            Some("ask") => Some(HookPermissionBehavior::Ask),
            _ => aggregate.permission_behavior,
        };
        if let Some(reason) = specific
            .and_then(|v| v.get("permissionDecisionReason"))
            .or_else(|| parsed.get("permissionDecisionReason"))
            .or_else(|| parsed.get("reason"))
            .and_then(Value::as_str)
        {
            aggregate.permission_decision_reason = Some(reason.to_string());
        }
        if let Some(updated) = specific
            .and_then(|v| v.get("updatedInput"))
            .or_else(|| parsed.get("updatedInput"))
        {
            aggregate.updated_input = Some(updated.clone());
        }
    }
}

pub fn pre_tool_input(
    session_id: &str,
    thread_id: &str,
    cwd: Option<&Path>,
    permission_mode: &str,
    tool_name: &str,
    tool_input: &Value,
) -> Value {
    json!({
        "session_id": session_id,
        "thread_id": thread_id,
        "cwd": cwd.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        "permission_mode": permission_mode,
        "hook_event_name": "PreToolUse",
        "tool_name": tool_name,
        "tool_input": tool_input,
    })
}

pub fn post_tool_input(
    event: HookEvent,
    session_id: &str,
    thread_id: &str,
    cwd: Option<&Path>,
    permission_mode: &str,
    tool_name: &str,
    tool_input: &Value,
    tool_response: &str,
) -> Value {
    json!({
        "session_id": session_id,
        "thread_id": thread_id,
        "cwd": cwd.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        "permission_mode": permission_mode,
        "hook_event_name": event.as_str(),
        "tool_name": tool_name,
        "tool_input": tool_input,
        "tool_response": tool_response,
    })
}

pub fn user_prompt_input(
    session_id: &str,
    thread_id: &str,
    cwd: Option<&Path>,
    permission_mode: &str,
    prompt: &str,
) -> Value {
    json!({
        "session_id": session_id,
        "thread_id": thread_id,
        "cwd": cwd.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        "permission_mode": permission_mode,
        "hook_event_name": "UserPromptSubmit",
        "prompt": prompt,
    })
}

pub fn stop_input(
    session_id: &str,
    thread_id: &str,
    cwd: Option<&Path>,
    permission_mode: &str,
) -> Value {
    json!({
        "session_id": session_id,
        "thread_id": thread_id,
        "cwd": cwd.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        "permission_mode": permission_mode,
        "hook_event_name": "Stop",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matcher_empty_matches_all() {
        assert!(matcher_matches(None, Some("run_command")));
        assert!(matcher_matches(Some("*"), Some("read_file")));
        assert!(matcher_matches(Some("run_command"), Some("RUN_COMMAND")));
        assert!(!matcher_matches(Some("read_file"), Some("run_command")));
    }

    #[test]
    fn exit_code_two_blocks() {
        let reason = hook_blocking_reason(Some(2), None, "no");
        assert_eq!(reason.as_deref(), Some("no"));
    }

    #[test]
    fn json_continue_false_blocks() {
        let parsed = json!({"continue": false, "stopReason": "stop"});
        let reason = hook_blocking_reason(Some(0), Some(&parsed), "");
        assert_eq!(reason.as_deref(), Some("stop"));
    }

    #[test]
    fn event_from_name_is_case_insensitive() {
        assert_eq!(
            HookEvent::from_name("pretooluse"),
            Some(HookEvent::PreToolUse)
        );
        assert_eq!(
            HookEvent::from_name("PermissionRequest"),
            Some(HookEvent::PermissionRequest)
        );
        assert_eq!(HookEvent::from_name("missing"), None);
    }

    #[test]
    fn project_trust_uses_normalized_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let trusted = tmp.path().to_string_lossy().replace('/', "\\");
        let config = HookConfigFile {
            trusted_projects: vec![trusted],
            ..Default::default()
        };
        assert!(project_is_trusted(tmp.path(), &config));
    }

    #[tokio::test]
    async fn single_test_hook_runs_command() {
        let hook = HookCommandConfig {
            id: Some("test".into()),
            r#type: HookCommandType::Command,
            command: if cfg!(windows) {
                "Write-Output '{\"continue\":true}'".into()
            } else {
                "printf '{\"continue\":true}'".into()
            },
            shell: None,
            timeout: Some(5),
            enabled: true,
        };
        let abort = CancellationToken::new();
        let trace =
            run_single_hook_for_test(HookEvent::PreToolUse, hook, json!({}), None, None, &abort)
                .await;
        assert_eq!(trace.outcome, "success");
        assert_eq!(trace.exit_code, Some(0));
        assert!(trace.stdout.contains("continue"));
    }
}
