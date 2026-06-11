use std::path::{Path, PathBuf};

use deepseek_core::hooks::{self, HookCommandConfig, HookConfigFile, HookEvent, HookTraceEntry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HooksScopeInput {
    pub scope: String,
    #[serde(default)]
    pub project_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveHooksConfigInput {
    pub scope: String,
    #[serde(default)]
    pub project_path: Option<String>,
    pub config: HookConfigFile,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestHookInput {
    pub event: String,
    #[serde(default)]
    pub matcher: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    pub hook: HookCommandConfig,
    #[serde(default)]
    pub input: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookEventInfoDto {
    pub id: String,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectHooksTrustDto {
    pub trusted: bool,
}

fn config_file_path() -> Result<PathBuf, String> {
    dirs::config_dir()
        .map(|dir| dir.join("crown").join("config.json"))
        .ok_or_else(|| "无法解析用户配置目录".to_string())
}

fn global_json() -> Value {
    let Ok(path) = config_file_path() else {
        return serde_json::json!({});
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return serde_json::json!({});
    };
    serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({}))
}

fn write_global_json(json: &Value) -> Result<(), String> {
    let path = config_file_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(json).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

fn global_hooks_config() -> HookConfigFile {
    serde_json::from_value(global_json()).unwrap_or_default()
}

fn write_global_hooks_config(config: &HookConfigFile) -> Result<(), String> {
    let mut json = global_json();
    json["disableAllHooks"] = Value::Bool(config.disable_all_hooks);
    json["trustedProjects"] =
        serde_json::to_value(&config.trusted_projects).map_err(|e| e.to_string())?;
    json["hooks"] = serde_json::to_value(&config.hooks).map_err(|e| e.to_string())?;
    write_global_json(&json)
}

fn project_hooks_path(project_path: &str) -> Result<PathBuf, String> {
    let trimmed = project_path.trim();
    if trimmed.is_empty() {
        return Err("项目路径为空".into());
    }
    Ok(PathBuf::from(trimmed).join(".crown").join("hooks.json"))
}

fn read_project_hooks_config(project_path: &str) -> Result<HookConfigFile, String> {
    let path = project_hooks_path(project_path)?;
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ok(HookConfigFile::default());
    };
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn write_project_hooks_config(project_path: &str, config: &HookConfigFile) -> Result<(), String> {
    let path = project_hooks_path(project_path)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

fn normalize_path(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase()
}

fn project_trusted(project_path: &str, config: &HookConfigFile) -> bool {
    let project = normalize_path(Path::new(project_path));
    config
        .trusted_projects
        .iter()
        .any(|p| normalize_path(Path::new(p)) == project)
}

#[tauri::command]
pub async fn list_hook_events() -> Result<Vec<HookEventInfoDto>, String> {
    Ok(HookEvent::all()
        .iter()
        .copied()
        .map(|event| HookEventInfoDto {
            id: event.as_str().to_string(),
            label: event.as_str().to_string(),
            description: event_description(event).to_string(),
        })
        .collect())
}

#[tauri::command]
pub async fn get_hooks_config(input: HooksScopeInput) -> Result<HookConfigFile, String> {
    match input.scope.as_str() {
        "global" => Ok(global_hooks_config()),
        "project" => {
            let project_path = input.project_path.ok_or("缺少项目路径")?;
            read_project_hooks_config(&project_path)
        }
        other => Err(format!("未知 hook 作用域: {other}")),
    }
}

#[tauri::command]
pub async fn save_hooks_config(input: SaveHooksConfigInput) -> Result<HookConfigFile, String> {
    let scope = input.scope.clone();
    let saved = match scope.as_str() {
        "global" => {
            write_global_hooks_config(&input.config)?;
            Ok(global_hooks_config())
        }
        "project" => {
            let project_path = input.project_path.ok_or("缺少项目路径")?;
            write_project_hooks_config(&project_path, &input.config)?;
            read_project_hooks_config(&project_path)
        }
        other => Err(format!("未知 hook 作用域: {other}")),
    }?;
    run_config_change_hook("hooks", scope.as_str(), None).await;
    Ok(saved)
}

#[tauri::command]
pub async fn test_hook(input: TestHookInput) -> Result<HookTraceEntry, String> {
    let event = HookEvent::from_name(&input.event)
        .ok_or_else(|| format!("未知 hook 事件: {}", input.event))?;
    let cwd = input.cwd.as_deref().map(Path::new);
    let abort = CancellationToken::new();
    Ok(hooks::run_single_hook_for_test(
        event,
        input.hook,
        if input.input.is_null() {
            serde_json::json!({})
        } else {
            input.input
        },
        input.matcher,
        cwd,
        &abort,
    )
    .await)
}

#[tauri::command]
pub async fn get_project_hooks_trust(project_path: String) -> Result<ProjectHooksTrustDto, String> {
    Ok(ProjectHooksTrustDto {
        trusted: project_trusted(&project_path, &global_hooks_config()),
    })
}

#[tauri::command]
pub async fn set_project_hooks_trust(
    project_path: String,
    trusted: bool,
) -> Result<ProjectHooksTrustDto, String> {
    let mut config = global_hooks_config();
    let normalized = normalize_path(Path::new(&project_path));
    config
        .trusted_projects
        .retain(|p| normalize_path(Path::new(p)) != normalized);
    if trusted {
        config.trusted_projects.push(project_path);
    }
    write_global_hooks_config(&config)?;
    run_config_change_hook("trustedProjects", "global", Some(trusted.to_string())).await;
    Ok(ProjectHooksTrustDto { trusted })
}

/// Read the global memory file (AGENTS.md). Returns empty string if not found.
#[tauri::command]
pub async fn read_global_memory(state: tauri::State<'_, crate::AppState>) -> Result<String, String> {
    let path = state.data_root.join("AGENTS.md");
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(content),
        Err(_) => Ok(String::new()),
    }
}

async fn run_config_change_hook(key: &str, scope: &str, value: Option<String>) {
    let abort = CancellationToken::new();
    let result = hooks::HookRunner::load(None)
        .run(
            HookEvent::ConfigChange,
            serde_json::json!({
                "session_id": "app",
                "thread_id": "",
                "cwd": "",
                "permission_mode": "default",
                "hook_event_name": "ConfigChange",
                "key": key,
                "scope": scope,
                "value": value,
            }),
            Some(key),
            None,
            &abort,
        )
        .await;
    for trace in &result.traces {
        tracing::debug!(
            event = %trace.event,
            hook_id = ?trace.hook_id,
            source = ?trace.source,
            outcome = %trace.outcome,
            duration_ms = trace.duration_ms,
            "hook trace"
        );
    }
}

fn event_description(event: HookEvent) -> &'static str {
    match event {
        HookEvent::PreToolUse => "工具执行前，可允许、拒绝、要求询问或改写输入",
        HookEvent::PostToolUse => "工具成功执行后，可记录日志或追加上下文",
        HookEvent::PostToolUseFailure => "工具失败后，可记录错误或阻断后续流程",
        HookEvent::UserPromptSubmit => "用户提示词提交后、进入模型前",
        HookEvent::Stop => "模型准备结束本轮时",
        HookEvent::StopFailure => "Stop hook 阻断后仍无法继续时",
        HookEvent::SessionStart => "会话启动时",
        HookEvent::SessionEnd => "会话结束时",
        HookEvent::SubagentStart => "子代理启动时",
        HookEvent::SubagentStop => "子代理结束时",
        HookEvent::Notification => "系统通知时",
        HookEvent::PreCompact => "上下文压缩前",
        HookEvent::PostCompact => "上下文压缩后",
        HookEvent::PermissionDenied => "权限被拒绝时",
        HookEvent::PermissionRequest => "权限请求出现时",
        HookEvent::CwdChanged => "工作目录变化时",
        HookEvent::FileChanged => "文件变化时",
        HookEvent::InstructionsLoaded => "项目指令加载后",
        HookEvent::TaskCreated => "任务创建时",
        HookEvent::TaskCompleted => "任务完成时",
        HookEvent::ConfigChange => "配置变化时",
        HookEvent::Setup => "初始化设置时",
    }
}
