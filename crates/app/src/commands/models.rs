//! list_models + switch_model commands.

use crate::commands::config::read_default_provider_id_pub;
use deepseek_core::pricing::deepseek;
use deepseek_core::pricing::ProviderId;
use deepseek_state::{ThreadRepo, ThreadUpdate};

use crate::dto::ModelInfoDto;
use crate::AppState;

/// Read from the single source of truth in [`deepseek_core::pricing::deepseek`].
/// Adding a new model is one line in `pricing/deepseek.rs::all_models` —
/// the UI catalog and the cost-computation table stay in sync because
/// they're literally the same data.
#[tauri::command]
pub async fn list_models(_state: tauri::State<'_, AppState>) -> Result<Vec<ModelInfoDto>, String> {
    let cfg = crate::commands::config::get_config(_state).await?;
    let models: Vec<ModelInfoDto> = cfg
        .providers
        .iter()
        .filter(|p| p.enabled)
        .flat_map(|p| {
            p.models.iter().filter(|m| m.enabled).map(|m| ModelInfoDto {
                id: m.id.clone(),
                label: m.label.clone(),
                description: p.name.clone(),
                price_per_million_input_usd: 0.0,
                price_per_million_output_usd: 0.0,
                price_per_million_cache_hit_usd: 0.0,
                context_window: 0,
                provider_id: p.id.clone(),
            })
        })
        .collect();
    if !models.is_empty() {
        return Ok(models);
    }
    Ok(deepseek::all_models()
        .iter()
        .map(|(id, p)| ModelInfoDto {
            id: (*id).into(),
            label: p.label.into(),
            description: p.description.into(),
            price_per_million_input_usd: p.cache_miss_per_m_usd,
            price_per_million_output_usd: p.output_per_m_usd,
            price_per_million_cache_hit_usd: p.cache_read_per_m_usd,
            context_window: p.context_window,
            provider_id: "deepseek".into(),
        })
        .collect())
}

#[tauri::command]
pub async fn switch_model(
    state: tauri::State<'_, AppState>,
    thread_id: String,
    model_id: String,
    provider_id: Option<String>,
) -> Result<(), String> {
    let provider_id = provider_id.unwrap_or_else(read_default_provider_id_pub);
    if let Some(s) = state.engine.cache().get(&thread_id) {
        *s.model.write() = model_id.clone();
        *s.provider_id.write() = provider_id.clone();
        *s.provider.write() = ProviderId::from_str_lossy(&provider_id);
    }
    let repo = ThreadRepo::new(state.db.as_ref());
    repo.update(
        &thread_id,
        ThreadUpdate {
            model: Some(model_id),
            provider_id: Some(provider_id),
            touch: true,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
