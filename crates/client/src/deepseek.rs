use std::pin::Pin;
use std::time::Duration;

use anyhow::{anyhow, Result};
use futures::stream::{self, Stream};
use reqwest::Client;
use serde::Deserialize;

use crate::retry::{fetch_with_retry, RetryConfig};
use crate::streaming::parse_sse_stream;
use crate::types::{
    ChatMessage, ChatRequest, ChatResponse, ExtraBody, StreamChunk, StreamOptions, ThinkingConfig,
    ToolCall, ToolSpec, Usage,
};

// ─── Config ───────────────────────────────────────────────────────────────────

/// Configuration for constructing a `DeepSeekClient`.
#[derive(Debug, Clone)]
pub struct DeepSeekClientConfig {
    pub api_key: String,
    pub base_url: String,
    pub timeout: Duration,
    pub retry: RetryConfig,
}

impl Default for DeepSeekClientConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.deepseek.com".to_string(),
            timeout: Duration::from_secs(660), // DeepSeek queue can take ~10 min
            retry: RetryConfig::default(),
        }
    }
}

// ─── Client ───────────────────────────────────────────────────────────────────

/// Options for a non-streaming [`DeepSeekClient::chat_with_opts`] call.
///
/// `Default` yields a provider-neutral request (no tools, no `extra_body`),
/// identical in shape to a vanilla OpenAI-style chat completion.
#[derive(Debug, Clone, Default)]
pub struct ChatOpts {
    /// Tool specs to include. Empty → field omitted from the request.
    pub tools: Vec<ToolSpec>,
    /// DeepSeek-specific request extension. MUST stay `None` for non-DeepSeek
    /// providers (see `.kiro/steering/provider-neutrality.md`).
    pub extra_body: Option<ExtraBody>,
    /// OpenAI-compatible providers that accept top-level `thinking`.
    pub thinking: Option<ThinkingConfig>,
    /// 供应商特定的推理强度。不支持的供应商或模型必须省略。
    pub reasoning_effort: Option<String>,
}

/// HTTP client for the DeepSeek chat completions API.
#[derive(Debug, Clone)]
pub struct DeepSeekClient {
    http: Client,
    api_key: String,
    base_url: String,
    retry: RetryConfig,
}

impl DeepSeekClient {
    /// Create a new client from explicit configuration.
    pub fn new(config: DeepSeekClientConfig) -> Result<Self> {
        if config.api_key.is_empty() {
            return Err(anyhow!("API key must not be empty"));
        }

        let http = Client::builder().timeout(config.timeout).build()?;

        Ok(Self {
            http,
            api_key: config.api_key,
            base_url: config.base_url,
            retry: config.retry,
        })
    }

    /// Create a client from environment variables.
    ///
    /// Required: `DEEPSEEK_API_KEY`
    /// Optional: `DEEPSEEK_BASE_URL` (defaults to "https://api.deepseek.com")
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("DEEPSEEK_API_KEY")
            .map_err(|_| anyhow!("DEEPSEEK_API_KEY environment variable is not set"))?;

        let base_url = std::env::var("DEEPSEEK_BASE_URL")
            .unwrap_or_else(|_| "https://api.deepseek.com".to_string());

        let config = DeepSeekClientConfig {
            api_key,
            base_url,
            ..Default::default()
        };

        Self::new(config)
    }

    /// The chat completions endpoint URL.
    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    /// Send a streaming request and return a parsed SSE stream of chunks.
    ///
    /// Retry logic applies only to the initial connection — once streaming
    /// begins, the stream is not retried.
    pub async fn stream(
        &self,
        messages: Vec<ChatMessage>,
        model: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        self.stream_with_tools(messages, model, Vec::new()).await
    }

    /// Send a streaming request with tools and return a parsed SSE stream.
    ///
    /// When `tools` is empty the request omits the field entirely — equivalent
    /// to [`Self::stream`]. Retry logic applies only to the initial
    /// connection; once streaming begins, the stream is not retried.
    pub async fn stream_with_tools(
        &self,
        messages: Vec<ChatMessage>,
        model: &str,
        tools: Vec<ToolSpec>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        self.stream_with_opts(
            messages,
            model,
            ChatOpts {
                tools,
                extra_body: None,
                thinking: None,
                reasoning_effort: None,
            },
        )
        .await
    }

    /// Send a streaming request with explicit options.
    ///
    /// `opts.extra_body` follows the same provider-neutrality contract as
    /// [`Self::chat_with_opts`]: only DeepSeek callers may populate it.
    pub async fn stream_with_opts(
        &self,
        messages: Vec<ChatMessage>,
        model: &str,
        opts: ChatOpts,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        let request_body = ChatRequest {
            model: model.to_string(),
            messages,
            tools: if opts.tools.is_empty() {
                None
            } else {
                Some(opts.tools)
            },
            temperature: None,
            max_tokens: None,
            reasoning_effort: opts.reasoning_effort,
            stream: Some(true),
            stream_options: Some(StreamOptions {
                include_usage: true,
            }),
            thinking: opts.thinking,
            extra_body: opts.extra_body,
        };

        self.stream_inner(request_body).await
    }

    /// Issue the streaming HTTP call for a fully-formed [`ChatRequest`].
    ///
    /// Handles both SSE (`text/event-stream`) and non-SSE (JSON) responses.
    /// Some providers (e.g. iFlytek) return a plain JSON response even when
    /// `stream: true` is set — we parse it as a single chunk to avoid silent
    /// empty responses.
    async fn stream_inner(
        &self,
        request_body: ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        let body_json = serde_json::to_string(&request_body)?;
        let endpoint = self.endpoint();
        let api_key = self.api_key.clone();

        let response = fetch_with_retry(
            &self.http,
            || {
                self.http
                    .post(&endpoint)
                    .header("Authorization", format!("Bearer {}", api_key))
                    .header("Content-Type", "application/json")
                    .header("Accept", "text/event-stream")
                    .body(body_json.clone())
            },
            &self.retry,
        )
        .await?;

        // Check if the response is JSON (not SSE) — some providers return a
        // regular JSON response even when stream:true is set.
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if content_type.contains("application/json") {
            let body = response.text().await.map_err(|e| anyhow!("Failed to read response body: {e}"))?;
            let parsed = parse_chat_response(&body)?;
            let chunk = StreamChunk {
                content_delta: Some(parsed.content),
                reasoning_delta: parsed.reasoning_content,
                tool_call_delta: None,
                usage: Some(parsed.usage),
                finish_reason: Some("stop".to_string()),
            };
            let stream: Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>> =
                Box::pin(futures::stream::once(async move { Ok(chunk) }));
            return Ok(stream);
        }

        Ok(parse_sse_stream(response))
    }

    /// Send a non-streaming chat request and return the aggregated response.
    ///
    /// Convenience wrapper over [`Self::chat_with_opts`] with no tools and
    /// no provider-specific extensions — safe for any provider.
    pub async fn chat(&self, messages: Vec<ChatMessage>, model: &str) -> Result<ChatResponse> {
        self.chat_with_opts(messages, model, ChatOpts::default())
            .await
    }

    /// Send a non-streaming chat request with explicit options (tools +
    /// optional DeepSeek `extra_body`).
    ///
    /// ## Provider neutrality
    ///
    /// `opts.extra_body` is forwarded verbatim. Callers MUST only set it
    /// when the active provider is DeepSeek — the default (`None`) keeps
    /// the request shape identical to a vanilla OpenAI-style call, so other
    /// providers are unaffected (see `.kiro/steering/provider-neutrality.md`).
    pub async fn chat_with_opts(
        &self,
        messages: Vec<ChatMessage>,
        model: &str,
        opts: ChatOpts,
    ) -> Result<ChatResponse> {
        let request_body = ChatRequest {
            model: model.to_string(),
            messages,
            tools: if opts.tools.is_empty() {
                None
            } else {
                Some(opts.tools)
            },
            temperature: None,
            max_tokens: None,
            reasoning_effort: opts.reasoning_effort,
            stream: Some(false),
            stream_options: None,
            thinking: opts.thinking,
            extra_body: opts.extra_body,
        };

        let body_json = serde_json::to_string(&request_body)?;
        let endpoint = self.endpoint();
        let api_key = self.api_key.clone();

        let response = fetch_with_retry(
            &self.http,
            || {
                self.http
                    .post(&endpoint)
                    .header("Authorization", format!("Bearer {}", api_key))
                    .header("Content-Type", "application/json")
                    .body(body_json.clone())
            },
            &self.retry,
        )
        .await?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(anyhow!("API error HTTP {}: {}", status.as_u16(), body));
        }

        parse_chat_response(&body)
    }

    /// Fetch the authenticated user's wallet balances from
    /// `<base_url>/user/balance`. Returns `Ok(None)` for any non-2xx
    /// response or transport error so the caller can degrade gracefully —
    /// the balance UI is informational and must never block chat.
    ///
    /// Endpoint:
    /// `GET https://api.deepseek.com/user/balance` with `Bearer <api_key>`
    /// returns `{ is_available, balance_infos: [{currency, total_balance,
    /// granted_balance?, topped_up_balance?}, ...] }`.
    pub async fn get_user_balance(&self) -> Result<Option<UserBalance>> {
        let url = format!("{}/user/balance", self.base_url);
        let resp = match self.http.get(&url).bearer_auth(&self.api_key).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!(error = %e, "get_user_balance: transport error");
                return Ok(None);
            }
        };
        if !resp.status().is_success() {
            tracing::debug!(status = %resp.status(), "get_user_balance: non-2xx");
            return Ok(None);
        }
        match resp.json::<UserBalance>().await {
            Ok(b) if !b.balance_infos.is_empty() || b.is_available => Ok(Some(b)),
            Ok(_) => Ok(None),
            Err(e) => {
                tracing::debug!(error = %e, "get_user_balance: deserialize failed");
                Ok(None)
            }
        }
    }
}

// ─── Response Parsing ─────────────────────────────────────────────────────────

/// Raw API response structure for non-streaming calls.
#[derive(Debug, Deserialize)]
struct RawChatResponse {
    choices: Vec<RawChoice>,
    usage: Option<RawUsage>,
}

#[derive(Debug, Deserialize)]
struct RawChoice {
    message: RawMessage,
}

#[derive(Debug, Deserialize)]
struct RawMessage {
    content: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<RawToolCall>>,
}

#[derive(Debug, Deserialize)]
struct RawToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: RawFunction,
}

#[derive(Debug, Deserialize)]
struct RawFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct RawUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
    #[serde(default)]
    prompt_cache_hit_tokens: u32,
    #[serde(default)]
    prompt_cache_miss_tokens: u32,
}

/// Parse the JSON response body from a non-streaming chat completions call.
fn parse_chat_response(body: &str) -> Result<ChatResponse> {
    let raw: RawChatResponse =
        serde_json::from_str(body).map_err(|e| anyhow!("Failed to parse response: {e}"))?;

    let choice = raw
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("No choices in response"))?;

    let tool_calls = choice
        .message
        .tool_calls
        .unwrap_or_default()
        .into_iter()
        .map(|tc| ToolCall {
            id: tc.id,
            call_type: tc.call_type,
            function: crate::types::FunctionCall {
                name: tc.function.name,
                arguments: tc.function.arguments,
            },
        })
        .collect();

    let usage = match raw.usage {
        Some(u) => Usage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
            prompt_cache_hit_tokens: u.prompt_cache_hit_tokens,
            prompt_cache_miss_tokens: u.prompt_cache_miss_tokens,
        },
        None => Usage::default(),
    };

    Ok(ChatResponse {
        content: choice.message.content.unwrap_or_default(),
        reasoning_content: choice.message.reasoning_content,
        tool_calls,
        usage,
    })
}

// ─── User balance types ───────────────────────────────────────────────────────

/// Top-level response from `/user/balance`.
///
/// `balance_infos` is an array because DeepSeek supports multi-currency
/// wallets (e.g. CNY + USD). Callers typically want the largest entry —
/// see [`pick_primary_balance`].
#[derive(Debug, Clone, Deserialize)]
pub struct UserBalance {
    /// Whether the API key is currently usable. When `false`, all chat
    /// requests will fail until the user tops up.
    pub is_available: bool,
    /// One entry per wallet currency.
    #[serde(default)]
    pub balance_infos: Vec<BalanceInfo>,
}

/// Single-currency wallet entry inside [`UserBalance`].
///
/// All numeric fields arrive as JSON strings (e.g. `"45.32"`) so we keep
/// them as `String` here; the IPC DTO layer converts to `f64` for UI use.
#[derive(Debug, Clone, Deserialize)]
pub struct BalanceInfo {
    /// ISO currency code (`"CNY"`, `"USD"`).
    pub currency: String,
    /// Total balance, including granted + topped-up portions.
    pub total_balance: String,
    /// Free credit DeepSeek granted at signup or via promos.
    #[serde(default)]
    pub granted_balance: Option<String>,
    /// Money the user paid in via top-up.
    #[serde(default)]
    pub topped_up_balance: Option<String>,
}

/// Pick the wallet with the largest `total_balance`. DeepSeek can return
/// multiple wallets (typically CNY for mainland users, USD for overseas),
/// and the user's intuition is "the wallet I actually paid for". Returns
/// `None` if `infos` is empty.
///
/// Parses each `total_balance` string as `f64`; entries that fail to parse
/// are treated as 0 so they never beat a real balance.
pub fn pick_primary_balance(infos: &[BalanceInfo]) -> Option<&BalanceInfo> {
    infos.iter().max_by(|a, b| {
        let av = a.total_balance.parse::<f64>().unwrap_or(0.0);
        let bv = b.total_balance.parse::<f64>().unwrap_or(0.0);
        av.partial_cmp(&bv).unwrap_or(std::cmp::Ordering::Equal)
    })
}

#[cfg(test)]
mod balance_tests {
    use super::*;

    #[test]
    fn pick_primary_picks_largest_total() {
        let infos = vec![
            BalanceInfo {
                currency: "CNY".into(),
                total_balance: "10.00".into(),
                granted_balance: None,
                topped_up_balance: None,
            },
            BalanceInfo {
                currency: "USD".into(),
                total_balance: "5.50".into(),
                granted_balance: None,
                topped_up_balance: None,
            },
            BalanceInfo {
                currency: "EUR".into(),
                total_balance: "12.34".into(),
                granted_balance: None,
                topped_up_balance: None,
            },
        ];
        let primary = pick_primary_balance(&infos).expect("non-empty");
        assert_eq!(primary.currency, "EUR");
    }

    #[test]
    fn pick_primary_handles_unparseable_strings() {
        let infos = vec![
            BalanceInfo {
                currency: "CNY".into(),
                total_balance: "not-a-number".into(),
                granted_balance: None,
                topped_up_balance: None,
            },
            BalanceInfo {
                currency: "USD".into(),
                total_balance: "5.50".into(),
                granted_balance: None,
                topped_up_balance: None,
            },
        ];
        let primary = pick_primary_balance(&infos).expect("non-empty");
        assert_eq!(primary.currency, "USD");
    }

    #[test]
    fn pick_primary_empty_returns_none() {
        let infos: Vec<BalanceInfo> = vec![];
        assert!(pick_primary_balance(&infos).is_none());
    }

    #[test]
    fn deserialize_response_with_full_fields() {
        let json = r#"{
            "is_available": true,
            "balance_infos": [
                {
                    "currency": "CNY",
                    "total_balance": "100.50",
                    "granted_balance": "10.00",
                    "topped_up_balance": "90.50"
                }
            ]
        }"#;
        let parsed: UserBalance = serde_json::from_str(json).unwrap();
        assert!(parsed.is_available);
        assert_eq!(parsed.balance_infos.len(), 1);
        let entry = &parsed.balance_infos[0];
        assert_eq!(entry.total_balance, "100.50");
        assert_eq!(entry.granted_balance.as_deref(), Some("10.00"));
    }

    #[test]
    fn deserialize_minimal_response() {
        let json = r#"{
            "is_available": false,
            "balance_infos": []
        }"#;
        let parsed: UserBalance = serde_json::from_str(json).unwrap();
        assert!(!parsed.is_available);
        assert!(parsed.balance_infos.is_empty());
    }
}
