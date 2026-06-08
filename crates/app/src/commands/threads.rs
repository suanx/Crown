//! Thread CRUD + search + export commands.

use chrono::{DateTime, Utc};
use deepseek_state::{MessageRepo, ThreadInsert, ThreadRepo, ThreadUpdate, UsageRepo};

use crate::dto::{
    BrainstormMessageMetaDto, CreateThreadInput, MessageDto, SegmentDto, ThreadDto,
    ThreadSummaryDto, ToolCallDto, UpdateThreadInput,
};
use crate::AppState;

/// Error-prefixes used to infer a historical tool card's status. ChatMessage
/// carries no is_error flag (see engine `append_tool_result_msg`), so we
/// detect the canonical error messages the engine writes on the failure path.
const TOOL_ERROR_PREFIXES: &[&str] = &[
    "<tool_use_error>",
    "Unknown tool:",
    "Invalid tool arguments",
    "Permission denied",
    "Tool '", // "Tool 'x' timed out after ..."
    "[用户已中止",
];

fn tool_result_is_error(content: &str) -> bool {
    TOOL_ERROR_PREFIXES.iter().any(|p| content.starts_with(p))
}

fn segments_for_message(
    content: &str,
    reasoning: Option<&str>,
    tool_calls: Option<&[ToolCallDto]>,
) -> Vec<SegmentDto> {
    let mut segments = Vec::new();
    if let Some(text) = reasoning.filter(|s| !s.is_empty()) {
        segments.push(SegmentDto::Reasoning {
            text: text.to_string(),
        });
    }
    if !content.is_empty() {
        segments.push(SegmentDto::Text {
            text: content.to_string(),
        });
    }
    if let Some(calls) = tool_calls {
        for call in calls {
            segments.push(SegmentDto::Tool {
                call_id: call.id.clone(),
                name: call.name.clone(),
                input: call.input.clone(),
                status: call.status.clone(),
                result: call.result.clone(),
                duration_ms: call.duration_ms,
                diff: call.diff.clone(),
                error_message: call.error_message.clone(),
            });
        }
    }
    segments
}

/// Convert persisted message rows into frontend DTOs, reconstructing tool
/// call cards: each assistant message's `tool_calls` become `ToolCallDto`s,
/// and the subsequent `role:"tool"` messages are merged in as their results
/// (paired by tool_call_id). Tool-role rows are consumed by this merge and
/// are NOT emitted as standalone messages.
fn assemble_message_dtos(rows: Vec<deepseek_state::MessageRow>) -> Vec<MessageDto> {
    use std::collections::HashMap;

    // First pass: collect tool results by tool_call_id.
    let mut tool_results: HashMap<String, String> = HashMap::new();
    for m in &rows {
        if m.role != "tool" {
            continue;
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&m.content_json).unwrap_or(serde_json::Value::Null);
        if let Some(id) = parsed.get("tool_call_id").and_then(|v| v.as_str()) {
            let content = parsed
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            tool_results.insert(id.to_string(), content);
        }
    }

    // Second pass: emit user/assistant/system messages; skip tool rows
    // (they were merged into the owning assistant card above).
    let mut out = Vec::new();
    for m in rows {
        if m.role == "tool" {
            continue;
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&m.content_json).unwrap_or(serde_json::Value::Null);
        let content = parsed
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        let reasoning = parsed
            .get("reasoning_content")
            .and_then(|c| c.as_str())
            .map(String::from);
        let brainstorm = parsed
            .get("brainstorm")
            .cloned()
            .and_then(|v| serde_json::from_value::<BrainstormMessageMetaDto>(v).ok());

        let tool_calls = parsed
            .get("tool_calls")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|tc| {
                        let id = tc
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let name = tc
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let args_str = tc
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("{}");
                        let input: serde_json::Value =
                            serde_json::from_str(args_str).unwrap_or(serde_json::Value::Null);
                        let result = tool_results.get(&id).cloned();
                        let is_error = result.as_deref().map(tool_result_is_error).unwrap_or(false);
                        ToolCallDto {
                            id,
                            name,
                            input,
                            status: if is_error {
                                "error".into()
                            } else {
                                "success".into()
                            },
                            result: if is_error { None } else { result.clone() },
                            duration_ms: None,
                            diff: None,
                            error_message: if is_error { result } else { None },
                        }
                    })
                    .collect::<Vec<_>>()
            });

        let segments = segments_for_message(&content, reasoning.as_deref(), tool_calls.as_deref());

        out.push(MessageDto {
            id: m.id.to_string(),
            thread_id: m.thread_id,
            seq: m.seq,
            role: m.role,
            content,
            timestamp: DateTime::<Utc>::from_timestamp_millis(m.created_at)
                .unwrap_or_else(|| {
                    DateTime::<Utc>::from_timestamp(0, 0).expect("epoch is always representable")
                })
                .to_rfc3339(),
            reasoning,
            tool_calls,
            segments,
            usage: None,
            is_streaming: false,
            interrupted: false,
            brainstorm,
            attachments: vec![],
        });
    }
    out
}

#[tauri::command]
pub async fn list_threads(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ThreadSummaryDto>, String> {
    let repo = ThreadRepo::new(state.db.as_ref());
    let summaries = repo.list().map_err(|e| e.to_string())?;
    Ok(summaries.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn get_thread(
    state: tauri::State<'_, AppState>,
    thread_id: String,
) -> Result<ThreadDto, String> {
    let trepo = ThreadRepo::new(state.db.as_ref());
    let mrepo = MessageRepo::new(state.db.as_ref());
    let urepo = UsageRepo::new(state.db.as_ref());
    let thread = trepo.get(&thread_id).map_err(|e| e.to_string())?;
    let messages = mrepo
        .load_by_thread(&thread_id)
        .map_err(|e| e.to_string())?;
    // Aggregate per-thread cost from the usage table. Best-effort: a query
    // failure logs and falls back to 0 rather than failing the whole
    // get_thread call (the rest of the thread payload is still useful).
    let cost_usd = urepo.thread_cost(&thread_id).unwrap_or_else(|e| {
        tracing::warn!(error = %e, %thread_id, "thread_cost lookup failed");
        0.0
    });

    let mut dto: ThreadDto = thread.into();
    dto.cost_usd = cost_usd;
    dto.messages = assemble_message_dtos(messages);
    Ok(dto)
}

#[tauri::command]
pub async fn create_thread(
    state: tauri::State<'_, AppState>,
    input: Option<CreateThreadInput>,
) -> Result<ThreadSummaryDto, String> {
    let input = input.unwrap_or_default();
    let provider_id = input
        .provider_id
        .unwrap_or_else(crate::commands::config::read_default_provider_id_pub);
    let model = input
        .model
        .unwrap_or_else(crate::commands::config::read_default_model_pub);
    let thinking_effort = input.thinking_effort.unwrap_or_else(|| "medium".into());
    let repo = ThreadRepo::new(state.db.as_ref());
    let thread = repo
        .create(ThreadInsert {
            name: None,
            model,
            cwd: input.cwd,
            permission_mode: "default".into(),
            provider_id,
            thinking_effort: Some(thinking_effort),
            parent_thread_id: None,
            project_id: input.project_id,
        })
        .map_err(|e| e.to_string())?;
    let summary = deepseek_state::ThreadSummary {
        id: thread.id.clone(),
        name: thread.name,
        updated_at: thread.updated_at,
        preview: None,
        is_pinned: false,
        message_count: 0,
        provider_id: thread.provider_id,
        project_id: thread.project_id,
    };
    Ok(summary.into())
}

#[tauri::command]
pub async fn update_thread(
    state: tauri::State<'_, AppState>,
    input: UpdateThreadInput,
) -> Result<(), String> {
    let repo = ThreadRepo::new(state.db.as_ref());
    let mut upd = ThreadUpdate::default();
    if let Some(title) = input.title {
        upd.name = Some(Some(title));
    }
    if let Some(pin) = input.is_pinned {
        upd.is_pinned = Some(pin);
    }
    if let Some(project_id) = input.project_id {
        upd.project_id = Some(project_id);
    }
    if let Some(mode) = input.permission_mode {
        upd.permission_mode = Some(mode.as_str().to_string());
        // Sync in-memory cache so the next turn picks up the new mode.
        if let Some(s) = state.engine.cache().get(&input.thread_id) {
            s.permission_ctx.write().mode = mode;
        }
    }
    if let Some(effort) = input.thinking_effort {
        upd.thinking_effort = Some(effort.clone());
        if let Some(s) = state.engine.cache().get(&input.thread_id) {
            *s.thinking_effort.write() = effort;
        }
    }
    repo.update(&input.thread_id, upd)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn delete_thread(
    state: tauri::State<'_, AppState>,
    thread_id: String,
) -> Result<(), String> {
    state.engine.cache().remove(&thread_id);
    let repo = ThreadRepo::new(state.db.as_ref());
    repo.delete(&thread_id).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn search_threads(
    state: tauri::State<'_, AppState>,
    query: String,
) -> Result<Vec<ThreadSummaryDto>, String> {
    let repo = ThreadRepo::new(state.db.as_ref());
    let hits = repo.search(&query).map_err(|e| e.to_string())?;
    Ok(hits.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn export_thread(
    state: tauri::State<'_, AppState>,
    thread_id: String,
) -> Result<String, String> {
    let trepo = ThreadRepo::new(state.db.as_ref());
    let mrepo = MessageRepo::new(state.db.as_ref());
    let thread = trepo.get(&thread_id).map_err(|e| e.to_string())?;
    let messages = mrepo
        .load_by_thread(&thread_id)
        .map_err(|e| e.to_string())?;
    let mut out = format!("# {}\n\n", thread.name.unwrap_or_else(|| "Untitled".into()));
    for m in messages {
        out.push_str(&format!("## {}\n\n", m.role));
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&m.content_json) {
            if let Some(content) = v.get("content").and_then(|c| c.as_str()) {
                out.push_str(content);
                out.push_str("\n\n");
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deepseek_state::MessageRow;

    fn row(id: i64, seq: i64, role: &str, json: &str) -> MessageRow {
        MessageRow {
            id,
            thread_id: "t1".into(),
            seq,
            role: role.into(),
            content_json: json.into(),
            created_at: 1_700_000_000_000,
        }
    }

    #[test]
    fn assistant_tool_calls_become_cards_with_results() {
        let rows = vec![
            row(1, 0, "user", r#"{"role":"user","content":"hi"}"#),
            row(
                2,
                1,
                "assistant",
                r#"{"role":"assistant","content":"working","tool_calls":[
                    {"id":"call_a","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"x.rs\"}"}},
                    {"id":"call_b","type":"function","function":{"name":"grep","arguments":"{\"pattern\":\"fn\"}"}}
                ]}"#,
            ),
            row(
                3,
                2,
                "tool",
                r#"{"role":"tool","content":"file contents here","tool_call_id":"call_a"}"#,
            ),
            row(
                4,
                3,
                "tool",
                r#"{"role":"tool","content":"3 matches","tool_call_id":"call_b"}"#,
            ),
            row(
                5,
                4,
                "assistant",
                r#"{"role":"assistant","content":"done"}"#,
            ),
        ];
        let dtos = assemble_message_dtos(rows);

        assert_eq!(
            dtos.len(),
            3,
            "got: {:?}",
            dtos.iter().map(|d| &d.role).collect::<Vec<_>>()
        );

        let assistant = &dtos[1];
        assert_eq!(assistant.role, "assistant");
        let calls = assistant
            .tool_calls
            .as_ref()
            .expect("assistant has tool_calls");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].input["path"], "x.rs");
        assert_eq!(calls[0].result.as_deref(), Some("file contents here"));
        assert_eq!(calls[0].status, "success");
        assert_eq!(calls[1].name, "grep");
        assert_eq!(calls[1].result.as_deref(), Some("3 matches"));

        assert_eq!(dtos[0].role, "user");
        assert_eq!(dtos[2].content, "done");
    }

    #[test]
    fn tool_result_without_matching_call_is_dropped() {
        let rows = vec![row(
            1,
            0,
            "tool",
            r#"{"role":"tool","content":"orphan","tool_call_id":"ghost"}"#,
        )];
        let dtos = assemble_message_dtos(rows);
        assert_eq!(dtos.len(), 0);
    }

    #[test]
    fn error_prefixed_tool_result_marks_card_error() {
        let rows = vec![
            row(
                1,
                0,
                "assistant",
                r#"{"role":"assistant","tool_calls":[
                    {"id":"c1","type":"function","function":{"name":"edit_file","arguments":"{}"}}
                ]}"#,
            ),
            row(
                2,
                1,
                "tool",
                r#"{"role":"tool","content":"Unknown tool: edit_file","tool_call_id":"c1"}"#,
            ),
        ];
        let dtos = assemble_message_dtos(rows);
        assert_eq!(dtos.len(), 1);
        let calls = dtos[0].tool_calls.as_ref().unwrap();
        assert_eq!(calls[0].status, "error");
        assert_eq!(
            calls[0].error_message.as_deref(),
            Some("Unknown tool: edit_file")
        );
    }

    #[test]
    fn structured_tool_error_marks_card_error() {
        let rows = vec![
            row(
                1,
                0,
                "assistant",
                r#"{"role":"assistant","tool_calls":[
                    {"id":"c1","type":"function","function":{"name":"read_file","arguments":"{}"}}
                ]}"#,
            ),
            row(
                2,
                1,
                "tool",
                r#"{"role":"tool","content":"<tool_use_error>\n错误: No such file\n分类: path_not_found\n恢复建议: 先列出父目录\n</tool_use_error>","tool_call_id":"c1"}"#,
            ),
        ];
        let dtos = assemble_message_dtos(rows);
        assert_eq!(dtos.len(), 1);
        let calls = dtos[0].tool_calls.as_ref().unwrap();
        assert_eq!(calls[0].status, "error");
        assert!(calls[0]
            .error_message
            .as_deref()
            .unwrap()
            .contains("path_not_found"));
    }

    #[test]
    fn assistant_without_tool_calls_has_none() {
        let rows = vec![row(
            1,
            0,
            "assistant",
            r#"{"role":"assistant","content":"just text","reasoning_content":"thinking"}"#,
        )];
        let dtos = assemble_message_dtos(rows);
        assert_eq!(dtos.len(), 1);
        assert!(dtos[0].tool_calls.is_none());
        assert_eq!(dtos[0].reasoning.as_deref(), Some("thinking"));
        assert_eq!(dtos[0].content, "just text");
    }
}
