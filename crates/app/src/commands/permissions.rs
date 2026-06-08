//! approve_tool + list/remove/get permission rules commands.

use deepseek_tools::permission::PermissionRule;

use crate::dto::{
    ApproveToolDecision, ApproveToolInput, CyclePermissionModeResult, ToolPermissionContextDto,
};
use crate::AppState;

/// Receive an approval decision from the frontend and forward it to the gate.
#[tauri::command]
pub async fn approve_tool(
    state: tauri::State<'_, AppState>,
    input: ApproveToolInput,
) -> Result<(), String> {
    let domain_decision = match input.decision {
        ApproveToolDecision::Allow {
            updated_input,
            permission_updates,
        } => deepseek_core::gate::ApprovalDecision::Allow {
            updated_input,
            permission_updates,
        },
        ApproveToolDecision::Deny { message } => {
            deepseek_core::gate::ApprovalDecision::Deny { message }
        }
    };
    let delivered = state
        .gate
        .feed_response(&input.tool_use_id, domain_decision);
    if !delivered {
        tracing::warn!(
            tool_use_id = %input.tool_use_id,
            "approve_tool: no pending request matched",
        );
        return Err("no pending approval request matched this tool_use_id".into());
    }
    Ok(())
}

/// List the session-scoped permission rules for `thread_id`.
#[tauri::command]
pub async fn list_permission_rules(
    state: tauri::State<'_, AppState>,
    thread_id: String,
) -> Result<Vec<PermissionRule>, String> {
    let s = state
        .engine
        .get_or_load(&thread_id)
        .map_err(|e| e.to_string())?;
    let rules = s.permission_ctx.read().list_rules();
    Ok(rules)
}

/// Remove a specific rule from `thread_id`'s permission context.
#[tauri::command]
pub async fn remove_permission_rule(
    state: tauri::State<'_, AppState>,
    thread_id: String,
    rule: PermissionRule,
) -> Result<(), String> {
    let s = state
        .engine
        .get_or_load(&thread_id)
        .map_err(|e| e.to_string())?;
    s.permission_ctx.write().remove_rule(&rule);
    Ok(())
}

/// Snapshot the entire permission context for `thread_id`.
#[tauri::command]
pub async fn get_permission_context(
    state: tauri::State<'_, AppState>,
    thread_id: String,
) -> Result<ToolPermissionContextDto, String> {
    let s = state
        .engine
        .get_or_load(&thread_id)
        .map_err(|e| e.to_string())?;
    let dto = {
        let ctx = s.permission_ctx.read();
        ToolPermissionContextDto {
            mode: ctx.mode,
            always_allow_rules: ctx.list_allow_rules(),
            always_deny_rules: ctx.list_deny_rules(),
            always_ask_rules: ctx.list_ask_rules(),
            additional_working_directories: ctx.additional_working_directories.clone(),
            is_bypass_permissions_mode_available: ctx.is_bypass_permissions_mode_available,
        }
    };
    Ok(dto)
}

/// Cycle to the next permission mode for `thread_id`.
///
/// Mode cycling order: default → acceptEdits → plan → bypassPermissions (if available) → default.
/// Returns the new mode after cycling.
#[tauri::command]
pub async fn cycle_permission_mode(
    state: tauri::State<'_, AppState>,
    thread_id: String,
) -> Result<CyclePermissionModeResult, String> {
    let s = state
        .engine
        .get_or_load(&thread_id)
        .map_err(|e| e.to_string())?;
    let new_mode = {
        let mut ctx = s.permission_ctx.write();
        let bypass_available = ctx.is_bypass_permissions_mode_available;
        let next = deepseek_core::permission::get_next_permission_mode(ctx.mode, bypass_available);
        ctx.mode = next;
        next
    };
    Ok(CyclePermissionModeResult { new_mode })
}
