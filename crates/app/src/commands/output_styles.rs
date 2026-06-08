//! Output-style commands (Phase 2): list / read / save / set-active.
//!
//! Output styles are user-editable Markdown snippets at
//! `<data_root>/output-styles/<name>.md`. The active one (persisted in
//! config.json `outputStyle`) is appended to every thread's system prompt by
//! [`deepseek_core::memory::PromptAugment`]. Editing + activating from the
//! settings UI takes effect on the next turn (cache is evicted on activate).

use serde::{Deserialize, Serialize};

use crate::AppState;

/// One output-style entry for the settings list.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputStyleDto {
    /// Style name (file stem, no `.md`).
    pub name: String,
    /// Whether this style is the currently-active one.
    pub active: bool,
}

/// Validate a style name: letters, digits, hyphen, underscore only (so it
/// maps safely to a `<name>.md` filename and can't escape the directory).
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// List all output styles under `<data_root>/output-styles/*.md`, marking the
/// active one.
#[tauri::command]
pub async fn list_output_styles(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<OutputStyleDto>, String> {
    let dir = state.data_root.join("output-styles");
    let active = state.prompt_augment.output_style();
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        let mut names: Vec<String> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "md").unwrap_or(false))
            .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .collect();
        names.sort();
        for name in names {
            let is_active = active.as_deref() == Some(name.as_str());
            out.push(OutputStyleDto {
                name,
                active: is_active,
            });
        }
    }
    Ok(out)
}

/// Read an output-style's Markdown body.
#[tauri::command]
pub async fn read_output_style(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<String, String> {
    if !valid_name(&name) {
        return Err("invalid output-style name".into());
    }
    let path = state
        .data_root
        .join("output-styles")
        .join(format!("{name}.md"));
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

/// Create or overwrite an output-style file.
#[tauri::command]
pub async fn save_output_style(
    state: tauri::State<'_, AppState>,
    name: String,
    content: String,
) -> Result<(), String> {
    if !valid_name(&name) {
        return Err("invalid output-style name: use letters, digits, hyphens, underscores".into());
    }
    let dir = state.data_root.join("output-styles");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{name}.md"));
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}

/// Set (or clear with `null`) the active output-style. Persists to
/// config.json, updates the live `PromptAugment`, and evicts cached threads
/// so the next turn recomposes its system prompt with the new style.
#[tauri::command]
pub async fn set_active_output_style(
    state: tauri::State<'_, AppState>,
    name: Option<String>,
) -> Result<(), String> {
    if let Some(n) = &name {
        if !valid_name(n) {
            return Err("invalid output-style name".into());
        }
    }
    crate::commands::config::write_active_output_style(name.as_deref())?;
    state.prompt_augment.set_output_style(name);
    // Drop cached thread states so each recomposes its prompt next turn.
    state.engine.cache().clear();
    Ok(())
}

/// Delete an output-style file. If it was the active style, clears the active
/// selection (persisted) and evicts cached threads so the next turn drops it.
#[tauri::command]
pub async fn delete_output_style(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    if !valid_name(&name) {
        return Err("invalid output-style name".into());
    }
    let path = state
        .data_root
        .join("output-styles")
        .join(format!("{name}.md"));
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    // If the deleted style was active, clear the active selection.
    if state.prompt_augment.output_style().as_deref() == Some(name.as_str()) {
        crate::commands::config::write_active_output_style(None)?;
        state.prompt_augment.set_output_style(None);
        state.engine.cache().clear();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn output_style_dto_camelcase() {
        let dto = OutputStyleDto {
            name: "terse".into(),
            active: true,
        };
        let v = serde_json::to_value(&dto).unwrap();
        assert_eq!(v, json!({ "name": "terse", "active": true }));
        let back: OutputStyleDto = serde_json::from_value(v).unwrap();
        assert_eq!(back.name, "terse");
        assert!(back.active);
    }

    #[test]
    fn name_validation() {
        assert!(valid_name("terse"));
        assert!(valid_name("my-style_2"));
        assert!(!valid_name(""));
        assert!(!valid_name("../escape"));
        assert!(!valid_name("has space"));
        assert!(!valid_name("dot.name"));
    }
}
