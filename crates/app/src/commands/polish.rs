//! polish_prompt command — optimizes user's draft text using the configured model.

use deepseek_client::types::ChatMessage;

use crate::commands::config::{client_for_provider_id, read_default_provider_id_pub, read_default_model_pub};

/// Polish/optimize the user's draft message using the current model.
/// Returns the polished text.
#[tauri::command]
pub async fn polish_prompt(text: String) -> Result<String, String> {
    let provider_id = read_default_provider_id_pub();
    let model = read_default_model_pub();

    let client = client_for_provider_id(&provider_id)
        .ok_or_else(|| format!("provider '{provider_id}' is not configured"))?;

    let system_prompt = "You are a helpful assistant that rewrites user messages to make them clearer, more concise, and more actionable. Keep the original intent and key information. Output ONLY the rewritten message, no explanations, no quotes, no prefixes.";

    let messages = vec![
        ChatMessage::system(system_prompt),
        ChatMessage::user(&text),
    ];

    let response = client
        .chat(messages, &model)
        .await
        .map_err(|e| format!("polish failed: {e}"))?;

    let polished = response.content.trim().to_string();
    if polished.is_empty() {
        return Err("model returned empty response".into());
    }
    Ok(polished)
}
