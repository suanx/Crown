//! 项目管理 IPC 命令。

use deepseek_state::{ProjectInsert, ProjectRepo, ProjectUpdate};

use crate::dto::{CreateProjectInput, ProjectSummaryDto, UpdateProjectInput};
use crate::AppState;

#[tauri::command]
pub async fn pick_project_directory() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .set_title("选择项目文件夹")
            .pick_folder()
            .map(|path| path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| format!("pick directory task failed: {e}"))
}

#[tauri::command]
pub async fn list_projects(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ProjectSummaryDto>, String> {
    let repo = ProjectRepo::new(state.db.as_ref());
    let projects = repo.list().map_err(|e| e.to_string())?;
    Ok(projects.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn create_project(
    state: tauri::State<'_, AppState>,
    input: CreateProjectInput,
) -> Result<ProjectSummaryDto, String> {
    let repo = ProjectRepo::new(state.db.as_ref());
    let name = input.name.trim();
    let path = input.path.trim();
    if name.is_empty() {
        return Err("项目名称不能为空".into());
    }
    if path.is_empty() {
        return Err("项目路径不能为空".into());
    }
    let project = repo
        .create(ProjectInsert {
            name: name.to_string(),
            path: path.to_string(),
        })
        .map_err(|e| e.to_string())?;
    let summary = repo.get(&project.id).map_err(|e| e.to_string())?;
    Ok(ProjectSummaryDto {
        id: summary.id,
        name: summary.name,
        path: summary.path,
        thread_count: 0,
        last_used_at: crate::dto::ms_to_rfc3339(summary.updated_at),
    })
}

#[tauri::command]
pub async fn update_project(
    state: tauri::State<'_, AppState>,
    input: UpdateProjectInput,
) -> Result<(), String> {
    let repo = ProjectRepo::new(state.db.as_ref());
    repo.update(
        &input.project_id,
        ProjectUpdate {
            name: input
                .name
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            path: input
                .path
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        },
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn delete_project(
    state: tauri::State<'_, AppState>,
    project_id: String,
) -> Result<(), String> {
    let repo = ProjectRepo::new(state.db.as_ref());
    repo.delete(&project_id).map_err(|e| e.to_string())?;
    Ok(())
}
