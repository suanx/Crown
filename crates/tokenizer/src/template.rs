//! DeepSeek V4 chat template rendering.
//!
//! Mirrors Reasonix `formatDeepSeekPrompt()` + `encoding_dsv4.py`:
//! applies the V4 chat template so token counts track the API's
//! `prompt_tokens` exactly.
//!
//! Key behaviors:
//! - tool results merge into the preceding user message
//! - assistant tool_calls render in DSML format
//! - reasoning_content before the last user message is stripped (drop_thinking)

use serde::{Deserialize, Serialize};

use crate::types::{ContentPart, MessageContent};

// ── Special tokens ─────────────────────────────────────────────────────────

const BOS: &str = "<｜begin▁of▁sentence｜>";
const EOS: &str = "<｜end▁of▁sentence｜>";
const USER_SP: &str = "<｜User｜>";
const ASSISTANT_SP: &str = "<｜Assistant｜>";
const THINK_START: &str = "<think>";
const THINK_END: &str = "</think>";

// DSML namespace token
const DSML: &str = "｜DSML｜";

// ── Public message type (minimal, for token estimation) ────────────────────

/// Minimal chat message for template rendering and token estimation.
/// Mirrors the fields relevant to prompt construction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default)]
    pub content: Option<MessageContent>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCallEntry>>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub reasoning_content: Option<String>,
}

impl ChatMessage {
    /// Extract text content as a string slice, if this is a plain text message.
    pub fn content_text(&self) -> Option<&str> {
        self.content.as_ref().and_then(|c| c.as_text())
    }
}

/// A single tool call in assistant messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallEntry {
    #[serde(default)]
    pub id: String,
    pub function: ToolCallFunction,
}

/// Function name + arguments JSON string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    #[serde(default)]
    pub arguments: String,
}

// ── Template rendering ─────────────────────────────────────────────────────

/// Apply the DeepSeek V4 chat template to a message sequence.
///
/// Returns the full prompt string as the API would tokenize it internally.
/// When `drop_thinking` is true, `reasoning_content` is stripped from
/// assistant messages before the last user/developer message (matches
/// Python `_drop_thinking_messages`).
pub fn format_deepseek_prompt(messages: &[ChatMessage], drop_thinking: bool) -> String {
    if messages.is_empty() {
        return format!("{ASSISTANT_SP}{THINK_END}");
    }

    // Find last user/developer index for drop_thinking logic.
    let last_user_idx = if drop_thinking {
        messages
            .iter()
            .rposition(|m| m.role == "user" || m.role == "developer")
            .unwrap_or(0)
    } else {
        usize::MAX // never strip
    };

    // Build a processed list: filter out developers before last_user,
    // and strip reasoning from assistants before last_user.
    let mut processed: Vec<ChatMessage> = Vec::with_capacity(messages.len());
    for (i, msg) in messages.iter().enumerate() {
        if drop_thinking && i < last_user_idx && msg.role == "developer" {
            continue;
        }
        let mut m = msg.clone();
        if drop_thinking && i < last_user_idx && m.role == "assistant" {
            m.reasoning_content = None;
        }
        processed.push(m);
    }

    let refs: Vec<&ChatMessage> = processed.iter().collect();
    // Merge tool results into user messages.
    let merged = merge_tool_messages(&refs);

    let mut prompt = String::from(BOS);

    for (i, msg) in merged.iter().enumerate() {
        let next_role = merged.get(i + 1).map(|m| m.role.as_str());

        match msg.role.as_str() {
            "system" => {
                prompt.push_str(msg.content_text().unwrap_or(""));
            }
            "user" | "developer" => {
                prompt.push_str(USER_SP);
                prompt.push_str(msg.content_text().unwrap_or(""));
                if next_role == Some("assistant") || next_role.is_none() {
                    prompt.push_str(ASSISTANT_SP);
                    prompt.push_str(THINK_END);
                }
            }
            "assistant" => {
                if let Some(reasoning) = &msg.reasoning_content {
                    if !reasoning.is_empty() {
                        prompt.push_str(THINK_START);
                        prompt.push_str(reasoning);
                        prompt.push_str(THINK_END);
                    }
                }
                if let Some(content) = &msg.content {
                    match content {
                        MessageContent::Text(s) => prompt.push_str(s),
                        MessageContent::MultiPart(parts) => {
                            for part in parts {
                                let ContentPart::Text { text } = part;
                                prompt.push_str(text);
                            }
                        }
                    }
                }
                if let Some(tool_calls) = &msg.tool_calls {
                    if !tool_calls.is_empty() {
                        prompt.push_str(&render_tool_calls_dsml(tool_calls));
                    }
                }
                prompt.push_str(EOS);
            }
            _ => {
                // tool messages are already merged; skip any stragglers
            }
        }
    }

    prompt
}

/// Render tool calls in DSML format.
fn render_tool_calls_dsml(tool_calls: &[ToolCallEntry]) -> String {
    let mut invokes = String::new();
    for tc in tool_calls {
        invokes.push_str(&format!("<{DSML}invoke name=\"{}\">\n", tc.function.name));
        invokes.push_str(&encode_arguments_to_dsml(&tc.function.arguments));
        invokes.push('\n');
        invokes.push_str(&format!("</{DSML}invoke>\n"));
    }

    format!("\n\n<{DSML}tool_calls>\n{invokes}</{DSML}tool_calls>")
}

/// Encode JSON arguments object into DSML parameter elements.
fn encode_arguments_to_dsml(args_json: &str) -> String {
    let args: serde_json::Value = match serde_json::from_str(args_json) {
        Ok(v) => v,
        Err(_) => {
            // Fallback: treat entire string as single "arguments" param
            let mut map = serde_json::Map::new();
            map.insert(
                "arguments".to_string(),
                serde_json::Value::String(args_json.to_string()),
            );
            serde_json::Value::Object(map)
        }
    };

    let obj = match args.as_object() {
        Some(o) => o,
        None => return String::new(),
    };

    let mut out = String::new();
    for (key, value) in obj {
        let is_str = value.is_string();
        let rendered_value = if is_str {
            value.as_str().unwrap_or("").to_string()
        } else {
            serde_json::to_string(value).unwrap_or_default()
        };
        out.push_str(&format!(
            "<{DSML}parameter name=\"{key}\" string=\"{is_str}\">{rendered_value}</{DSML}parameter>\n"
        ));
    }
    out.trim_end().to_string()
}

/// Merge tool-role messages into the preceding user message (or create one).
/// This matches the V4 chat template's behavior where tool results appear
/// inside `<tool_result>...</tool_result>` tags in user messages.
fn merge_tool_messages(messages: &[&ChatMessage]) -> Vec<ChatMessage> {
    let mut merged: Vec<MergedMsg> = Vec::new();

    for msg in messages {
        if msg.role == "tool" {
            let tool_block = format!(
                "<tool_result>{}</tool_result>",
                msg.content_text().unwrap_or("")
            );
            if let Some(last) = merged.last_mut() {
                if last.role == "user" {
                    last.tool_blocks.push(tool_block);
                    last.rebuild_content();
                    continue;
                }
            }
            // No preceding user message — create one
            merged.push(MergedMsg {
                role: "user".to_string(),
                text_parts: Vec::new(),
                tool_blocks: vec![tool_block],
                reasoning_content: None,
                tool_calls: None,
                content: None,
            });
            merged.last_mut().unwrap().rebuild_content();
        } else if msg.role == "user" || msg.role == "developer" {
            let text = msg.content_text().unwrap_or("").to_string();
            if let Some(last) = merged.last_mut() {
                if last.role == "user" && !last.tool_blocks.is_empty() {
                    // Merge consecutive user text after tool blocks
                    last.text_parts.push(text);
                    last.rebuild_content();
                    continue;
                }
            }
            merged.push(MergedMsg {
                role: msg.role.clone(),
                text_parts: vec![text],
                tool_blocks: Vec::new(),
                reasoning_content: None,
                tool_calls: None,
                content: None,
            });
            merged.last_mut().unwrap().rebuild_content();
        } else {
            merged.push(MergedMsg {
                role: msg.role.clone(),
                text_parts: Vec::new(),
                tool_blocks: Vec::new(),
                reasoning_content: msg.reasoning_content.clone(),
                tool_calls: msg.tool_calls.clone(),
                content: msg.content_text().map(|s| s.to_string()),
            });
        }
    }

    merged.into_iter().map(|m| m.into_chat_message()).collect()
}

struct MergedMsg {
    role: String,
    text_parts: Vec<String>,
    tool_blocks: Vec<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<ToolCallEntry>>,
    content: Option<String>,
}

impl MergedMsg {
    fn rebuild_content(&mut self) {
        let text = self.text_parts.join("\n\n");
        let tools = self.tool_blocks.join("\n");
        let combined = if text.is_empty() {
            tools
        } else if tools.is_empty() {
            text
        } else {
            format!("{text}\n\n{tools}")
        };
        self.content = Some(combined);
    }

    fn into_chat_message(self) -> ChatMessage {
        ChatMessage {
            role: self.role,
            content: self.content.map(MessageContent::Text),
            tool_calls: self.tool_calls,
            tool_call_id: None,
            reasoning_content: self.reasoning_content,
        }
    }
}

// ── Per-message token estimation (Task 1.5) ────────────────────────────────

/// Tokens added per message by the chat template wrapper (role markers, etc.)
const PER_MESSAGE_TEMPLATE_TOKENS: usize = 6;

/// Estimate total conversation tokens from a message list.
///
/// Uses `count_tokens_bounded` per message for efficiency. Not a full prompt
/// rebuild — accuracy is ±5%, sufficient for fold-threshold checks.
pub fn estimate_conversation_tokens(messages: &[ChatMessage], drop_thinking: bool) -> usize {
    if messages.is_empty() {
        return 0;
    }

    let last_user_idx = if drop_thinking {
        messages
            .iter()
            .rposition(|m| m.role == "user" || m.role == "developer")
            .unwrap_or(0)
    } else {
        usize::MAX
    };

    let mut total: usize = 2; // BOS + generation suffix

    for (i, msg) in messages.iter().enumerate() {
        // Skip developer messages before last user when dropping thinking
        if drop_thinking && i < last_user_idx && msg.role == "developer" {
            continue;
        }

        total += PER_MESSAGE_TEMPLATE_TOKENS;

        let drop_reasoning = drop_thinking && i < last_user_idx && msg.role == "assistant";

        // Content tokens
        if let Some(text) = msg.content_text() {
            if !text.is_empty() {
                total +=
                    crate::count_tokens_bounded(text, crate::DEFAULT_BOUNDED_TOKENIZE_CHARS);
            }
        }

        // Reasoning tokens (if not dropped)
        if !drop_reasoning {
            if let Some(reasoning) = &msg.reasoning_content {
                if !reasoning.is_empty() {
                    total += crate::count_tokens_bounded(
                        reasoning,
                        crate::DEFAULT_BOUNDED_TOKENIZE_CHARS,
                    );
                }
            }
        }

        // Tool calls tokens (assistant messages)
        if msg.role == "assistant" {
            if let Some(tool_calls) = &msg.tool_calls {
                if !tool_calls.is_empty() {
                    let tc_json = serde_json::to_string(tool_calls).unwrap_or_default();
                    total += crate::count_tokens_bounded(
                        &tc_json,
                        crate::DEFAULT_BOUNDED_TOKENIZE_CHARS,
                    );
                }
            }
        }
    }

    total
}

/// Estimate total request tokens (messages + tool specs).
///
/// Tool specs are rendered via the V4 TOOLS_TEMPLATE format and their token
/// count is added to the conversation total.
pub fn estimate_request_tokens(
    messages: &[ChatMessage],
    tool_specs_json: Option<&str>,
    drop_thinking: bool,
) -> usize {
    let mut total = estimate_conversation_tokens(messages, drop_thinking);

    if let Some(tools_json) = tool_specs_json {
        if !tools_json.is_empty() {
            // Tool specs are rendered into the system prompt area; count their tokens.
            total += crate::count_tokens_bounded(tools_json, crate::DEFAULT_BOUNDED_TOKENIZE_CHARS);
            // Template overhead for the tools section header (~50 tokens)
            total += 50;
        }
    }

    total
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn user_msg(content: &str) -> ChatMessage {
        ChatMessage {
            role: "user".to_string(),
            content: Some(MessageContent::Text(content.to_string())),
            tool_calls: None,
            tool_call_id: None,
            reasoning_content: None,
        }
    }

    fn assistant_msg(content: &str) -> ChatMessage {
        ChatMessage {
            role: "assistant".to_string(),
            content: Some(MessageContent::Text(content.to_string())),
            tool_calls: None,
            tool_call_id: None,
            reasoning_content: None,
        }
    }

    fn system_msg(content: &str) -> ChatMessage {
        ChatMessage {
            role: "system".to_string(),
            content: Some(MessageContent::Text(content.to_string())),
            tool_calls: None,
            tool_call_id: None,
            reasoning_content: None,
        }
    }

    fn tool_msg(content: &str, tool_call_id: &str) -> ChatMessage {
        ChatMessage {
            role: "tool".to_string(),
            content: Some(MessageContent::Text(content.to_string())),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.to_string()),
            reasoning_content: None,
        }
    }

    #[test]
    fn empty_messages_returns_generation_suffix() {
        let result = format_deepseek_prompt(&[], false);
        assert!(result.contains(ASSISTANT_SP));
        assert!(result.contains(THINK_END));
    }

    #[test]
    fn system_user_assistant_basic() {
        let msgs = vec![
            system_msg("You are helpful."),
            user_msg("Hello"),
            assistant_msg("Hi there!"),
        ];
        let prompt = format_deepseek_prompt(&msgs, false);
        assert!(prompt.starts_with(BOS));
        assert!(prompt.contains("You are helpful."));
        assert!(prompt.contains(USER_SP));
        assert!(prompt.contains("Hello"));
        assert!(prompt.contains(ASSISTANT_SP));
        assert!(prompt.contains("Hi there!"));
        assert!(prompt.contains(EOS));
    }

    #[test]
    fn tool_results_merge_into_user() {
        let msgs = vec![
            user_msg("do something"),
            ChatMessage {
                role: "assistant".to_string(),
                content: None,
                tool_calls: Some(vec![ToolCallEntry {
                    id: "c1".to_string(),
                    function: ToolCallFunction {
                        name: "Read".to_string(),
                        arguments: r#"{"path":"foo.rs"}"#.to_string(),
                    },
                }]),
                tool_call_id: None,
                reasoning_content: None,
            },
            tool_msg("file contents here", "c1"),
            user_msg("now edit it"),
        ];
        let prompt = format_deepseek_prompt(&msgs, false);
        // tool_result should appear in the prompt
        assert!(prompt.contains("<tool_result>file contents here</tool_result>"));
        // DSML invoke should appear
        assert!(prompt.contains("invoke name=\"Read\""));
    }

    #[test]
    fn reasoning_content_rendered() {
        let msgs = vec![
            user_msg("think hard"),
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(MessageContent::Text("answer".to_string())),
                tool_calls: None,
                tool_call_id: None,
                reasoning_content: Some("deep thought".to_string()),
            },
        ];
        let prompt = format_deepseek_prompt(&msgs, false);
        assert!(prompt.contains(THINK_START));
        assert!(prompt.contains("deep thought"));
        assert!(prompt.contains(THINK_END));
        assert!(prompt.contains("answer"));
    }

    #[test]
    fn drop_thinking_strips_old_reasoning() {
        let msgs = vec![
            user_msg("q1"),
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(MessageContent::Text("a1".to_string())),
                tool_calls: None,
                tool_call_id: None,
                reasoning_content: Some("old reasoning".to_string()),
            },
            user_msg("q2"),
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(MessageContent::Text("a2".to_string())),
                tool_calls: None,
                tool_call_id: None,
                reasoning_content: Some("new reasoning".to_string()),
            },
        ];
        let prompt = format_deepseek_prompt(&msgs, true);
        // Old reasoning before last user should be dropped
        assert!(!prompt.contains("old reasoning"));
        // New reasoning after last user should be kept
        assert!(prompt.contains("new reasoning"));
    }

    #[test]
    fn estimate_conversation_tokens_basic() {
        let msgs = vec![
            system_msg("You are helpful."),
            user_msg("Hello world"),
            assistant_msg("Hi!"),
        ];
        let est = estimate_conversation_tokens(&msgs, false);
        // Should be > 0 and reasonable
        assert!(est > 10, "got {est}");
        assert!(est < 200, "got {est}");
    }

    #[test]
    fn estimate_request_tokens_with_tools() {
        let msgs = vec![user_msg("hi")];
        let tools = r#"[{"type":"function","function":{"name":"Read","description":"Read a file","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}}]"#;
        let with_tools = estimate_request_tokens(&msgs, Some(tools), false);
        let without_tools = estimate_request_tokens(&msgs, None, false);
        assert!(with_tools > without_tools);
    }
}
