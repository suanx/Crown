use base64::engine::general_purpose::STANDARD as BASE64;

use base64::Engine;


use serde::{Deserialize, Serialize};

// ─── Chat Message ────────────────────────────────────────────────────────────

// ─── Multimodal Content Parts ───────────────────────────────────────────

/// A single part of a multimodal message content array.
/// Follows the OpenAI vision API format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrl },
}

/// An image URL or base64 data URI within a multimodal message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    /// A URL or data URI (e.g. `data:image/png;base64,<base64>`)
    pub url: String,
}

impl ImageUrl {
    /// Create an ImageUrl from raw image bytes with the given MIME type.
    pub fn from_bytes(bytes: &[u8], mime: &str) -> Self {
        let b64 = BASE64.encode(bytes);
        Self {
            url: format!("data:{};base64,{}", mime, b64),
        }
    }
}

/// Message content — either plain text or a multimodal array of parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Simple text content (default, backward compatible).
    Text(String),
    /// Multimodal content (text + images).
    MultiPart(Vec<ContentPart>),
}

impl MessageContent {
    /// Extract text if this is a plain text message.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            MessageContent::Text(s) => Some(s.as_str()),
            MessageContent::MultiPart(_) => None,
        }
    }

    /// Convert to text, returning empty string if multimodal.
    pub fn into_text_lossy(self) -> String {
        match self {
            MessageContent::Text(s) => s,
            MessageContent::MultiPart(_) => String::new(),
        }
    }
}

// ─── Chat Message ────────────────────────────────────────────────────────────

/// A chat message in the conversation.
///
/// `content` can be either a plain string (backwards compatible) or
/// a multimodal array of text/image parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<MessageContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    /// Create a simple user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: Some(MessageContent::Text(content.into())),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: Some(MessageContent::Text(content.into())),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: Some(MessageContent::Text(content.into())),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create a user message with multimodal content (text + images).
    pub fn user_with_images(text: impl Into<String>, images: Vec<ContentPart>) -> Self {
        let mut parts = Vec::new();
        let t = text.into();
        if !t.is_empty() {
            parts.push(ContentPart::Text { text: t });
        }
        parts.extend(images);
        Self {
            role: "user".to_string(),
            content: Some(MessageContent::MultiPart(parts)),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Get the content as a text string, if it is plain text.
    pub fn content_text(&self) -> Option<&str> {
        self.content.as_ref().and_then(|c| c.as_text())
    }
}

// ─── Tool Call ────────────────────────────────────────────────────────────────

/// A tool call made by the assistant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

/// The function name and arguments within a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

// ─── Tool Spec ────────────────────────────────────────────────────────────────

/// Specification of a tool available to the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionSpec,
}

/// Specification of a function within a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

// ─── Chat Request ─────────────────────────────────────────────────────────────

/// Request payload for the chat completions API.
#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolSpec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    /// DeepSeek-specific request extensions. Omitted entirely when `None`
    /// (the provider-neutral default) so non-DeepSeek endpoints never see
    /// a field they would reject.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_body: Option<ExtraBody>,
}

/// DeepSeek `extra_body` request extension. Only the fields we use are
/// modeled; all are optional and skip-when-`None` so the serialized object
/// stays minimal.
///
/// ⚠️ Provider-coupling: this whole struct is DeepSeek-specific. Callers
/// must only populate it when the active provider is DeepSeek (see
/// `.kiro/steering/provider-neutrality.md`). The default (`None` on
/// [`ChatRequest::extra_body`]) is the safe cross-provider behavior.
#[derive(Debug, Clone, Serialize)]
pub struct ExtraBody {
    /// V4 thinking-mode toggle. Lives under `extra_body.thinking.type` per
    /// DeepSeek docs (NOT a top-level field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
}

/// DeepSeek V4 thinking-mode configuration object.
#[derive(Debug, Clone, Serialize)]
pub struct ThinkingConfig {
    /// `"enabled"` or `"disabled"`.
    #[serde(rename = "type")]
    pub thinking_type: String,
}

/// Options for streaming responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamOptions {
    pub include_usage: bool,
}

// ─── Usage ────────────────────────────────────────────────────────────────────

/// Token usage information from the API response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    #[serde(default)]
    pub prompt_cache_hit_tokens: u32,
    #[serde(default)]
    pub prompt_cache_miss_tokens: u32,
}

impl Usage {
    /// Returns the ratio of cache hits to total prompt tokens.
    /// Returns 0.0 if prompt_tokens is 0.
    pub fn cache_hit_ratio(&self) -> f64 {
        if self.prompt_tokens == 0 {
            0.0
        } else {
            self.prompt_cache_hit_tokens as f64 / self.prompt_tokens as f64
        }
    }
}

// ─── Streaming Types ──────────────────────────────────────────────────────────

/// A parsed stream chunk with extracted deltas.
#[derive(Debug, Clone, Default)]
pub struct StreamChunk {
    pub content_delta: Option<String>,
    pub reasoning_delta: Option<String>,
    pub tool_call_delta: Option<ToolCallDelta>,
    pub usage: Option<Usage>,
    pub finish_reason: Option<String>,
}

/// Delta for an in-progress tool call streamed incrementally.
#[derive(Debug, Clone)]
pub struct ToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    pub name: Option<String>,
    pub arguments_delta: Option<String>,
}

// ─── Chat Response (aggregated) ───────────────────────────────────────────────

/// Aggregated response after streaming completes.
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub reasoning_content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Usage,
}

// ─── Raw API response structures (for deserialization) ────────────────────────

/// Raw SSE chunk from the DeepSeek API (matches the actual JSON structure).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawStreamChunk {
    #[allow(dead_code)]
    pub id: Option<String>,
    pub choices: Vec<RawStreamChoice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawStreamChoice {
    pub delta: RawDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawDelta {
    pub content: Option<String>,
    pub reasoning_content: Option<String>,
    pub tool_calls: Option<Vec<RawToolCallDelta>>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    pub function: Option<RawFunctionDelta>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawFunctionDelta {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

#[cfg(test)]
mod request_shape_tests {
    use super::*;

    fn base_request() -> ChatRequest {
        ChatRequest {
            model: "deepseek-v4-flash".into(),
            messages: vec![ChatMessage::user("hi")],
            tools: None,
            temperature: None,
            max_tokens: None,
            reasoning_effort: None,
            stream: Some(false),
            stream_options: None,
            thinking: None,
            extra_body: None,
        }
    }

    /// Provider-neutral default: no `extra_body` key in the serialized
    /// request when `extra_body` is `None`. Non-DeepSeek endpoints must
    /// never see DeepSeek-specific fields.
    #[test]
    fn extra_body_omitted_by_default() {
        let req = base_request();
        let v = serde_json::to_value(&req).unwrap();
        assert!(
            !v.as_object().unwrap().contains_key("extra_body"),
            "extra_body must be absent by default, got: {v}"
        );
    }

    /// DeepSeek thinking toggle serializes under `extra_body.thinking.type`
    /// (NOT a top-level `thinking` field) per DeepSeek docs.
    #[test]
    fn thinking_serializes_under_extra_body() {
        let mut req = base_request();
        req.extra_body = Some(ExtraBody {
            thinking: Some(ThinkingConfig {
                thinking_type: "disabled".into(),
            }),
        });
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["extra_body"]["thinking"]["type"], "disabled");
        // Must NOT leak as a top-level field.
        assert!(v.as_object().unwrap().get("thinking").is_none());
    }

    #[test]
    fn top_level_thinking_serializes_for_compatible_providers() {
        let mut req = base_request();
        req.thinking = Some(ThinkingConfig {
            thinking_type: "enabled".into(),
        });
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["thinking"]["type"], "enabled");
    }

    #[test]
    fn reasoning_effort_serializes_top_level() {
        let mut req = base_request();
        req.reasoning_effort = Some("max".into());
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["reasoning_effort"], "max");
    }
}
