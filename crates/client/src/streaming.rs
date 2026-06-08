use std::pin::Pin;

use anyhow::{anyhow, Result};
use futures::stream::Stream;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::io::StreamReader;
use tracing::warn;

use crate::types::{RawStreamChunk, StreamChunk, ToolCallDelta};

/// Outcome of parsing a single SSE `data:` payload.
///
/// The three cases are deliberately distinct so the stream loop can tell
/// "the model finished" (`Done`) apart from "this one frame was malformed"
/// (`Malformed`). The old code collapsed both into `None` and `break`ed on
/// either — so a single garbled chunk silently truncated the whole answer.
#[derive(Debug)]
enum SseEvent {
    /// A well-formed content/reasoning/tool/usage delta.
    Chunk(Box<StreamChunk>),
    /// The `[DONE]` sentinel — the stream is complete (normal end).
    Done,
    /// The payload could not be parsed. The frame is skipped (logged) rather
    /// than ending the stream, so one bad frame never truncates the answer.
    Malformed,
}

/// Parse a single SSE `data:` payload into an [`SseEvent`].
fn parse_sse_event(data: &str) -> SseEvent {
    let data = data.trim();
    if data == "[DONE]" {
        return SseEvent::Done;
    }

    let raw: RawStreamChunk = match serde_json::from_str(data) {
        Ok(r) => r,
        Err(_) => return SseEvent::Malformed,
    };

    let mut chunk = StreamChunk::default();

    // Extract usage if present (typically on the final chunk)
    if let Some(usage) = raw.usage {
        chunk.usage = Some(usage);
    }

    // Extract from the first choice (DeepSeek streams one choice at a time)
    if let Some(choice) = raw.choices.first() {
        chunk.finish_reason = choice.finish_reason.clone();
        chunk.content_delta = choice.delta.content.clone();
        chunk.reasoning_delta = choice.delta.reasoning_content.clone();

        // Extract tool call delta if present
        if let Some(tool_calls) = &choice.delta.tool_calls {
            if let Some(tc) = tool_calls.first() {
                let (name, arguments_delta) = match &tc.function {
                    Some(f) => (f.name.clone(), f.arguments.clone()),
                    None => (None, None),
                };
                chunk.tool_call_delta = Some(ToolCallDelta {
                    index: tc.index,
                    id: tc.id.clone(),
                    name,
                    arguments_delta,
                });
            }
        }
    }

    SseEvent::Chunk(Box::new(chunk))
}

/// Parse a single SSE data line into a [`StreamChunk`].
///
/// Returns `None` for the `[DONE]` sentinel **or** an unparseable payload.
/// Prefer [`parse_sse_event`] internally — this thin wrapper is kept for
/// any external/test callers that only need the happy-path chunk.
pub fn parse_sse_data(data: &str) -> Option<StreamChunk> {
    match parse_sse_event(data) {
        SseEvent::Chunk(c) => Some(*c),
        SseEvent::Done | SseEvent::Malformed => None,
    }
}

/// Parse an SSE stream from a reqwest response into a Stream of StreamChunks.
///
/// Error handling (the whole point of this function over a naive loop):
/// - **Transport error** (connection drop mid-stream): yielded as `Err` so
///   the engine surfaces a *retryable* error instead of silently treating a
///   truncated answer as a clean finish.
/// - **`[DONE]` sentinel**: ends the stream normally.
/// - **Malformed frame**: logged and skipped — one bad frame must not
///   truncate the rest of the answer.
pub fn parse_sse_stream(
    response: reqwest::Response,
) -> Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>> {
    use futures::StreamExt;

    let byte_stream = response
        .bytes_stream()
        .map(|result| result.map_err(std::io::Error::other));

    let reader = StreamReader::new(byte_stream);
    let buf_reader = BufReader::new(reader);

    Box::pin(async_stream::stream! {
        let mut lines = buf_reader.lines();
        loop {
            let line = match lines.next_line().await {
                Ok(Some(line)) => line,
                // Clean EOF: server closed the stream without an explicit
                // [DONE]. Treat as normal end.
                Ok(None) => break,
                // Transport error mid-stream (connection reset, TLS drop):
                // surface it so the answer isn't silently truncated.
                Err(e) => {
                    yield Err(anyhow!("stream transport error: {e}"));
                    break;
                }
            };

            // Skip empty lines (SSE uses them as event delimiters)
            if line.is_empty() {
                continue;
            }
            // Only process data lines
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            match parse_sse_event(data) {
                SseEvent::Chunk(chunk) => yield Ok(*chunk),
                SseEvent::Done => break,
                SseEvent::Malformed => {
                    // One unparseable frame must NOT end the stream. Log and
                    // keep reading — the model may still have more to say.
                    warn!(payload = %truncate_for_log(data), "skipping malformed SSE frame");
                    continue;
                }
            }
        }
    })
}

/// Truncate a payload for safe logging (avoid dumping huge frames).
fn truncate_for_log(s: &str) -> String {
    const MAX: usize = 200;
    if s.len() <= MAX {
        s.to_string()
    } else {
        let mut end = MAX;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn done_sentinel_is_done() {
        assert!(matches!(parse_sse_event("[DONE]"), SseEvent::Done));
        assert!(matches!(parse_sse_event("  [DONE]  "), SseEvent::Done));
    }

    #[test]
    fn valid_content_delta_parses() {
        let data = r#"{"id":"x","choices":[{"delta":{"content":"hello"},"finish_reason":null}]}"#;
        match parse_sse_event(data) {
            SseEvent::Chunk(c) => assert_eq!(c.content_delta.as_deref(), Some("hello")),
            other => panic!("expected Chunk, got {other:?}"),
        }
    }

    #[test]
    fn malformed_frame_is_malformed_not_done() {
        // Regression (P0-3): a garbled JSON frame must be `Malformed` (skip),
        // NOT collapsed into the same signal as `[DONE]` (which used to end
        // the whole stream and truncate the answer).
        assert!(matches!(
            parse_sse_event("{not valid json"),
            SseEvent::Malformed
        ));
        assert!(matches!(parse_sse_event("garbage"), SseEvent::Malformed));
    }

    #[test]
    fn reasoning_and_usage_extracted() {
        let data = r#"{"id":"x","choices":[{"delta":{"reasoning_content":"think"},"finish_reason":null}],"usage":{"prompt_tokens":3,"completion_tokens":1,"total_tokens":4}}"#;
        match parse_sse_event(data) {
            SseEvent::Chunk(c) => {
                assert_eq!(c.reasoning_delta.as_deref(), Some("think"));
                assert_eq!(c.usage.map(|u| u.total_tokens), Some(4));
            }
            other => panic!("expected Chunk, got {other:?}"),
        }
    }

    #[test]
    fn tool_call_delta_extracted() {
        let data = r#"{"id":"x","choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"read_file","arguments":"{\"p\""}}]},"finish_reason":null}]}"#;
        match parse_sse_event(data) {
            SseEvent::Chunk(c) => {
                let tcd = c.tool_call_delta.expect("tool call delta");
                assert_eq!(tcd.id.as_deref(), Some("call_1"));
                assert_eq!(tcd.name.as_deref(), Some("read_file"));
            }
            other => panic!("expected Chunk, got {other:?}"),
        }
    }

    #[test]
    fn truncate_for_log_respects_char_boundary() {
        // Multibyte content must not panic when truncated for logging.
        let long = "中".repeat(300);
        let out = truncate_for_log(&long);
        assert!(out.len() <= 200 + "…".len() + 3);
    }

    #[test]
    fn parse_sse_data_wrapper_back_compat() {
        assert!(parse_sse_data("[DONE]").is_none());
        assert!(parse_sse_data("garbage").is_none());
        assert!(parse_sse_data(
            r#"{"id":"x","choices":[{"delta":{"content":"hi"},"finish_reason":null}]}"#
        )
        .is_some());
    }
}
