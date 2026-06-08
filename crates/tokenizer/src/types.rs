//! Shared types for the tokenizer crate.
//!
//! Mirrors client crate types to avoid a direct dependency.

use serde::{Deserialize, Serialize};

/// A single part of a multimodal message content array.
/// Follows the OpenAI vision API format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
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
