//! send_message + abort_turn commands.

use base64::Engine;
use deepseek_core::pricing::ProviderId;
use deepseek_state::ThreadRepo;

use crate::events::dispatch_engine_event;
use crate::AppState;
/// Run a turn for `input.threadId`, streaming results back as Tauri events.
///
/// If `input.threadId` is empty, a fresh thread is created and the first
/// stream event will carry its real id (P4 debug-UI affordance — task 7.1
/// removes this once the prototype's `create_thread` flow is the entry).
#[tauri::command]
pub async fn send_message(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: crate::dto::SendMessageInput,
) -> Result<(), String> {
    let engine = state.engine.clone();

    let resolved_id: String = if input.thread_id.is_empty() {
        let provider_id = crate::commands::config::read_default_provider_id_pub();
        let model = crate::commands::config::read_default_model_pub();
        let workspace_dir = crate::commands::config::read_stored_workspace_dir();
        let repo = deepseek_state::ThreadRepo::new(state.db.as_ref());
        repo.create(deepseek_state::ThreadInsert {
            name: None,
            model,
            cwd: Some(workspace_dir).filter(|s| !s.is_empty()),
            permission_mode: "default".into(),
            provider_id,
            thinking_effort: Some("high".into()),
            parent_thread_id: None,
            project_id: None,
        })
        .map_err(|e| format!("create_thread: {e}"))?
        .id
    } else {
        input.thread_id
    };

    sync_thread_runtime_from_db(&state, &resolved_id)?;

    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<deepseek_core::engine::EngineEvent>();

    let app_clone = app.clone();
    let forward = tokio::spawn(async move {
        while let Some(ev) = event_rx.recv().await {
            dispatch_engine_event(&app_clone, ev);
        }
    });

    // Process attachments: separate images (→ multimodal API) from text files (→ prepend).
    let mut image_data_uris: Vec<String> = Vec::new();
    let mut text_parts: Vec<String> = Vec::new();
    for att in &input.attachments {
        if att.starts_with("data:image/") {
            // Already a data URI from the frontend (pasted/screenshot).
            image_data_uris.push(att.clone());
        } else if att.starts_with("data:") {
            // Non-image data URI — skip (not expected).
            continue;
        } else {
            let path = std::path::Path::new(att);
            let is_image = path.extension()
                .and_then(|e| e.to_str())
                .map(|e| matches!(e.to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp"))
                .unwrap_or(false);
            if is_image {
                match std::fs::read(path) {
                    Ok(bytes) => {
                        use base64::engine::general_purpose::STANDARD as B64;
                        let b64 = B64.encode(&bytes);
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
                        let mime = match ext.as_str() {
                            "jpg" | "jpeg" => "image/jpeg",
                            "gif" => "image/gif",
                            "webp" => "image/webp",
                            "bmp" => "image/bmp",
                            _ => "image/png",
                        };
                        image_data_uris.push(format!("data:{mime};base64,{b64}"));
                    }
                    Err(e) => {
                        text_parts.push(format!("--- Attached file: {att} (error reading: {e}) ---"));
                    }
                }
            } else {
                match std::fs::read_to_string(path) {
                    Ok(text) => {
                        let filename = path.file_name()
                            .map(|n| n.to_string_lossy())
                            .unwrap_or_default();
                        let truncated = if text.len() > 50_000 {
                            format!("{}...(truncated, {} chars)", &text[..50_000], text.len())
                        } else {
                            text
                        };
                        text_parts.push(format!(
                            "--- Begin attached file: {filename} ---\n{truncated}\n--- End attached file: {filename} ---"
                        ));
                    }
                    Err(e) => {
                        text_parts.push(format!("--- Attached file: {att} (error reading: {e}) ---"));
                    }
                }
            }
        }
    }
    if !input.content.trim().is_empty() {
        text_parts.push(input.content.clone());
    }
    let text_content = text_parts.join("\n\n");

    let result = if image_data_uris.is_empty() {
        engine.send_message(resolved_id, text_content, event_tx).await
    } else {
        engine.send_message_with_images(resolved_id, text_content, image_data_uris, event_tx).await
    }
    .map_err(|e| format!("Engine error: {e}"));

    let _ = forward.await;

    result
}

fn sync_thread_runtime_from_db(
    state: &tauri::State<'_, AppState>,
    thread_id: &str,
) -> Result<(), String> {
    let repo = ThreadRepo::new(state.db.as_ref());
    let row = repo.get(thread_id).map_err(|e| e.to_string())?;
    if let Some(cached) = state.engine.cache().get(thread_id) {
        *cached.model.write() = row.model.clone();
        *cached.provider_id.write() = row.provider_id.clone();
        *cached.provider.write() = ProviderId::from_str_lossy(&row.provider_id);
        *cached.thinking_effort.write() = row.thinking_effort.clone();
    }
    tracing::info!(
        thread_id = %thread_id,
        provider_id = %row.provider_id,
        model = %row.model,
        thinking_effort = %row.thinking_effort,
        "send_message runtime synced"
    );
    Ok(())
}

#[tauri::command]
pub async fn abort_turn(
    state: tauri::State<'_, AppState>,
    thread_id: String,
) -> Result<(), String> {
    state.engine.abort_turn(&thread_id);
    Ok(())
}
