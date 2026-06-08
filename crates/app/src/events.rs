//! Event emission helpers — map [`EngineEvent`] to Tauri events with
//! claude-aligned camelCase payloads.
//!
//! Event names + payload shapes track `docs/ipc-protocol-claude-aligned.md`
//! §5 verbatim, with **nested** structures matching
//! `frontend/src/api/contracts.ts`:
//! - `stream:tool_call_start` → `{ threadId, messageId, toolCall: ToolCall }`
//! - `stream:tool_call_update` → flat fields (toolUseId/status/...) plus
//!   `diff: ToolDiff | null` reserved for P5 (Roadmap GAP-DIFF-001)
//! - `stream:turn_complete` → `{ threadId, messageId, usage: MessageUsage }`
//!
//! All field names use camelCase via `#[serde(rename_all = "camelCase")]`.
//!
//! ## ToolCallStart status
//!
//! At emit time the call has been parsed from the model stream but **the
//! permission decision is not yet known**. The semantically correct status
//! is `pending_approval` — the UI uses it to render an "awaiting decision"
//! ToolCallCard until either:
//! - a `tool_call_update` with `status=running` arrives (gate allowed
//!   without prompting, or user approved), or
//! - an `approval:request` event arrives (gate decided `ask`), promoting
//!   the card to a full `ApprovalDialog`.
//!
//! Emitting `running` here would cause the card to skip the
//! "awaiting" state and show "running" while the gate is still deciding.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use deepseek_core::engine::{EngineEvent, ToolStatusEvent};

use crate::dto::{MessageUsageDto, TodoItemDto, ToolCallDto, ToolDiffDto};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContentDeltaPayload {
    pub thread_id: String,
    pub message_id: String,
    pub delta: String,
    /// Sub-agent thread id when this event came from a sub-agent (P4);
    /// `None` for the main agent. The UI nests sub-agent activity under the
    /// owning `task` tool card.
    pub agent_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolCallStartPayload {
    pub thread_id: String,
    pub message_id: String,
    pub tool_call: ToolCallDto,
    pub agent_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolCallUpdatePayload {
    pub thread_id: String,
    pub message_id: String,
    pub tool_use_id: String,
    pub status: String,
    /// 工具入参。流式参数增长时可能是 partial input；流结束后回填完整 input。
    /// 纯状态更新为 `None`，前端保留已有 input。
    pub input: Option<serde_json::Value>,
    pub result: Option<String>,
    /// P4 forever `None`; P5 file-edit diff support fills this in
    /// (Roadmap GAP-DIFF-001).
    pub diff: Option<ToolDiffDto>,
    pub duration_ms: Option<u64>,
    pub error_message: Option<String>,
    pub agent_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TurnCompletePayload {
    pub thread_id: String,
    pub message_id: String,
    pub usage: MessageUsageDto,
    pub agent_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TodosUpdatedPayload {
    pub thread_id: String,
    pub todos: Vec<TodoItemDto>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StreamErrorPayload {
    pub thread_id: String,
    pub message_id: Option<String>,
    pub error: String,
    pub retryable: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StreamAbortedPayload {
    pub thread_id: String,
    pub message_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContextUsagePayload {
    pub thread_id: String,
    pub used_tokens: u64,
    pub max_tokens: u64,
    pub ratio: f64,
    pub source: String,
}

fn status_to_str(s: ToolStatusEvent) -> &'static str {
    match s {
        ToolStatusEvent::PendingApproval => "pending_approval",
        ToolStatusEvent::Running => "running",
        ToolStatusEvent::Success => "success",
        ToolStatusEvent::Error => "error",
        ToolStatusEvent::Aborted => "aborted",
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpServerStatusPayload {
    pub name: String,
    pub status: String,
    pub error: Option<String>,
}

/// Emit `mcp:server_status_changed` for a single server.
pub fn dispatch_mcp_status(
    app: &AppHandle,
    name: &str,
    status: deepseek_mcp::types::ServerStatus,
    error: Option<String>,
) {
    let status_str = serde_json::to_value(status)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "failed".to_string());
    let _ = app.emit(
        "mcp:server_status_changed",
        McpServerStatusPayload {
            name: name.to_string(),
            status: status_str,
            error,
        },
    );
}

/// Emit `mcp:tools_changed` (no payload) so the UI re-fetches the tool list.
pub fn dispatch_mcp_tools_changed(app: &AppHandle) {
    let _ = app.emit("mcp:tools_changed", ());
}

/// Dispatch a single [`EngineEvent`] to its corresponding Tauri event
/// (main-agent events; `agentId` is `None`).
pub fn dispatch_engine_event(app: &AppHandle, event: EngineEvent) {
    dispatch_engine_event_scoped(app, event, None, None);
}

/// Whether a **sub-agent** event of this kind is surfaced to the UI nested
/// under the parent's `task` tool card.
///
/// The frontend `chatStore.updateSubAgent` only consumes three event kinds
/// when they carry an `agentId`: incremental assistant text
/// (`content_delta`), nested tool-call start, and nested tool-call update.
/// For those, the payload's `threadId` must be the **parent** thread (the
/// one actually loaded in `threadsById`) so the reducer can locate the
/// owning card — otherwise every reducer's `if (!thread) return s` guard
/// drops the event because the sub-thread is never loaded (it is hidden
/// from the sidebar).
///
/// All other sub-agent events (reasoning, turn_complete, aborted, error,
/// todos) are intentionally **not** surfaced: routing them to the parent
/// thread id would corrupt parent state (e.g. prematurely flip the parent's
/// streaming message to complete, or replace the parent todo list). Keeping
/// them on the sub-thread id makes the frontend ignore them safely.
fn sub_agent_event_is_surfaced(event: &EngineEvent) -> bool {
    matches!(
        event,
        EngineEvent::ContentDelta { .. }
            | EngineEvent::ToolCallStart { .. }
            | EngineEvent::ToolCallUpdate { .. }
    )
}

/// Dispatch an [`EngineEvent`] tagging it with an optional sub-agent id (P4).
///
/// `agent_id = Some(sub_thread_id)` marks the event as coming from a
/// sub-agent; the UI nests it under the owning `task` card. `None` is a
/// main-agent event.
///
/// `parent_thread_id = Some(parent)` is the parent (visible) thread for a
/// sub-agent. For the UI-surfaced event kinds (see
/// [`sub_agent_event_is_surfaced`]) the emitted payload's `threadId` is
/// rewritten to this parent id so the frontend can locate the loaded thread
/// and the owning card; the `agentId` still carries the sub-thread id for
/// per-sub-agent binding. For main-agent events it is `None`.
pub fn dispatch_engine_event_scoped(
    app: &AppHandle,
    event: EngineEvent,
    agent_id: Option<String>,
    parent_thread_id: Option<String>,
) {
    // For sub-agent events the UI nests under the parent card, emit the
    // parent thread id as `threadId` (frontend finds the loaded thread);
    // otherwise keep the event's own (sub) thread id.
    let routed_parent: Option<String> = match parent_thread_id {
        Some(pid) if sub_agent_event_is_surfaced(&event) => Some(pid),
        _ => None,
    };
    match event {
        EngineEvent::ContentDelta {
            thread_id,
            message_id,
            delta,
        } => {
            let _ = app.emit(
                "stream:content_delta",
                ContentDeltaPayload {
                    thread_id: routed_parent.unwrap_or(thread_id),
                    message_id,
                    delta,
                    agent_id,
                },
            );
        }
        EngineEvent::ReasoningDelta {
            thread_id,
            message_id,
            delta,
        } => {
            let _ = app.emit(
                "stream:reasoning_delta",
                ContentDeltaPayload {
                    thread_id,
                    message_id,
                    delta,
                    agent_id,
                },
            );
        }
        EngineEvent::ToolCallStart {
            thread_id,
            message_id,
            tool_use_id,
            tool_name,
            input,
        } => {
            // Status is `pending_approval` because the permission gate has
            // not yet decided. The runtime will emit a follow-up
            // `tool_call_update` with `status=running` once the call is
            // cleared to execute.
            let tool_call = ToolCallDto {
                id: tool_use_id,
                name: tool_name,
                input,
                status: "pending_approval".into(),
                result: None,
                duration_ms: None,
                diff: None,
                error_message: None,
            };
            let _ = app.emit(
                "stream:tool_call_start",
                ToolCallStartPayload {
                    thread_id: routed_parent.unwrap_or(thread_id),
                    message_id,
                    tool_call,
                    agent_id,
                },
            );
        }
        EngineEvent::ToolCallUpdate {
            thread_id,
            message_id,
            tool_use_id,
            status,
            input,
            result,
            duration_ms,
            error_message,
        } => {
            let _ = app.emit(
                "stream:tool_call_update",
                ToolCallUpdatePayload {
                    thread_id: routed_parent.unwrap_or(thread_id),
                    message_id,
                    tool_use_id,
                    status: status_to_str(status).into(),
                    input,
                    result,
                    diff: None,
                    duration_ms,
                    error_message,
                    agent_id,
                },
            );
        }
        EngineEvent::TurnComplete {
            thread_id,
            message_id,
            usage,
            cost_usd,
        } => {
            // Map provider-internal Usage to provider-agnostic 4-tier
            // breakdown for IPC. DeepSeek emits prompt_cache_hit_tokens
            // and prompt_cache_miss_tokens directly; cache_creation is
            // Anthropic-only and stays 0 here.
            let dto = MessageUsageDto {
                cache_read_tokens: usage.prompt_cache_hit_tokens as u64,
                cache_miss_tokens: usage.prompt_cache_miss_tokens as u64,
                cache_creation_tokens: 0,
                output_tokens: usage.completion_tokens as u64,
                cost_usd,
            };
            let _ = app.emit(
                "stream:turn_complete",
                TurnCompletePayload {
                    thread_id,
                    message_id,
                    usage: dto,
                    agent_id,
                },
            );
        }
        EngineEvent::TodosUpdated { thread_id, todos } => {
            let _ = app.emit(
                "stream:todos_updated",
                TodosUpdatedPayload {
                    thread_id,
                    todos: todos.into_iter().map(TodoItemDto::from).collect(),
                },
            );
        }
        EngineEvent::ContextUsage {
            thread_id,
            used_tokens,
            max_tokens,
            ratio,
            source,
        } => {
            // Sub-agent context usage is internal noise — only surface the
            // main agent's ring.
            if agent_id.is_some() {
                return;
            }
            let source_str = match source {
                deepseek_core::compaction::ContextUsageSource::Api => "api",
                deepseek_core::compaction::ContextUsageSource::Local => "local",
            };
            let _ = app.emit(
                "stream:context_usage",
                ContextUsagePayload {
                    thread_id,
                    used_tokens,
                    max_tokens,
                    ratio,
                    source: source_str.to_string(),
                },
            );
        }
        EngineEvent::Aborted {
            thread_id,
            message_id,
        } => {
            let _ = app.emit(
                "stream:aborted",
                StreamAbortedPayload {
                    thread_id,
                    message_id,
                },
            );
        }
        EngineEvent::Error {
            thread_id,
            message_id,
            error,
            retryable,
        } => {
            let _ = app.emit(
                "stream:error",
                StreamErrorPayload {
                    thread_id,
                    message_id,
                    error,
                    retryable,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    //! Shape contract tests — each test pins the JSON wire format the
    //! frontend's contracts.ts expects. If a field
    //! is renamed, removed, or re-typed, the assertion below blows up at
    //! `cargo test` time so we never ship a shape mismatch into IPC.
    //!
    //! Coverage matches `INTEGRATION_RUNBOOK_2026-05-28.md` §3 critical
    //! shape checks.

    use super::*;
    use deepseek_client::types::Usage;
    use serde_json::json;

    fn sample_tool_call(status: &str) -> ToolCallDto {
        ToolCallDto {
            id: "toolu_01".into(),
            name: "write_file".into(),
            input: json!({"path": "/tmp/x.txt", "content": "hi"}),
            status: status.into(),
            result: None,
            duration_ms: None,
            diff: None,
            error_message: None,
        }
    }

    /// `stream:content_delta` and `stream:reasoning_delta` share the same
    /// payload shape — verify the canonical JSON form.
    #[test]
    fn content_delta_payload_shape() {
        let p = ContentDeltaPayload {
            thread_id: "th_1".into(),
            message_id: "msg_1".into(),
            delta: "Hello".into(),
            agent_id: None,
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(
            v,
            json!({
                "threadId": "th_1",
                "messageId": "msg_1",
                "delta": "Hello",
                "agentId": serde_json::Value::Null,
            })
        );
        let back: ContentDeltaPayload = serde_json::from_value(v).unwrap();
        assert_eq!(back, p);
    }

    /// Critical: `tool_call_start.toolCall.status` must be
    /// `"pending_approval"` so the UI can render the awaiting-decision
    /// card. Frontend runbook §3 step 3 asserts this.
    #[test]
    fn tool_call_start_status_is_pending_approval() {
        let p = ToolCallStartPayload {
            thread_id: "th_1".into(),
            message_id: "msg_1".into(),
            tool_call: sample_tool_call("pending_approval"),
            agent_id: None,
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["toolCall"]["status"], "pending_approval");
        assert_eq!(v["toolCall"]["id"], "toolu_01");
        assert_eq!(v["toolCall"]["name"], "write_file");
        // input passes through verbatim
        assert_eq!(v["toolCall"]["input"]["path"], "/tmp/x.txt");
        // P4 invariants: result/durationMs/diff/errorMessage all explicit null
        assert_eq!(v["toolCall"]["result"], serde_json::Value::Null);
        assert_eq!(v["toolCall"]["durationMs"], serde_json::Value::Null);
        assert_eq!(v["toolCall"]["diff"], serde_json::Value::Null);
        assert_eq!(v["toolCall"]["errorMessage"], serde_json::Value::Null);
    }

    /// `tool_call_update` carries `diff: null` always in P4. P5 will fill
    /// in for filesystem edit tools.
    #[test]
    fn tool_call_update_diff_is_always_null_in_p4() {
        let p = ToolCallUpdatePayload {
            thread_id: "th_1".into(),
            message_id: "msg_1".into(),
            tool_use_id: "toolu_01".into(),
            status: "success".into(),
            input: None,
            result: Some("OK".into()),
            diff: None,
            duration_ms: Some(42),
            error_message: None,
            agent_id: None,
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["toolUseId"], "toolu_01");
        assert_eq!(v["status"], "success");
        assert_eq!(v["diff"], serde_json::Value::Null);
        assert_eq!(v["durationMs"], 42);
        assert_eq!(v["errorMessage"], serde_json::Value::Null);
    }

    /// All four runtime statuses must serialize to their canonical lowercase
    /// strings — these are the values the prototype's `ToolStatus` union
    /// expects.
    #[test]
    fn tool_call_update_all_status_strings() {
        for (variant, expected) in [
            (ToolStatusEvent::Running, "running"),
            (ToolStatusEvent::Success, "success"),
            (ToolStatusEvent::Error, "error"),
            (ToolStatusEvent::Aborted, "aborted"),
        ] {
            assert_eq!(status_to_str(variant), expected);
        }
    }

    /// `turn_complete.usage` is a nested `MessageUsage`, **not** a flat
    /// merge into the parent. Runbook step 2 asserts `usage.cacheMissTokens`.
    #[test]
    fn turn_complete_usage_is_nested() {
        let p = TurnCompletePayload {
            thread_id: "th_1".into(),
            message_id: "msg_1".into(),
            usage: MessageUsageDto {
                cache_read_tokens: 50,
                cache_miss_tokens: 70,
                cache_creation_tokens: 0,
                output_tokens: 350,
                cost_usd: 0.0,
            },
            agent_id: None,
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["threadId"], "th_1");
        assert_eq!(v["usage"]["cacheReadTokens"], 50);
        assert_eq!(v["usage"]["cacheMissTokens"], 70);
        assert_eq!(v["usage"]["cacheCreationTokens"], 0);
        assert_eq!(v["usage"]["outputTokens"], 350);
        // P3a task 3: cost still 0 here; task 4 wires real value through
        // EngineEvent::TurnComplete.cost_usd.
        assert_eq!(v["usage"]["costUsd"], 0.0);
    }

    /// `stream:error.messageId` is `Option<String>` — must serialize as
    /// explicit `null` (not omitted) per IPC §6 rule 2.
    #[test]
    fn stream_error_message_id_is_explicit_null() {
        let p = StreamErrorPayload {
            thread_id: "th_1".into(),
            message_id: None,
            error: "deepseek 502".into(),
            retryable: true,
        };
        let v = serde_json::to_value(&p).unwrap();
        assert!(
            v.as_object().unwrap().contains_key("messageId"),
            "messageId must appear in JSON even when None (no skip_serializing_if)"
        );
        assert_eq!(v["messageId"], serde_json::Value::Null);
        assert_eq!(v["retryable"], true);
    }

    /// `stream:aborted` is the simplest payload — just the two ids.
    #[test]
    fn stream_aborted_minimal_shape() {
        let p = StreamAbortedPayload {
            thread_id: "th_1".into(),
            message_id: "msg_1".into(),
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v.as_object().unwrap().len(), 2);
        assert_eq!(v["threadId"], "th_1");
        assert_eq!(v["messageId"], "msg_1");
    }

    /// `stream:todos_updated` carries `threadId` + a `todos` array whose
    /// items use camelCase keys and snake_case status strings — the shape
    /// the frontend todo panel expects.
    #[test]
    fn todos_updated_payload_shape() {
        let p = TodosUpdatedPayload {
            thread_id: "th_1".into(),
            todos: vec![
                TodoItemDto {
                    content: "Run tests".into(),
                    active_form: "Running tests".into(),
                    status: "in_progress".into(),
                },
                TodoItemDto {
                    content: "Ship it".into(),
                    active_form: "Shipping it".into(),
                    status: "pending".into(),
                },
            ],
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["threadId"], "th_1");
        assert_eq!(v["todos"][0]["content"], "Run tests");
        assert_eq!(v["todos"][0]["activeForm"], "Running tests");
        assert_eq!(v["todos"][0]["status"], "in_progress");
        assert_eq!(v["todos"][1]["status"], "pending");
        let back: TodosUpdatedPayload = serde_json::from_value(v).unwrap();
        assert_eq!(back.todos.len(), 2);
    }

    /// `stream:context_usage` carries usage ratio + source for the frontend
    /// progress ring. Verify camelCase field names.
    #[test]
    fn context_usage_payload_shape() {
        let p = ContextUsagePayload {
            thread_id: "th_1".into(),
            used_tokens: 750_000,
            max_tokens: 1_000_000,
            ratio: 0.75,
            source: "api".into(),
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["threadId"], "th_1");
        assert_eq!(v["usedTokens"], 750_000);
        assert_eq!(v["maxTokens"], 1_000_000);
        assert_eq!(v["ratio"], 0.75);
        assert_eq!(v["source"], "api");
        let back: ContextUsagePayload = serde_json::from_value(v).unwrap();
        assert_eq!(back, p);
    }

    /// `mcp:server_status_changed` carries name + status string + optional error.
    #[test]
    fn mcp_server_status_payload_shape() {
        let p = McpServerStatusPayload {
            name: "github".into(),
            status: "connected".into(),
            error: None,
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["name"], "github");
        assert_eq!(v["status"], "connected");
        assert!(
            v.as_object().unwrap().contains_key("error"),
            "error key present even when None"
        );
        assert_eq!(v["error"], serde_json::Value::Null);
    }

    /// P4 routing: the three UI-surfaced sub-agent event kinds
    /// (content_delta, tool_call_start, tool_call_update) must be re-routed
    /// to the **parent** thread id so the frontend can locate the loaded
    /// thread + owning `task` card. Without this, every chatStore reducer's
    /// `if (!thread) return s` guard drops the event (the sub-thread is never
    /// loaded into `threadsById`).
    #[test]
    fn sub_agent_surfaced_events_route_to_parent() {
        assert!(sub_agent_event_is_surfaced(&EngineEvent::ContentDelta {
            thread_id: "sub".into(),
            message_id: "m".into(),
            delta: "x".into(),
        }));
        assert!(sub_agent_event_is_surfaced(&EngineEvent::ToolCallStart {
            thread_id: "sub".into(),
            message_id: "m".into(),
            tool_use_id: "t".into(),
            tool_name: "read_file".into(),
            input: json!({}),
        }));
        assert!(sub_agent_event_is_surfaced(&EngineEvent::ToolCallUpdate {
            thread_id: "sub".into(),
            message_id: "m".into(),
            tool_use_id: "t".into(),
            status: ToolStatusEvent::Success,
            input: None,
            result: Some("ok".into()),
            duration_ms: Some(1),
            error_message: None,
        }));
    }

    /// P4 routing: the non-surfaced sub-agent event kinds must NOT be
    /// re-routed to the parent thread — routing e.g. TurnComplete to the
    /// parent would prematurely flip the parent's streaming message to
    /// complete. Keeping them on the (unloaded) sub-thread id makes the
    /// frontend ignore them safely.
    #[test]
    fn sub_agent_non_surfaced_events_stay_on_sub_thread() {
        assert!(!sub_agent_event_is_surfaced(&EngineEvent::ReasoningDelta {
            thread_id: "sub".into(),
            message_id: "m".into(),
            delta: "x".into(),
        }));
        assert!(!sub_agent_event_is_surfaced(&EngineEvent::TurnComplete {
            thread_id: "sub".into(),
            message_id: "m".into(),
            usage: Usage::default(),
            cost_usd: 0.0,
        }));
        assert!(!sub_agent_event_is_surfaced(&EngineEvent::Aborted {
            thread_id: "sub".into(),
            message_id: "m".into(),
        }));
        assert!(!sub_agent_event_is_surfaced(&EngineEvent::Error {
            thread_id: "sub".into(),
            message_id: None,
            error: "boom".into(),
            retryable: false,
        }));
        assert!(!sub_agent_event_is_surfaced(&EngineEvent::TodosUpdated {
            thread_id: "sub".into(),
            todos: vec![],
        }));
        assert!(!sub_agent_event_is_surfaced(&EngineEvent::ContextUsage {
            thread_id: "sub".into(),
            used_tokens: 1,
            max_tokens: 2,
            ratio: 0.5,
            source: deepseek_core::compaction::ContextUsageSource::Local,
        }));
    }
}
