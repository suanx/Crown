#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod dto;
mod events;
mod gate_impl;
mod question_gate_impl;
mod subagent;
mod webview_memory;

use std::sync::Arc;

use deepseek_client::deepseek::{DeepSeekClient, DeepSeekClientConfig};
use deepseek_core::engine::AgentEngine;
use deepseek_state::Database;
use deepseek_tools::{specs::register_default_tools, web::WebToolsState, ToolRegistry};
use tauri::Manager;

use crate::gate_impl::TauriPermissionGate;
use crate::question_gate_impl::TauriQuestionGate;

struct ConfigProviderClientResolver;

impl deepseek_core::engine::ProviderClientResolver for ConfigProviderClientResolver {
    fn client_for(&self, provider_id: &str) -> Option<DeepSeekClient> {
        commands::config::client_for_provider_id(provider_id)
    }
}

async fn run_lifecycle_hook(event: deepseek_core::hooks::HookEvent, data_root: std::path::PathBuf) {
    let abort = tokio_util::sync::CancellationToken::new();
    let result = deepseek_core::hooks::HookRunner::load(None)
        .run(
            event,
            serde_json::json!({
                "session_id": "app",
                "thread_id": "",
                "cwd": "",
                "permission_mode": "default",
                "hook_event_name": event.as_str(),
                "data_root": data_root.to_string_lossy().to_string(),
            }),
            None,
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

fn spawn_lifecycle_hook(event: deepseek_core::hooks::HookEvent, data_root: std::path::PathBuf) {
    tauri::async_runtime::spawn(run_lifecycle_hook(event, data_root));
}

/// Application state shared across Tauri commands.
///
/// `engine` is multi-thread aware (P4) — it owns the [`ThreadCache`]
/// internally and resolves threads by id on each `send_message`. `gate` is
/// the same instance the engine uses for approval prompts; commands also
/// hold a reference so `approve_tool` can deliver the user's decision.
/// `db` is exposed for commands that touch threads/messages directly
/// (list / delete / search) without going through the engine.
/// `session_start_ms` is the epoch-ms baseline used by
/// [`crate::dto::UsageStatsWindow::Session`] — it captures process
/// startup, never changes mid-run.
pub struct AppState {
    /// Multi-thread agent engine.
    pub engine: Arc<AgentEngine>,
    /// Tauri-backed approval gate (same instance the engine ask()s into).
    pub gate: Arc<TauriPermissionGate>,
    /// Tauri-backed structured-question gate (same instance the engine's
    /// `ask_user_question` tool ask()s into). `submit_answers` delivers here.
    pub question_gate: Arc<TauriQuestionGate>,
    /// SQLite-backed thread / message / checkpoint store.
    pub db: Arc<Database>,
    /// MCP server connection manager (tools bridged into the registry).
    pub mcp: Arc<deepseek_mcp::manager::McpManager>,
    /// Shared tool registry (built-ins + dynamic MCP tools).
    pub tools: Arc<ToolRegistry>,
    /// Shared web tools state; settings can update web_search provider at runtime.
    pub web_state: Arc<WebToolsState>,
    /// Process-start epoch ms — drives the `Session` usage window.
    pub session_start_ms: i64,
    /// Crown data root (`%APPDATA%\crown` etc.) — for commands that read/write
    /// user files (output-styles, etc.) directly.
    pub data_root: std::path::PathBuf,
    /// Per-thread prompt augmentation (Phase 2). Commands toggle the active
    /// output-style through it.
    pub prompt_augment: Arc<deepseek_core::memory::PromptAugment>,
    /// 交互式终端 PTY 会话管理器。
    pub pty: Arc<commands::pty::PtyManager>,
    /// 正在运行的多 Agent 头脑风暴任务，用于 stop_brainstorm 中止。
    pub brainstorm_runs: Arc<dashmap::DashMap<String, tokio_util::sync::CancellationToken>>,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "deepseek=debug,info".into()))
        .init();

    let api_key = std::env::var("DEEPSEEK_API_KEY").unwrap_or_default();
    // Fallback: read from config file if env var is not set
    let api_key = if api_key.is_empty() {
        commands::config::read_stored_api_key_pub().unwrap_or_default()
    } else {
        api_key
    };
    let base_url = std::env::var("DEEPSEEK_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(commands::config::read_stored_base_url_pub)
        .unwrap_or_else(|| "https://api.deepseek.com".to_string());

    let client = if api_key.is_empty() {
        tracing::warn!("DEEPSEEK_API_KEY is not set; API calls will fail.");
        DeepSeekClient::new(DeepSeekClientConfig {
            api_key: "placeholder".into(),
            base_url: base_url.clone(),
            ..Default::default()
        })
        .expect("failed to create DeepSeek client")
    } else {
        DeepSeekClient::new(DeepSeekClientConfig {
            api_key,
            base_url: base_url.clone(),
            ..Default::default()
        })
        .expect("failed to create DeepSeek client")
    };

    let registry = ToolRegistry::new();
    let web_config = commands::config::read_web_config_pub();
    let web_state = Arc::new(WebToolsState::new(web_config));
    register_default_tools(&registry, web_state.clone());
    let tools = Arc::new(registry);

    // MCP manager — loads mcp.json, connects servers, bridges their tools
    // into the shared registry. Created here so it can be shared with both
    // the engine (via the registry) and the IPC commands.
    let mcp = Arc::new(deepseek_mcp::manager::McpManager::new());
    // Register the self-install tools (mcp_install / mcp_reload) with the
    // live manager captured, so the agent can YOLO-install servers.
    deepseek_mcp::install_tools::register_install_tools(&tools, &mcp);

    tauri::Builder::default()
        .setup(move |app| {
            // Resolve the single data root via Tauri's app_data_dir
            // (= %APPDATA%\crown on Windows). All persisted files derive from
            // this one root via CrownPaths — no scattered dirs:: joins.
            let data_root = app
                .path()
                .app_data_dir()
                .expect("resolve app_data_dir");
            let crown_paths = deepseek_core::paths::CrownPaths::with_root(data_root);
            crown_paths
                .ensure_data_root()
                .expect("create crown data root");

            let db = Arc::new(
                Database::open(crown_paths.db_path()).expect("open state.db"),
            );

            // System prompt base (7 sections + install guidance) WITHOUT the
            // environment block — the per-thread prompt composer (PromptAugment)
            // appends a fresh environment block plus AGENTS.md memory / rules /
            // output-style. Paths point under the crown data root so the agent
            // knows where to write MCP/Skill config.
            let mcp_path = crown_paths.mcp_config().to_string_lossy().into_owned();
            let skills_dir = crown_paths.skills_dir().to_string_lossy().into_owned();
            let system_prompt = deepseek_core::prompt::build_system_prompt_base(
                Some(&mcp_path),
                Some(&skills_dir),
            );

            // Phase 2: prompt augmentation rooted at the crown data root
            // (global AGENTS.md / rules / output-styles). Initial active
            // output-style read from settings (config.json), if any.
            let prompt_augment = Arc::new(deepseek_core::memory::PromptAugment::new(
                crown_paths.data_root().to_path_buf(),
            ));
            if let Some(style) = commands::config::read_active_output_style() {
                prompt_augment.set_output_style(Some(style));
            }

            let gate = Arc::new(TauriPermissionGate::new(app.handle().clone()));
            // 结构化问答 gate（EPIC 1）—— 与权限 gate 独立的实例。
            let question_gate = Arc::new(TauriQuestionGate::new(app.handle().clone()));
            // Engine takes the gate as `Arc<dyn PermissionGate>`; cloning the
            // concrete `Arc<TauriPermissionGate>` upcasts cleanly because
            // `TauriPermissionGate: PermissionGate`.
            let gate_for_engine: Arc<dyn deepseek_core::gate::PermissionGate> = gate.clone();
            let engine = Arc::new(AgentEngine::new(
                client.clone(),
                system_prompt.clone(),
                tools.clone(),
                gate_for_engine,
                db.clone(),
            ));
            engine.set_prompt_augment(prompt_augment.clone());
            engine.set_provider_client_resolver(Arc::new(ConfigProviderClientResolver));

            // EPIC 1: 注入结构化问答 gate（`ask_user_question` 工具委托此处阻塞）。
            {
                let qg_for_engine: Arc<dyn deepseek_tools::QuestionGate> = question_gate.clone();
                engine.set_question_gate(qg_for_engine);
            }

            // P4: inject the sub-agent launcher (needs the AppHandle for
            // event dispatch). The `task` tool delegates here.
            {
                let gate_for_sub: Arc<dyn deepseek_core::gate::PermissionGate> = gate.clone();
                let launcher = Arc::new(crate::subagent::AppSubagentLauncher::new(
                    app.handle().clone(),
                    client.clone(),
                    gate_for_sub,
                    db.clone(),
                    tools.clone(),
                ));
                engine.set_subagent_launcher(launcher);
            }

            app.manage(AppState {
                engine,
                gate,
                question_gate: question_gate.clone(),
                db: db.clone(),
                mcp: mcp.clone(),
                tools: tools.clone(),
                web_state: web_state.clone(),
                // Captured once at process start; UsageStatsWindow::Session
                // resolves to this value forever for the run.
                session_start_ms: chrono::Utc::now().timestamp_millis(),
                data_root: crown_paths.data_root().to_path_buf(),
                prompt_augment: prompt_augment.clone(),
                pty: Arc::new(commands::pty::PtyManager::new()),
                brainstorm_runs: Arc::new(dashmap::DashMap::new()),
            });

            spawn_lifecycle_hook(deepseek_core::hooks::HookEvent::Setup, crown_paths.data_root().to_path_buf());
            spawn_lifecycle_hook(
                deepseek_core::hooks::HookEvent::SessionStart,
                crown_paths.data_root().to_path_buf(),
            );

            // Start MCP: load config, connect servers, keep the registry in
            // sync as servers connect/disconnect, and forward status to the UI.
            {
                let mcp = mcp.clone();
                let tools = tools.clone();
                let app_handle = app.handle().clone();
                let mcp_config_path = crown_paths.mcp_config();
                tauri::async_runtime::spawn(async move {
                    // Forward manager events to Tauri + re-sync registry tools.
                    let mut rx = mcp.subscribe();
                    {
                        let mcp_ev = mcp.clone();
                        let tools_ev = tools.clone();
                        let app_ev = app_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            use deepseek_mcp::manager::McpEvent;
                            while let Ok(ev) = rx.recv().await {
                                match ev {
                                    McpEvent::StatusChanged { name, status, error } => {
                                        crate::events::dispatch_mcp_status(&app_ev, &name, status, error);
                                    }
                                    McpEvent::ToolsChanged => {
                                        deepseek_mcp::sync_registry_tools(&tools_ev, &mcp_ev);
                                        crate::events::dispatch_mcp_tools_changed(&app_ev);
                                    }
                                }
                            }
                        });
                    }
                    let cfg = deepseek_mcp::config::McpConfig::load_trusted(&mcp_config_path);
                    mcp.set_config_path(mcp_config_path.clone());
                    mcp.load_from_config(cfg).await;
                });
            }

            // Pre-warm the BPE tokenizer off the main thread so the first
            // user turn's context-size estimate (pre-fold hook) doesn't pay
            // the ~100ms gunzip+parse init cost inline.
            std::thread::spawn(|| {
                deepseek_core::warmup_tokenizer();
            });

            // Auto-open devtools in dev so we always have a console even if
            // the webview gets into a state where shortcuts (F12 / Ctrl+Shift+I)
            // can no longer reach it.
            #[cfg(debug_assertions)]
            {
                if let Some(window) = app.get_webview_window("main") {
                    window.open_devtools();
                }
            }

            // PERF-1: lower the WebView2 memory usage target when the window is
            // unfocused, restore it on focus. Best-effort — failures are logged
            // and ignored so a memory hint can never break the app. No-op on
            // non-Windows platforms (see webview_memory module).
            if let Some(window) = app.get_webview_window("main") {
                let win_for_events = window.clone();
                let data_root_for_close = crown_paths.data_root().to_path_buf();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(focused) = event {
                        let level = if *focused {
                            crate::webview_memory::MemoryLevel::Normal
                        } else {
                            crate::webview_memory::MemoryLevel::Low
                        };
                        if let Err(e) =
                            crate::webview_memory::set_memory_level(&win_for_events, level)
                        {
                            tracing::debug!(error = %e, "webview memory level set failed (ignored)");
                        }
                    } else if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                        spawn_lifecycle_hook(
                            deepseek_core::hooks::HookEvent::SessionEnd,
                            data_root_for_close.clone(),
                        );
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // threads
            commands::list_threads,
            commands::get_thread,
            commands::create_thread,
            commands::update_thread,
            commands::delete_thread,
            commands::search_threads,
            commands::export_thread,
            // projects
            commands::list_projects,
            commands::pick_project_directory,
            commands::create_project,
            commands::update_project,
            commands::delete_project,
            // messages
            commands::send_message,
            commands::abort_turn,
            commands::start_brainstorm,
            commands::continue_brainstorm,
            commands::stop_brainstorm,
            // permissions
            commands::approve_tool,
            commands::list_permission_rules,
            commands::remove_permission_rule,
            commands::get_permission_context,
            commands::cycle_permission_mode,
            commands::submit_answers,
            // models
            commands::list_models,
            commands::switch_model,
            // config
            commands::get_config,
            commands::set_config,
            commands::save_providers,
            commands::save_web_search_config,
            commands::fetch_provider_models,
            commands::test_provider_connection,
            // hooks
            commands::list_hook_events,
            commands::get_hooks_config,
            commands::save_hooks_config,
            commands::test_hook,
            commands::get_project_hooks_trust,
            commands::set_project_hooks_trust,
            // mcp
            commands::list_mcp_servers,
            commands::restart_mcp_server,
            commands::toggle_mcp_server,
            commands::mcp_add_server,
            commands::mcp_remove_server,
            commands::mcp_reload,
            commands::list_mcp_tools,
            // skill
            commands::skill_list,
            commands::skill_read,
            commands::skill_reload,
            // output styles
            commands::list_output_styles,
            commands::read_output_style,
            commands::save_output_style,
            commands::set_active_output_style,
            commands::delete_output_style,
            // rewind
            commands::rewind_thread,
            commands::list_rewind_points,
            // stats
            commands::get_usage_stats,
            commands::export_diagnostics,
            commands::get_usage_chart,

            // balance
            commands::get_user_balance,
            // filesystem
            commands::fs_get_workspace_root,
            commands::fs_list_directory,
            commands::fs_read_file,
            // 终端 PTY
            commands::pty_list,
            commands::pty_snapshot,
            commands::pty_spawn,
            commands::pty_write,
            commands::pty_resize,
            commands::pty_kill,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
