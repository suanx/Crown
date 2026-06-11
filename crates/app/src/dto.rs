//! IPC boundary DTOs.
//!
//! Field names, optional-ness, and string enum values match
//! `docs/ipc-protocol-claude-aligned.md` v2 verbatim. Each type carries a
//! round-trip test against the canonical JSON form.
//!
//! ## Direction conventions
//!
//! Output DTOs (backend → frontend) derive `Serialize` and usually
//! `Deserialize` for round-trip tests. Input DTOs (frontend → backend) are
//! `Deserialize` only — Tauri command arguments use them.

#![allow(dead_code)] // many fields are placeholders for roadmap items

use serde::{Deserialize, Serialize};

use deepseek_core::permission::PermissionMode;
use deepseek_tools::permission::{PermissionRule, PermissionUpdate};

// ─── Time helper ─────────────────────────────────────────────────────────

pub(crate) fn ms_to_rfc3339(ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .unwrap_or_else(|| {
            chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0)
                .expect("epoch is always representable")
        })
        .to_rfc3339()
}

// ─── UsageStatsWindow enum ───────────────────────────────────────────────

/// Time window for [`UsageStatsDto`] aggregation. 5 archetypes covering
/// "right now" (`Session`), the calendar bucket (`Today`), rolling
/// recent ranges (`Last7d`/`Last30d`), and forever (`Lifetime`).
///
/// Wire format is camelCase string per protocol §6:
/// `"session" | "today" | "7d" | "30d" | "lifetime"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UsageStatsWindow {
    /// From the dev/app process start to now. Reset on app restart.
    #[default]
    #[serde(rename = "session")]
    Session,
    /// From UTC midnight to now (local-tz support deferred).
    #[serde(rename = "today")]
    Today,
    /// Now minus 7 × 24h.
    #[serde(rename = "7d")]
    Last7d,
    /// Now minus 30 × 24h.
    #[serde(rename = "30d")]
    Last30d,
    /// All persisted history (`since_ms = 0`).
    #[serde(rename = "lifetime")]
    Lifetime,
}

impl UsageStatsWindow {
    /// Lower-bound timestamp (epoch ms) for the window. `session_start_ms`
    /// is the [`crate::AppState`] baseline written once at startup.
    ///
    /// Today's bucket uses UTC midnight rather than local midnight in
    /// P3a — chrono's local-tz dependency is gated behind a feature we
    /// don't pull in, and "today UTC" is good enough for a usage badge
    /// (off by at most one timezone offset).
    pub fn since_ms(self, session_start_ms: i64) -> i64 {
        let now = chrono::Utc::now();
        match self {
            UsageStatsWindow::Session => session_start_ms,
            UsageStatsWindow::Today => now
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .map(|dt| dt.and_utc().timestamp_millis())
                .unwrap_or(0),
            UsageStatsWindow::Last7d => now.timestamp_millis() - 7 * 86_400_000,
            UsageStatsWindow::Last30d => now.timestamp_millis() - 30 * 86_400_000,
            UsageStatsWindow::Lifetime => 0,
        }
    }

    /// Wire-format string used in [`UsageStatsDto::window_label`].
    pub fn as_str(self) -> &'static str {
        match self {
            UsageStatsWindow::Session => "session",
            UsageStatsWindow::Today => "today",
            UsageStatsWindow::Last7d => "7d",
            UsageStatsWindow::Last30d => "30d",
            UsageStatsWindow::Lifetime => "lifetime",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSummaryDto {
    pub id: String,
    pub title: String,
    pub updated_at: String, // RFC3339
    pub message_count: u64,
    pub is_streaming: bool,
    pub is_pinned: bool,
    pub preview: Option<String>,
    pub project_id: Option<String>,
    /// Provider id, e.g. "deepseek". P3a defaults to "deepseek" for all
    /// rows; future multi-provider support flips this per thread.
    pub provider_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadDto {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub model: String,
    pub thinking_effort: String,
    pub permission_mode: PermissionMode,
    pub cost_usd: f64, // P4 always 0.0 (P3 fills)
    pub messages: Vec<MessageDto>,
    /// Provider id, e.g. "deepseek".
    pub provider_id: String,
    pub project_id: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummaryDto {
    pub id: String,
    pub name: String,
    pub path: String,
    pub thread_count: u64,
    pub last_used_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageDto {
    pub id: String,
    pub thread_id: String,
    /// Sequence number within the thread (0-based). Used by rewind to target
    /// a message position.
    pub seq: i64,
    pub role: String, // user | assistant | system | tool
    pub content: String,
    pub timestamp: String,
    pub reasoning: Option<String>,
    pub tool_calls: Option<Vec<ToolCallDto>>,
    pub segments: Vec<SegmentDto>,
    pub usage: Option<MessageUsageDto>,
    pub is_streaming: bool,
    pub interrupted: bool,
    #[serde(default)]
    pub brainstorm: Option<BrainstormMessageMetaDto>,
    /// File attachments sent with this message (filenames).
    #[serde(default)]
    pub attachments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SegmentDto {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "reasoning")]
    Reasoning { text: String },
    #[serde(rename = "tool")]
    Tool {
        call_id: String,
        name: String,
        input: serde_json::Value,
        status: String,
        result: Option<String>,
        duration_ms: Option<u64>,
        diff: Option<ToolDiffDto>,
        error_message: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrainstormMessageMetaDto {
    pub run_id: String,
    pub message_id: String,
    pub participant: BrainstormParticipantDto,
}

/// A point a thread can be rewound to — one per user message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewindPointDto {
    /// The user message's seq (rewind target).
    pub message_seq: i64,
    /// Short preview of the user message.
    pub preview: String,
    /// How many distinct files were changed at or after this point.
    pub files_changed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallDto {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
    pub status: String, // pending_approval | running | success | error | aborted
    pub result: Option<String>,
    pub duration_ms: Option<u64>,
    pub diff: Option<ToolDiffDto>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDiffDto {
    pub path: String,
    pub before: String,
    pub after: String,
}

/// A todo item crossing the IPC boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoItemDto {
    pub content: String,
    pub active_form: String,
    pub status: String,
}

impl From<deepseek_tools::todo::TodoItem> for TodoItemDto {
    fn from(t: deepseek_tools::todo::TodoItem) -> Self {
        // Serialize the status enum to its snake_case wire string
        // (pending | in_progress | completed) the frontend expects.
        let status = match t.status {
            deepseek_tools::todo::TodoStatus::Pending => "pending",
            deepseek_tools::todo::TodoStatus::InProgress => "in_progress",
            deepseek_tools::todo::TodoStatus::Completed => "completed",
        }
        .to_string();
        Self {
            content: t.content,
            active_form: t.active_form,
            status,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageUsageDto {
    /// Cache-read input tokens (the cheapest tier).
    /// DeepSeek: prompt_cache_hit. OpenAI: cached. Anthropic: cache_read.
    pub cache_read_tokens: u64,
    /// Uncached input tokens.
    /// DeepSeek: prompt_cache_miss. OpenAI: prompt - cached. Anthropic:
    /// input - cache_read - cache_creation.
    pub cache_miss_tokens: u64,
    /// Cache-creation input tokens (Anthropic-only; usually 1.25× input).
    /// DeepSeek and OpenAI always emit 0 here.
    pub cache_creation_tokens: u64,
    /// Output / completion tokens.
    pub output_tokens: u64,
    /// Cost in USD computed at emit time using the active price table.
    pub cost_usd: f64,
}

// ─── Models / Config / Mcp / Usage DTOs ──────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfoDto {
    pub id: String,
    pub label: String,
    pub description: String,
    pub price_per_million_input_usd: f64,
    pub price_per_million_output_usd: f64,
    pub price_per_million_cache_hit_usd: f64,
    pub context_window: u64,
    /// Provider id, e.g. "deepseek". Lets the UI group / switch models
    /// by source. P3a only emits "deepseek".
    pub provider_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigDto {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_key_present: bool,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub models: Vec<ProviderModelDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelDto {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub supports_tools: bool,
    #[serde(default)]
    pub supports_reasoning: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProvidersInput {
    pub providers: Vec<ProviderConfigDto>,
    pub default_provider_id: String,
    pub default_model: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelsInput {
    pub provider: ProviderConfigDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTestResultDto {
    pub ok: bool,
    pub latency_ms: u64,
    pub model_count: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfigDto {
    pub api_key_present: bool,
    pub base_url: String,
    pub default_model: String,
    pub default_provider_id: String,
    pub providers: Vec<ProviderConfigDto>,
    pub web_search: WebSearchConfigDto,
    pub permission_mode: PermissionMode,
    pub theme: String,    // light | dark | system
    pub language: String, // zh | en
    pub budget: BudgetDto,
    pub compaction: CompactionDto,
    pub shell: ShellDto,
    pub subagent: SubagentDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetDto {
    pub mode: String, // per_session | per_day | unlimited
    pub limit_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionDto {
    pub trigger_ratio: f64,
    pub keep_recent_turns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellDto {
    pub timeout_secs: u64,
    pub max_output_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentDto {
    pub max_subtasks: u64,
    pub model: String,
}



#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchConfigDto {
    pub default_provider_id: String,
    pub providers: Vec<WebSearchProviderDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchProviderDto {
    pub id: String,
    pub name: String,
    pub api_key: Option<String>,
    pub api_key_present: bool,
    pub enabled: bool,
    pub implemented: bool,
    pub key_required: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveWebSearchConfigInput {
    pub default_provider_id: String,
    pub providers: Vec<WebSearchProviderDto>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigPatchDto {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
    pub permission_mode: Option<PermissionMode>,
    pub theme: Option<String>,
    pub language: Option<String>,
    pub budget: Option<BudgetDto>,
    pub compaction: Option<CompactionDto>,
    pub shell: Option<ShellDto>,
    pub subagent: Option<SubagentDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerDto {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub status: String,
    pub enabled: bool,
    pub tool_count: u64,
    pub error_message: Option<String>,
}

/// A discovered skill crossing the IPC boundary.
///
/// `scope` is `"global" | "project"`; `source` is
/// `"native" | "claude" | "mcp"`. Both are provider-/origin-agnostic
/// descriptors the UI uses to badge skills. `path` is the absolute
/// SKILL.md path (so the UI can reveal-in-folder).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDto {
    pub name: String,
    pub description: String,
    pub scope: String,
    pub source: String,
    pub path: String,
    pub allowed_tools: Vec<String>,
}

impl From<&deepseek_skill::discovery::SkillMeta> for SkillDto {
    fn from(m: &deepseek_skill::discovery::SkillMeta) -> Self {
        use deepseek_skill::discovery::{Scope, Source};
        let scope = match m.scope {
            Scope::Global => "global",
            Scope::Project => "project",
        };
        let source = match m.source {
            Source::Native => "native",
            Source::Claude => "claude",
            Source::Mcp => "mcp",
        };
        Self {
            name: m.name.clone(),
            description: m.description.clone(),
            scope: scope.into(),
            source: source.into(),
            path: m.path.to_string_lossy().into_owned(),
            allowed_tools: m.allowed_tools.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageStatsDto {
    pub total_cost_usd: f64,
    /// Cumulative cache savings in USD across all turns in the window
    /// — `(cache_miss_price - cache_read_price) × cache_read_tokens / 1M`.
    /// Filled by [`crate::commands::stats::get_usage_stats`] (P3a task 6);
    /// task 3 just defines the field.
    pub cumulative_cache_saved_usd: f64,
    /// Aggregated cache-read input tokens.
    pub cache_read_tokens: u64,
    /// Aggregated uncached input tokens.
    pub cache_miss_tokens: u64,
    /// Aggregated cache-creation input tokens (Anthropic-only, 0 here).
    pub cache_creation_tokens: u64,
    /// Aggregated output / completion tokens.
    pub output_tokens: u64,
    pub cache_hit_ratio: f64,
    pub window_label: String,
    pub budget_limit_usd: Option<f64>,
    pub budget_used_pct: Option<f64>,
}


/// One point in the daily usage chart — cost + token breakdown per day.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageChartPoint {
    /// Start-of-day epoch ms (UTC) — for frontend date display.
    pub day_epoch_ms: i64,
    pub cache_read_tokens: u64,
    pub cache_miss_tokens: u64,
    pub output_tokens: u64,
    pub total_cost_usd: f64,
}


// ─── Input DTOs (frontend → backend) ─────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageInput {
    pub thread_id: String,
    pub content: String,
    #[serde(default)]
    pub attachments: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartBrainstormInput {
    pub thread_id: String,
    pub topic: String,
    #[serde(default)]
    pub rounds: Option<u8>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContinueBrainstormInput {
    pub thread_id: String,
    pub run_id: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrainstormParticipantDto {
    pub id: String,
    pub name: String,
    pub role: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartBrainstormResultDto {
    pub run_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproveToolInput {
    pub thread_id: String,
    pub tool_use_id: String,
    pub decision: ApproveToolDecision,
}

/// Wire shape of the user's approval decision. Mirrors
/// [`deepseek_core::gate::ApprovalDecision`] but lives in the DTO layer so
/// the IPC boundary is fully serde-driven.
#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "behavior",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum ApproveToolDecision {
    Allow {
        #[serde(default)]
        updated_input: serde_json::Value,
        #[serde(default)]
        permission_updates: Vec<PermissionUpdate>,
    },
    Deny {
        #[serde(default)]
        message: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateThreadInput {
    pub thread_id: String,
    pub title: Option<String>,
    pub is_pinned: Option<bool>,
    pub permission_mode: Option<PermissionMode>,
    pub thinking_effort: Option<String>,
    pub project_id: Option<Option<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateThreadInput {
    pub project_id: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub provider_id: Option<String>,
    pub thinking_effort: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectInput {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProjectInput {
    pub project_id: String,
    pub name: Option<String>,
    pub path: Option<String>,
}

/// Optional input for `get_usage_stats`. Absent / missing `window` ⇒
/// [`UsageStatsWindow::Session`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUsageStatsInput {
    /// Window archetype; see [`UsageStatsWindow`] variants.
    pub window: Option<UsageStatsWindow>,
}

/// Optional input for `get_user_balance`. Absent / missing `providerId`
/// ⇒ `"deepseek"`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUserBalanceInput {
    /// Provider identifier (e.g. `"deepseek"`). Future providers can be
    /// queried by passing their id once the integration ships.
    pub provider_id: Option<String>,
}

/// Provider wallet snapshot returned by `get_user_balance`. All failure
/// modes (network, auth, unsupported provider) collapse to `Option::None`
/// at the command boundary; this struct is only present when there is
/// real balance data to show.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserBalanceDto {
    /// Whether the API key is currently usable (DeepSeek's
    /// `is_available`). When `false`, chat requests will fail until
    /// the user tops up.
    pub is_available: bool,
    /// Currency of the primary wallet (largest `total`). Empty string
    /// when `balance_infos` is empty — unusual but handled.
    pub primary_currency: String,
    /// One entry per wallet currency. Sorted by API; UI typically picks
    /// the largest via the same rule as
    /// [`deepseek_client::deepseek::pick_primary_balance`].
    pub balance_infos: Vec<BalanceInfoDto>,
}

/// Single-currency wallet entry. Numeric strings from the API are parsed
/// to `f64` here for direct UI use; parse failures collapse to 0 (which
/// is conservative — the UI shows ¥0 instead of crashing).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceInfoDto {
    /// ISO currency code (`"CNY"`, `"USD"`, ...).
    pub currency: String,
    /// Total balance including granted + topped-up portions.
    pub total: f64,
    /// Free credit DeepSeek granted at signup or via promos. `None` when
    /// the API didn't return this field for this wallet.
    pub granted: Option<f64>,
    /// Money the user paid in via top-up.
    pub topped_up: Option<f64>,
}

// ─── Permission context DTO ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolPermissionContextDto {
    pub mode: PermissionMode,
    pub always_allow_rules: Vec<PermissionRule>,
    pub always_deny_rules: Vec<PermissionRule>,
    pub always_ask_rules: Vec<PermissionRule>,
    pub additional_working_directories: Vec<String>,
    pub is_bypass_permissions_mode_available: bool,
}

/// Result of cycling the permission mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CyclePermissionModeResult {
    /// The new active mode after cycling.
    pub new_mode: PermissionMode,
}

// ─── From conversions ────────────────────────────────────────────────────

impl From<deepseek_state::ThreadSummary> for ThreadSummaryDto {
    fn from(s: deepseek_state::ThreadSummary) -> Self {
        Self {
            id: s.id,
            title: s.name.unwrap_or_else(|| "New chat".into()),
            updated_at: ms_to_rfc3339(s.updated_at),
            message_count: s.message_count,
            is_streaming: false, // P4: derived by frontend from stream events
            is_pinned: s.is_pinned,
            preview: s.preview,
            project_id: s.project_id,
            provider_id: s.provider_id,
        }
    }
}

impl From<deepseek_state::ProjectSummary> for ProjectSummaryDto {
    fn from(p: deepseek_state::ProjectSummary) -> Self {
        Self {
            id: p.id,
            name: p.name,
            path: p.path,
            thread_count: p.thread_count,
            last_used_at: ms_to_rfc3339(p.last_used_at),
        }
    }
}

impl From<deepseek_state::Thread> for ThreadDto {
    fn from(t: deepseek_state::Thread) -> Self {
        Self {
            id: t.id,
            title: t.name.unwrap_or_else(|| "New chat".into()),
            created_at: ms_to_rfc3339(t.created_at),
            updated_at: ms_to_rfc3339(t.updated_at),
            model: t.model,
            thinking_effort: t.thinking_effort,
            permission_mode: PermissionMode::from_str_lossy(&t.permission_mode),
            cost_usd: 0.0,
            messages: Vec::new(), // populated by commands::get_thread
            provider_id: t.provider_id,
            project_id: t.project_id,
            cwd: t.cwd,
        }
    }
}

// ─── Round-trip tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn thread_summary_round_trip() {
        let dto = ThreadSummaryDto {
            id: "01H0000".into(),
            title: "test".into(),
            updated_at: "2026-05-28T12:00:00+00:00".into(),
            message_count: 5,
            is_streaming: false,
            is_pinned: true,
            preview: Some("hello".into()),
            project_id: Some("proj_1".into()),
            provider_id: "deepseek".into(),
        };
        let v = serde_json::to_value(&dto).unwrap();
        assert_eq!(v["id"], "01H0000");
        assert_eq!(v["isPinned"], true);
        assert_eq!(v["projectId"], "proj_1");
        assert_eq!(v["messageCount"], 5);
        assert_eq!(v["providerId"], "deepseek");
        let back: ThreadSummaryDto = serde_json::from_value(v).unwrap();
        assert_eq!(back.id, dto.id);
        assert_eq!(back.is_pinned, dto.is_pinned);
        assert_eq!(back.provider_id, dto.provider_id);
    }

    #[test]
    fn approve_tool_decision_allow_form() {
        let json = json!({
            "behavior": "allow",
            "updatedInput": {"path": "/tmp"},
            "permissionUpdates": []
        });
        let dec: ApproveToolDecision = serde_json::from_value(json).unwrap();
        match dec {
            ApproveToolDecision::Allow {
                updated_input,
                permission_updates,
            } => {
                assert_eq!(updated_input, json!({"path": "/tmp"}));
                assert!(permission_updates.is_empty());
            }
            _ => panic!("expected allow"),
        }
    }

    #[test]
    fn approve_tool_decision_deny_with_null_message() {
        let json = json!({"behavior": "deny", "message": null});
        let dec: ApproveToolDecision = serde_json::from_value(json).unwrap();
        assert!(matches!(dec, ApproveToolDecision::Deny { message: None }));
    }

    #[test]
    fn approve_tool_decision_deny_with_feedback() {
        let json = json!({"behavior": "deny", "message": "no thanks"});
        let dec: ApproveToolDecision = serde_json::from_value(json).unwrap();
        if let ApproveToolDecision::Deny { message: Some(m) } = dec {
            assert_eq!(m, "no thanks");
        } else {
            panic!("expected deny with feedback");
        }
    }

    #[test]
    fn update_thread_input_partial_fields() {
        let json = json!({"threadId": "x", "permissionMode": "plan"});
        let upd: UpdateThreadInput = serde_json::from_value(json).unwrap();
        assert_eq!(upd.thread_id, "x");
        assert_eq!(upd.permission_mode, Some(PermissionMode::Plan));
        assert!(upd.title.is_none());
        assert!(upd.is_pinned.is_none());
        assert!(upd.project_id.is_none());
    }

    #[test]
    fn tool_permission_context_dto_camelcase() {
        let dto = ToolPermissionContextDto {
            mode: PermissionMode::Default,
            always_allow_rules: vec![],
            always_deny_rules: vec![],
            always_ask_rules: vec![],
            additional_working_directories: vec![],
            is_bypass_permissions_mode_available: false,
        };
        let v = serde_json::to_value(&dto).unwrap();
        assert!(v.get("alwaysAllowRules").is_some());
        assert!(v.get("isBypassPermissionsModeAvailable").is_some());
        assert!(v.get("additionalWorkingDirectories").is_some());
    }

    #[test]
    fn brainstorm_message_meta_round_trip() {
        let meta = BrainstormMessageMetaDto {
            run_id: "run_1".into(),
            message_id: "msg_1".into(),
            participant: BrainstormParticipantDto {
                id: "critic".into(),
                name: "反方审查".into(),
                role: "审查风险漏洞".into(),
                color: "#FF6B6B".into(),
            },
        };
        let v = serde_json::to_value(&meta).unwrap();
        assert_eq!(v["runId"], "run_1");
        assert_eq!(v["messageId"], "msg_1");
        assert_eq!(v["participant"]["id"], "critic");
        let back: BrainstormMessageMetaDto = serde_json::from_value(v).unwrap();
        assert_eq!(back.participant.name, "反方审查");
    }

    #[test]
    fn ms_to_rfc3339_known_value() {
        // Round-trip: convert a known ms value through and back via chrono
        // to verify the output is a valid RFC3339 timestamp ending in `+00:00`.
        let s = ms_to_rfc3339(1_700_000_000_000);
        assert!(s.contains("T"), "expected RFC3339 with T separator: {s}");
        assert!(
            s.ends_with("+00:00") || s.ends_with('Z'),
            "expected UTC suffix: {s}"
        );
        // Round-trip: parse back and confirm same ms.
        let dt = chrono::DateTime::parse_from_rfc3339(&s).expect("parse");
        assert_eq!(dt.timestamp_millis(), 1_700_000_000_000);
    }

    #[test]
    fn config_patch_partial_fields() {
        let json = json!({"defaultModel": "deepseek-v4-pro"});
        let patch: ConfigPatchDto = serde_json::from_value(json).unwrap();
        assert_eq!(patch.default_model, Some("deepseek-v4-pro".into()));
        assert!(patch.theme.is_none());
    }

    #[test]
    fn model_info_dto_carries_provider_id() {
        let dto = ModelInfoDto {
            id: "deepseek-v4-flash".into(),
            label: "DeepSeek V4 Flash".into(),
            description: "fast".into(),
            price_per_million_input_usd: 0.14,
            price_per_million_output_usd: 0.28,
            price_per_million_cache_hit_usd: 0.0028,
            context_window: 1_000_000,
            provider_id: "deepseek".into(),
        };
        let v = serde_json::to_value(&dto).unwrap();
        assert_eq!(v["providerId"], "deepseek");
        let back: ModelInfoDto = serde_json::from_value(v).unwrap();
        assert_eq!(back.provider_id, "deepseek");
    }

    #[test]
    fn usage_stats_window_serde_strings() {
        for (w, s) in [
            (UsageStatsWindow::Session, "session"),
            (UsageStatsWindow::Today, "today"),
            (UsageStatsWindow::Last7d, "7d"),
            (UsageStatsWindow::Last30d, "30d"),
            (UsageStatsWindow::Lifetime, "lifetime"),
        ] {
            let v = serde_json::to_value(w).unwrap();
            assert_eq!(v, serde_json::Value::String(s.into()));
            let back: UsageStatsWindow =
                serde_json::from_value(serde_json::Value::String(s.into())).unwrap();
            assert_eq!(back, w);
            assert_eq!(w.as_str(), s);
        }
    }

    #[test]
    fn usage_stats_window_since_ms_orderings() {
        let session_start = 1_000_000_i64;
        let now_ms = chrono::Utc::now().timestamp_millis();

        let session = UsageStatsWindow::Session.since_ms(session_start);
        assert_eq!(session, session_start);

        let today = UsageStatsWindow::Today.since_ms(session_start);
        // today midnight is no later than now and at most 24h earlier
        assert!(today <= now_ms);
        assert!(now_ms - today <= 86_400_000);

        let d7 = UsageStatsWindow::Last7d.since_ms(session_start);
        assert_eq!(d7, now_ms - 7 * 86_400_000);

        let d30 = UsageStatsWindow::Last30d.since_ms(session_start);
        assert_eq!(d30, now_ms - 30 * 86_400_000);
        assert!(d30 < d7); // 30d window starts earlier than 7d

        assert_eq!(UsageStatsWindow::Lifetime.since_ms(session_start), 0);
    }

    #[test]
    fn usage_stats_window_default_is_session() {
        assert_eq!(UsageStatsWindow::default(), UsageStatsWindow::Session);
    }

    #[test]
    fn skill_dto_camelcase_and_from_meta() {
        use deepseek_skill::discovery::{Scope, SkillMeta, Source};
        let meta = SkillMeta {
            name: "commit".into(),
            description: "Create a git commit. Use when the user asks to commit.".into(),
            scope: Scope::Project,
            source: Source::Native,
            path: std::path::PathBuf::from("/tmp/.crown/skills/commit/SKILL.md"),
            allowed_tools: vec!["run_command".into()],
        };
        let dto = SkillDto::from(&meta);
        let v = serde_json::to_value(&dto).unwrap();
        assert_eq!(v["name"], "commit");
        assert_eq!(v["scope"], "project");
        assert_eq!(v["source"], "native");
        assert_eq!(v["allowedTools"][0], "run_command");
        assert!(v.as_object().unwrap().contains_key("description"));
        let back: SkillDto = serde_json::from_value(v).unwrap();
        assert_eq!(back.name, "commit");
        assert_eq!(back.allowed_tools, vec!["run_command".to_string()]);
    }
}

// ─── AskUserQuestion (EPIC 1) ───────────────────────────────────────────

/// 前端提交结构化问答答案的入参（对应 `submit_answers` command）。
///
/// Input DTO（frontend → backend），仅 `Deserialize`。内嵌 [`deepseek_tools::AnswerItem`]
/// （同为 receive-only），直接做嵌套反序列化。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitAnswersInput {
    /// 对应的 tool_use id。
    pub tool_use_id: String,
    /// 用户点了取消（= 拒绝工具调用）。
    #[serde(default)]
    pub cancelled: bool,
    /// 逐题答案（透传给 `QuestionAnswers`）。
    #[serde(default)]
    pub answers: Vec<deepseek_tools::AnswerItem>,
}
