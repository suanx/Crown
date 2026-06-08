//! Skill commands — discovery + read, backed by the `deepseek-skill` crate.
//!
//! Skills are plain directories with a `SKILL.md` file, discovered from four
//! source dirs (global native/claude + project native/claude). Discovery is
//! stateless (no manager): every call re-scans the directories, so a skill
//! dropped on disk shows up on the next `skill_list` without a restart. That
//! makes `skill_reload` a no-op success the UI can call to refresh.
//!
//! Project-scoped skills resolve against a thread's `cwd`. The optional
//! `threadId` argument lets the UI pass the active thread so its project
//! skills are included; absent ⇒ global-only.

use std::path::PathBuf;

use deepseek_skill::discovery::{discover_all, SkillMeta};
use deepseek_skill::loader::load_skill_body;
use deepseek_state::ThreadRepo;

use crate::dto::SkillDto;
use crate::AppState;

/// Resolve the cwd for project-scope skill discovery from an optional thread id.
fn cwd_for_thread(state: &AppState, thread_id: Option<&str>) -> Option<PathBuf> {
    let tid = thread_id?;
    let thread = ThreadRepo::new(&state.db).get(tid).ok()?;
    thread.cwd.map(PathBuf::from)
}

/// Discover all available skills (global + the thread's project scope).
#[tauri::command]
pub async fn skill_list(
    state: tauri::State<'_, AppState>,
    thread_id: Option<String>,
) -> Result<Vec<SkillDto>, String> {
    let cwd = cwd_for_thread(&state, thread_id.as_deref());
    let metas: Vec<SkillMeta> = discover_all(cwd.as_deref());
    Ok(metas.iter().map(SkillDto::from).collect())
}

/// Read a skill's full body by name (progressive disclosure level 2 — the
/// same text the model gets when it invokes the `skill` tool). Used by the UI
/// to preview a skill. `args` is substituted for `$ARGUMENTS`.
#[tauri::command]
pub async fn skill_read(
    state: tauri::State<'_, AppState>,
    name: String,
    thread_id: Option<String>,
    args: Option<String>,
) -> Result<String, String> {
    let cwd = cwd_for_thread(&state, thread_id.as_deref());
    let metas = discover_all(cwd.as_deref());
    let meta = metas
        .iter()
        .find(|m| m.name == name)
        .ok_or_else(|| format!("unknown skill '{name}'"))?;
    load_skill_body(meta, args.as_deref()).map_err(|e| e.to_string())
}

/// Re-scan skill directories. Discovery is stateless, so this is a refresh
/// signal: it returns the freshly discovered count so the UI can confirm.
#[tauri::command]
pub async fn skill_reload(
    state: tauri::State<'_, AppState>,
    thread_id: Option<String>,
) -> Result<usize, String> {
    let cwd = cwd_for_thread(&state, thread_id.as_deref());
    Ok(discover_all(cwd.as_deref()).len())
}
