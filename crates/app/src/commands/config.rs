//! get_config + set_config commands.
//!
//! Reads/writes config from a JSON file at app_config_dir/config.json.
//! Falls back to environment variables if no config file exists.

use deepseek_client::deepseek::{DeepSeekClient, DeepSeekClientConfig};
use deepseek_core::permission::PermissionMode;
use deepseek_tools::web::config::WebConfig;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Instant;

use crate::dto::{
    AppConfigDto, BudgetDto, CompactionDto, ConfigPatchDto, ProviderConfigDto, ProviderModelDto,
    ProviderModelsInput, ProviderTestResultDto, SaveProvidersInput, SaveWebSearchConfigInput,
    ShellDto, WebSearchConfigDto, WebSearchProviderDto,
};
use crate::AppState;

/// Get the config file path: `<config_dir>/crown/config.json`
/// (= `%APPDATA%\crown\config.json` on Windows). Shares the single crown data
/// root with all other persisted files.
fn config_file_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| config_path_in_root(&d.join("crown")))
}

/// Resolve the config.json path under an explicit data root.
fn config_path_in_root(root: &std::path::Path) -> PathBuf {
    root.join("config.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredProviderConfig {
    id: String,
    name: String,
    provider_type: String,
    base_url: String,
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    models: Vec<ProviderModelDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredWebSearchProvider {
    id: String,
    name: String,
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    implemented: bool,
    #[serde(default)]
    key_required: bool,
    #[serde(default)]
    note: Option<String>,
}

fn default_true() -> bool {
    true
}

fn web_search_provider_templates() -> Vec<StoredWebSearchProvider> {
    vec![
        StoredWebSearchProvider {
            id: "jina".into(),
            name: "Jina Search".into(),
            api_key: None,
            enabled: true,
            implemented: true,
            key_required: false,
            note: Some("无 key 可用；填入 Jina API key 后使用结构化搜索。".into()),
        },
        StoredWebSearchProvider {
            id: "duckduckgo".into(),
            name: "DuckDuckGo HTML".into(),
            api_key: None,
            enabled: true,
            implemented: true,
            key_required: false,
            note: Some("零配置 fallback，开箱可用。".into()),
        },
        StoredWebSearchProvider {
            id: "bocha".into(),
            name: "Bocha AI Search".into(),
            api_key: None,
            enabled: false,
            implemented: true,
            key_required: true,
            note: Some("后端已接入，需要 API key。".into()),
        },
        StoredWebSearchProvider {
            id: "brave".into(),
            name: "Brave Search API".into(),
            api_key: None,
            enabled: false,
            implemented: true,
            key_required: true,
            note: Some("后端已接入，需要 API key。".into()),
        },
        StoredWebSearchProvider {
            id: "tavily".into(),
            name: "Tavily".into(),
            api_key: None,
            enabled: false,
            implemented: true,
            key_required: true,
            note: Some("后端已接入，需要 API key。".into()),
        },
        StoredWebSearchProvider {
            id: "exa".into(),
            name: "Exa".into(),
            api_key: None,
            enabled: false,
            implemented: true,
            key_required: true,
            note: Some("后端已接入，需要 API key。".into()),
        },
        StoredWebSearchProvider {
            id: "serper".into(),
            name: "Serper".into(),
            api_key: None,
            enabled: false,
            implemented: true,
            key_required: true,
            note: Some("后端已接入，需要 API key。".into()),
        },
        StoredWebSearchProvider {
            id: "serpapi".into(),
            name: "SerpAPI".into(),
            api_key: None,
            enabled: false,
            implemented: true,
            key_required: true,
            note: Some("后端已接入，需要 API key。".into()),
        },
    ]
}

fn builtin_deepseek_provider(
    api_key: Option<String>,
    base_url: Option<String>,
) -> StoredProviderConfig {
    StoredProviderConfig {
        id: "deepseek".into(),
        name: "DeepSeek".into(),
        provider_type: "deepseek".into(),
        base_url: base_url.unwrap_or_else(|| "https://api.deepseek.com".into()),
        api_key,
        enabled: false,
        models: vec![
            ProviderModelDto {
                id: "deepseek-v4-flash".into(),
                label: "v4-flash".into(),
                enabled: true,
                supports_tools: true,
                supports_reasoning: true,
            },
            ProviderModelDto {
                id: "deepseek-v4-pro".into(),
                label: "v4-pro".into(),
                enabled: true,
                supports_tools: true,
                supports_reasoning: true,
            },
        ],
    }
}

fn builtin_provider_templates() -> Vec<StoredProviderConfig> {
    vec![
        // OpenCode CLI — 默认供应商，开箱即用
        StoredProviderConfig {
            id: "opencode".into(),
            name: "OpenCode CLI".into(),
            provider_type: "openai-compatible".into(),
            base_url: "https://opencode.ai/zen/v1".into(),
            api_key: None,
            enabled: true,
            models: vec![
                ProviderModelDto {
                    id: "deepseek-v4-flash".into(),
                    label: "DeepSeek V4 Flash".into(),
                    enabled: true,
                    supports_tools: true,
                    supports_reasoning: true,
                },
                ProviderModelDto {
                    id: "gpt-5.4-nano".into(),
                    label: "GPT 5.4 Nano".into(),
                    enabled: true,
                    supports_tools: true,
                    supports_reasoning: false,
                },
            ],
        },
        // 讯飞星辰 MaaS 平台 — OpenAI 兼容接口
        StoredProviderConfig {
            id: "xfyun".into(),
            name: "讯飞星辰 MaaS".into(),
            provider_type: "openai-compatible".into(),
            base_url: "https://maas-api.cn-huabei-1.xf-yun.com/v2".into(),
            api_key: None,
            enabled: false,
            models: vec![
                ProviderModelDto {
                    id: "xdeepseekv3".into(),
                    label: "DeepSeek V3".into(),
                    enabled: true,
                    supports_tools: false,
                    supports_reasoning: false,
                },
                ProviderModelDto {
                    id: "xdeepseekr1".into(),
                    label: "DeepSeek R1".into(),
                    enabled: true,
                    supports_tools: false,
                    supports_reasoning: true,
                },
                ProviderModelDto {
                    id: "xdeepseekv32".into(),
                    label: "DeepSeek V3.2".into(),
                    enabled: true,
                    supports_tools: true,
                    supports_reasoning: false,
                },
                ProviderModelDto {
                    id: "xglm4.7".into(),
                    label: "GLM-4.7".into(),
                    enabled: true,
                    supports_tools: true,
                    supports_reasoning: false,
                },
            ],
        },
        // OpenAI 通用接口 — 用户可自定义 Base URL，兼容任意 OpenAI 协议服务
        StoredProviderConfig {
            id: "openai-compatible".into(),
            name: "OpenAI 通用接口".into(),
            provider_type: "openai-compatible".into(),
            base_url: "".into(),
            api_key: None,
            enabled: false,
            models: Vec::new(),
        },



        builtin_deepseek_provider(None, None),
        StoredProviderConfig {
            id: "openai".into(),
            name: "OpenAI".into(),
            provider_type: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key: None,
            enabled: false,
            models: Vec::new(),
        },
        StoredProviderConfig {
            id: "anthropic".into(),
            name: "Anthropic".into(),
            provider_type: "anthropic".into(),
            base_url: "https://api.anthropic.com/v1".into(),
            api_key: None,
            enabled: false,
            models: Vec::new(),
        },
        StoredProviderConfig {
            id: "siliconflow".into(),
            name: "硅基流动".into(),
            provider_type: "openai-compatible".into(),
            base_url: "https://api.siliconflow.cn/v1".into(),
            api_key: None,
            enabled: false,
            models: Vec::new(),
        },
        StoredProviderConfig {
            id: "ollama".into(),
            name: "Ollama".into(),
            provider_type: "ollama".into(),
            base_url: "http://localhost:11434/v1".into(),
            api_key: Some("ollama".into()),
            enabled: false,
            models: Vec::new(),
        },
    ]
}


fn provider_to_dto(p: StoredProviderConfig, include_key: bool) -> ProviderConfigDto {
    let api_key_present = p.api_key.as_deref().is_some_and(|s| !s.is_empty());
    ProviderConfigDto {
        id: p.id,
        name: p.name,
        provider_type: p.provider_type,
        base_url: p.base_url,
        api_key: if include_key { p.api_key } else { None },
        api_key_present,
        enabled: p.enabled,
        models: p.models,
    }
}

fn dto_to_provider(p: &ProviderConfigDto, previous_key: Option<String>) -> StoredProviderConfig {
    StoredProviderConfig {
        id: p.id.clone(),
        name: p.name.clone(),
        provider_type: p.provider_type.clone(),
        base_url: p.base_url.clone(),
        api_key: p.api_key.clone().filter(|s| !s.is_empty()).or(previous_key),
        enabled: p.enabled,
        models: p.models.clone(),
    }
}

fn read_config_json() -> serde_json::Value {
    let Some(path) = config_file_path() else {
        return serde_json::json!({});
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return serde_json::json!({});
    };
    serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
}

fn write_config_json(json: &serde_json::Value) -> Result<(), String> {
    let path = config_file_path().ok_or("cannot resolve config dir")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        &path,
        serde_json::to_string_pretty(json).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    harden_file_permissions(&path);
    Ok(())
}

fn read_stored_providers() -> Vec<StoredProviderConfig> {
    let json = read_config_json();
    let legacy_key = json
        .get("apiKey")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let legacy_base = json
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let mut providers: Vec<StoredProviderConfig> = json
        .get("providers")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_else(|| vec![builtin_deepseek_provider(legacy_key, legacy_base)]);

    for template in builtin_provider_templates() {
        if !providers.iter().any(|p| p.id == template.id) {
            providers.push(template);
        }
    }
    providers
}

fn read_stored_web_search_providers() -> Vec<StoredWebSearchProvider> {
    let json = read_config_json();
    let stored_section = json.get("webSearch");
    let mut providers: Vec<StoredWebSearchProvider> = stored_section
        .and_then(|v| v.get("providers"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_else(web_search_provider_templates);

    let env_keys = [
        ("jina", "JINA_API_KEY"),
        ("bocha", "BOCHA_API_KEY"),
        ("brave", "BRAVE_API_KEY"),
        ("tavily", "TAVILY_API_KEY"),
    ];
    for (provider_id, env_key) in env_keys {
        if let Ok(key) = std::env::var(env_key) {
            if !key.is_empty() {
                if let Some(provider) = providers.iter_mut().find(|p| p.id == provider_id) {
                    if provider.api_key.as_deref().is_none_or(str::is_empty) {
                        provider.api_key = Some(key);
                    }
                }
            }
        }
    }

    for template in web_search_provider_templates() {
        if !providers.iter().any(|p| p.id == template.id) {
            providers.push(template);
        }
    }

    providers
}

fn default_web_search_provider_id_from_json(json: &serde_json::Value) -> String {
    json.get("webSearch")
        .and_then(|v| v.get("defaultProviderId"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::env::var("WEB_SEARCH_PROVIDER")
                .ok()
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "jina".into())
}

fn web_search_provider_to_dto(
    p: StoredWebSearchProvider,
    include_key: bool,
) -> WebSearchProviderDto {
    let api_key_present = p.api_key.as_deref().is_some_and(|s| !s.is_empty());
    WebSearchProviderDto {
        id: p.id,
        name: p.name,
        api_key: if include_key { p.api_key } else { None },
        api_key_present,
        enabled: p.enabled,
        implemented: p.implemented,
        key_required: p.key_required,
        note: p.note,
    }
}

fn dto_to_web_search_provider(
    p: &WebSearchProviderDto,
    previous_key: Option<String>,
) -> StoredWebSearchProvider {
    StoredWebSearchProvider {
        id: p.id.clone(),
        name: p.name.clone(),
        api_key: match p.api_key.clone() {
            Some(key) if !key.is_empty() => Some(key),
            Some(_) => None,
            None => previous_key,
        },
        enabled: p.enabled,
        implemented: p.implemented,
        key_required: p.key_required,
        note: p.note.clone(),
    }
}

fn read_web_search_config_dto(include_keys: bool) -> WebSearchConfigDto {
    let json = read_config_json();
    let providers = read_stored_web_search_providers();
    let implemented_default =
        normalize_web_search_default(default_web_search_provider_id_from_json(&json), &providers);
    WebSearchConfigDto {
        default_provider_id: implemented_default,
        providers: providers
            .into_iter()
            .map(|p| web_search_provider_to_dto(p, include_keys))
            .collect(),
    }
}

fn normalize_web_search_default(
    provider_id: String,
    providers: &[StoredWebSearchProvider],
) -> String {
    providers
        .iter()
        .find(|p| p.id == provider_id && p.implemented && p.enabled)
        .map(|p| p.id.clone())
        .or_else(|| {
            providers
                .iter()
                .find(|p| p.id == provider_id && p.implemented)
                .map(|p| p.id.clone())
        })
        .or_else(|| {
            providers
                .iter()
                .find(|p| p.id == "jina" && p.implemented)
                .map(|p| p.id.clone())
        })
        .unwrap_or_else(|| "jina".into())
}

fn stored_web_search_to_config(
    default_provider_id: String,
    providers: &[StoredWebSearchProvider],
) -> WebConfig {
    let key = |id: &str| {
        providers
            .iter()
            .find(|p| p.id == id)
            .and_then(|p| p.api_key.clone())
            .filter(|s| !s.is_empty())
    };
    WebConfig {
        search_provider: normalize_web_search_default(default_provider_id, providers),
        jina_api_key: key("jina"),
        bocha_api_key: key("bocha"),
        brave_api_key: key("brave"),
        tavily_api_key: key("tavily"),
        exa_api_key: key("exa"),
        serper_api_key: key("serper"),
        serpapi_api_key: key("serpapi"),
    }
}

pub fn read_web_config_pub() -> WebConfig {
    let json = read_config_json();
    let providers = read_stored_web_search_providers();
    stored_web_search_to_config(default_web_search_provider_id_from_json(&json), &providers)
}

fn default_provider_id_from_json(json: &serde_json::Value) -> String {
    json.get("defaultProviderId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("opencode")
        .to_string()
}

pub fn read_default_provider_id_pub() -> String {
    default_provider_id_from_json(&read_config_json())
}

pub fn read_default_model_pub() -> String {
    read_config_json()
        .get("defaultModel")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("deepseek-v4-flash")
        .to_string()
}

pub fn client_for_provider_id(provider_id: &str) -> Option<DeepSeekClient> {
    let providers = read_stored_providers();
    let p = providers.into_iter().find(|p| p.id == provider_id)?;
    let key = p
        .api_key
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "placeholder".into());
    DeepSeekClient::new(DeepSeekClientConfig {
        api_key: key,
        base_url: p.base_url,
        ..Default::default()
    })
    .ok()
}

/// Read stored API key from config file (public for use in main.rs startup)
pub fn read_stored_api_key_pub() -> Option<String> {
    read_stored_api_key()
}

/// Read stored base URL from config file (public for use in main.rs startup)
pub fn read_stored_base_url_pub() -> Option<String> {
    read_stored_base_url()
}

/// Read the active output-style name from config.json (`outputStyle` field).
/// `None` when unset/empty. Used at startup to seed `PromptAugment`.
pub fn read_active_output_style() -> Option<String> {
    let path = config_file_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("outputStyle")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Persist the active output-style name to config.json. `None` clears it.
pub fn write_active_output_style(name: Option<&str>) -> Result<(), String> {
    let path = config_file_path().ok_or("cannot resolve config dir")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut json: serde_json::Value = if let Ok(content) = std::fs::read_to_string(&path) {
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    match name {
        Some(n) => json["outputStyle"] = serde_json::Value::String(n.to_string()),
        None => {
            if let Some(obj) = json.as_object_mut() {
                obj.remove("outputStyle");
            }
        }
    }
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

/// Read stored API key from config file
fn read_stored_api_key() -> Option<String> {
    let default_id = default_provider_id_from_json(&read_config_json());
    read_stored_providers()
        .into_iter()
        .find(|p| p.id == default_id)
        .and_then(|p| p.api_key)
        .filter(|s| !s.is_empty())
}

/// Read stored base URL from config file
fn read_stored_base_url() -> Option<String> {
    let default_id = default_provider_id_from_json(&read_config_json());
    read_stored_providers()
        .into_iter()
        .find(|p| p.id == default_id)
        .map(|p| p.base_url)
        .filter(|s| !s.is_empty())
}

/// Restrict a credential file to owner read/write where the platform supports
/// it cheaply. Best-effort: failures are ignored (the file is still written).
fn harden_file_permissions(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o600);
            let _ = std::fs::set_permissions(path, perms);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path; // Windows: per-user %APPDATA% ACL; no cheap tightening.
    }
}

#[tauri::command]
pub async fn get_config(_state: tauri::State<'_, AppState>) -> Result<AppConfigDto, String> {
    let json = read_config_json();
    // Environment values remain a fallback/import path, but a packaged demo
    // works from config.json alone after the user fills settings in-app.
    let api_key_present = read_stored_api_key().is_some()
        || std::env::var("DEEPSEEK_API_KEY")
            .map(|k| !k.is_empty())
            .unwrap_or(false);

    let base_url = read_stored_base_url()
        .or_else(|| {
            std::env::var("DEEPSEEK_BASE_URL")
                .ok()
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "https://api.deepseek.com".into());

    let default_provider_id = default_provider_id_from_json(&json);
    let default_model = json
        .get("defaultModel")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::env::var("DEEPSEEK_MODEL")
                .ok()
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "deepseek-v4-flash".to_string());
    let providers = read_stored_providers()
        .into_iter()
        .map(|p| provider_to_dto(p, false))
        .collect();
    let permission_mode = json
        .get("permissionMode")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or(PermissionMode::Default);
    let theme = json
        .get("theme")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("dark")
        .to_string();
    let language = json
        .get("language")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("zh")
        .to_string();
    let budget = json
        .get("budget")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or(BudgetDto {
            mode: "unlimited".into(),
            limit_usd: None,
        });
    let compaction = json
        .get("compaction")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or(CompactionDto {
            trigger_ratio: 0.70,
            keep_recent_turns: 3,
        });
    let shell = json
        .get("shell")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or(ShellDto {
            timeout_secs: 120,
            max_output_bytes: 1_048_576,
        });

    Ok(AppConfigDto {
        api_key_present,
        base_url,
        default_model,
        default_provider_id,
        providers,
        web_search: read_web_search_config_dto(false),
        permission_mode,
        theme,
        language,
        budget,
        compaction,
        shell,
    })
}

/// Accepts a config patch and persists API key + base URL to config file.
#[tauri::command]
pub async fn set_config(
    state: tauri::State<'_, AppState>,
    patch: ConfigPatchDto,
) -> Result<AppConfigDto, String> {
    let mut json = read_config_json();
    if let Some(key) = patch.api_key.as_deref() {
        json["apiKey"] = serde_json::Value::String(key.to_string());
    }
    if let Some(url) = patch.base_url.as_deref() {
        json["baseUrl"] = serde_json::Value::String(url.to_string());
    }
    if let Some(model) = patch.default_model {
        json["defaultModel"] = serde_json::Value::String(model);
    }
    if let Some(mode) = patch.permission_mode {
        json["permissionMode"] = serde_json::to_value(mode).map_err(|e| e.to_string())?;
    }
    if let Some(theme) = patch.theme {
        json["theme"] = serde_json::Value::String(theme);
    }
    if let Some(language) = patch.language {
        json["language"] = serde_json::Value::String(language);
    }
    if let Some(budget) = patch.budget {
        json["budget"] = serde_json::to_value(budget).map_err(|e| e.to_string())?;
    }
    if let Some(compaction) = patch.compaction {
        json["compaction"] = serde_json::to_value(compaction).map_err(|e| e.to_string())?;
    }
    if let Some(shell) = patch.shell {
        json["shell"] = serde_json::to_value(shell).map_err(|e| e.to_string())?;
    }
    write_config_json(&json)?;
    get_config(state).await
}

#[tauri::command]
pub async fn save_providers(
    state: tauri::State<'_, AppState>,
    input: SaveProvidersInput,
) -> Result<AppConfigDto, String> {
    let old = read_stored_providers();
    let old_keys: std::collections::HashMap<String, String> = old
        .into_iter()
        .filter_map(|p| p.api_key.map(|k| (p.id, k)))
        .collect();
    let default_provider_id = input.default_provider_id.clone();
    let default_model = input.default_model.clone();
    let mut providers: Vec<StoredProviderConfig> = input
        .providers
        .iter()
        .map(|p| dto_to_provider(p, old_keys.get(&p.id).cloned()))
        .collect();
    for provider in &mut providers {
        if provider.id == default_provider_id {
            provider.enabled = true;
            for model in &mut provider.models {
                if model.id == default_model {
                    model.enabled = true;
                }
            }
        }
    }
    let mut json = read_config_json();
    json["providers"] = serde_json::to_value(&providers).map_err(|e| e.to_string())?;
    json["defaultProviderId"] = serde_json::Value::String(default_provider_id);
    json["defaultModel"] = serde_json::Value::String(default_model);
    if let Some(deepseek) = providers.iter().find(|p| p.id == "deepseek") {
        json["baseUrl"] = serde_json::Value::String(deepseek.base_url.clone());
        if let Some(key) = &deepseek.api_key {
            json["apiKey"] = serde_json::Value::String(key.clone());
        }
    }
    write_config_json(&json)?;
    get_config(state).await
}

#[tauri::command]
pub async fn save_web_search_config(
    state: tauri::State<'_, AppState>,
    input: SaveWebSearchConfigInput,
) -> Result<AppConfigDto, String> {
    let old = read_stored_web_search_providers();
    let old_keys: std::collections::HashMap<String, String> = old
        .into_iter()
        .filter_map(|p| p.api_key.map(|k| (p.id, k)))
        .collect();
    let mut providers: Vec<StoredWebSearchProvider> = input
        .providers
        .iter()
        .map(|p| dto_to_web_search_provider(p, old_keys.get(&p.id).cloned()))
        .collect();

    for template in web_search_provider_templates() {
        if !providers.iter().any(|p| p.id == template.id) {
            providers.push(template);
        }
    }

    let default_provider_id = normalize_web_search_default(input.default_provider_id, &providers);
    for provider in &mut providers {
        if provider.id == default_provider_id {
            provider.enabled = true;
        }
        if !provider.implemented {
            provider.enabled = false;
        }
    }

    let web_config = stored_web_search_to_config(default_provider_id.clone(), &providers);
    let mut json = read_config_json();
    json["webSearch"] = serde_json::json!({
        "defaultProviderId": default_provider_id,
        "providers": providers,
    });
    write_config_json(&json)?;
    state.web_state.set_config(web_config);
    get_config(state).await
}

fn provider_models_url(provider: &ProviderConfigDto) -> String {
    let base = provider.base_url.trim_end_matches('/');
    format!("{base}/models")
}

fn provider_headers(provider: &ProviderConfigDto) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    if let Some(key) = provider.api_key.as_deref().filter(|s| !s.is_empty()) {
        if provider.provider_type == "anthropic" {
            if let Ok(v) = HeaderValue::from_str(key) {
                headers.insert("x-api-key", v);
            }
            headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        } else {
            if let Ok(v) = HeaderValue::from_str(&format!("Bearer {key}")) {
                headers.insert(AUTHORIZATION, v);
            }
        }
    }
    headers
}

fn parse_model_list(value: serde_json::Value) -> Vec<ProviderModelDto> {
    let arr = value.get("data").and_then(|v| v.as_array());
    arr.into_iter()
        .flatten()
        .filter_map(|m| {
            let raw = m
                .get("id")
                .or_else(|| m.get("name"))
                .and_then(|v| v.as_str())?;
            let id = raw.trim_start_matches("models/").to_string();
            if id.is_empty() {
                return None;
            }
            Some(ProviderModelDto {
                label: id.clone(),
                id,
                enabled: true,
                supports_tools: true,
                supports_reasoning: false,
            })
        })
        .collect()
}

#[tauri::command]
pub async fn fetch_provider_models(
    input: ProviderModelsInput,
) -> Result<Vec<ProviderModelDto>, String> {
    let client = reqwest::Client::new();
    let mut req = client.get(provider_models_url(&input.provider));
    req = req.headers(provider_headers(&input.provider));
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!(
            "HTTP {}: {}",
            status.as_u16(),
            text.chars().take(300).collect::<String>()
        ));
    }
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(parse_model_list(value))
}

#[tauri::command]
pub async fn test_provider_connection(
    input: ProviderModelsInput,
) -> Result<ProviderTestResultDto, String> {
    let start = Instant::now();
    match fetch_provider_models(input).await {
        Ok(models) => Ok(ProviderTestResultDto {
            ok: true,
            latency_ms: start.elapsed().as_millis() as u64,
            model_count: models.len() as u64,
            error: None,
        }),
        Err(error) => Ok(ProviderTestResultDto {
            ok: false,
            latency_ms: start.elapsed().as_millis() as u64,
            model_count: 0,
            error: Some(error),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn config_path_is_under_root() {
        assert_eq!(
            config_path_in_root(&PathBuf::from("/data/crown")),
            PathBuf::from("/data/crown/config.json")
        );
    }
}
