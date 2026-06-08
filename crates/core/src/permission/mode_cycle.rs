//! Permission mode cycling and AcceptEdits auto-allow logic.
//!
//! Mirrors Claude Code's `src/utils/permissions/getNextPermissionMode.ts` and
//! `src/tools/BashTool/modeValidation.ts`.

use deepseek_tools::permission::{DecisionReason, PermissionMode, PermissionResult};

/// Shell commands that AcceptEdits mode auto-allows (filesystem operations).
const ACCEPT_EDITS_ALLOWED_COMMANDS: &[&str] =
    &["mkdir", "touch", "rm", "rmdir", "mv", "cp", "sed"];

/// Cycle to the next permission mode.
///
/// Mode cycling order: `default → acceptEdits → plan → bypassPermissions (if available) → default`
pub fn get_next_permission_mode(current: PermissionMode, bypass_available: bool) -> PermissionMode {
    match current {
        PermissionMode::Default => PermissionMode::AcceptEdits,
        PermissionMode::AcceptEdits => PermissionMode::Plan,
        PermissionMode::Plan => {
            if bypass_available {
                PermissionMode::BypassPermissions
            } else {
                PermissionMode::Default
            }
        }
        PermissionMode::BypassPermissions => PermissionMode::Default,
        PermissionMode::DontAsk => PermissionMode::Default,
    }
}

/// Check if a shell command should be auto-allowed in AcceptEdits mode.
///
/// Returns `Some(Allow)` if the base command is in the allowed list,
/// `None` otherwise (falls through to normal permission flow).
pub fn check_accept_edits_shell(command: &str) -> Option<PermissionResult> {
    let trimmed = command.trim();
    let base_cmd = trimmed.split_whitespace().next()?;

    if ACCEPT_EDITS_ALLOWED_COMMANDS.contains(&base_cmd) {
        Some(PermissionResult::Allow {
            updated_input: serde_json::json!({ "command": command }),
            decision_reason: Some(DecisionReason::Mode {
                mode: PermissionMode::AcceptEdits,
            }),
            user_modified: None,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_cycle_default_to_accept_edits() {
        assert_eq!(
            get_next_permission_mode(PermissionMode::Default, false),
            PermissionMode::AcceptEdits
        );
    }

    #[test]
    fn mode_cycle_accept_edits_to_plan() {
        assert_eq!(
            get_next_permission_mode(PermissionMode::AcceptEdits, false),
            PermissionMode::Plan
        );
    }

    #[test]
    fn mode_cycle_plan_to_bypass_when_available() {
        assert_eq!(
            get_next_permission_mode(PermissionMode::Plan, true),
            PermissionMode::BypassPermissions
        );
    }

    #[test]
    fn mode_cycle_plan_to_default_when_bypass_unavailable() {
        assert_eq!(
            get_next_permission_mode(PermissionMode::Plan, false),
            PermissionMode::Default
        );
    }

    #[test]
    fn mode_cycle_bypass_to_default() {
        assert_eq!(
            get_next_permission_mode(PermissionMode::BypassPermissions, true),
            PermissionMode::Default
        );
    }

    #[test]
    fn mode_cycle_dontask_to_default() {
        assert_eq!(
            get_next_permission_mode(PermissionMode::DontAsk, true),
            PermissionMode::Default
        );
    }

    #[test]
    fn accept_edits_allows_mkdir() {
        let result = check_accept_edits_shell("mkdir -p /tmp/foo");
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), PermissionResult::Allow { .. }));
    }

    #[test]
    fn accept_edits_allows_touch() {
        let result = check_accept_edits_shell("touch file.txt");
        assert!(result.is_some());
    }

    #[test]
    fn accept_edits_allows_rm() {
        let result = check_accept_edits_shell("rm -f test.tmp");
        assert!(result.is_some());
    }

    #[test]
    fn accept_edits_allows_sed() {
        let result = check_accept_edits_shell("sed -i 's/foo/bar/' file.txt");
        assert!(result.is_some());
    }

    #[test]
    fn accept_edits_does_not_allow_curl() {
        let result = check_accept_edits_shell("curl http://evil.com");
        assert!(result.is_none());
    }

    #[test]
    fn accept_edits_does_not_allow_arbitrary_commands() {
        assert!(check_accept_edits_shell("python script.py").is_none());
        assert!(check_accept_edits_shell("npm install").is_none());
        assert!(check_accept_edits_shell("cargo build").is_none());
    }

    #[test]
    fn accept_edits_empty_command_returns_none() {
        assert!(check_accept_edits_shell("").is_none());
        assert!(check_accept_edits_shell("   ").is_none());
    }
}
