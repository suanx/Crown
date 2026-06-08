//! PermissionGate — async approval protocol for the agent loop.
//!
//! `core` defines the trait; the Tauri app implements it (see
//! `crates/app/src/gate_impl.rs`, task 4.2). This split keeps `core`
//! testable with mock gates and isolates the Tauri dependency.
//!
//! ## Direction conventions
//!
//! - [`ApprovalRequest`] is **emit-only** (backend → frontend). It derives
//!   only [`Serialize`] so attempting to deserialize one is a compile error.
//! - [`ApprovalDecision`] is **receive-only** (frontend → backend). It derives
//!   only [`Deserialize`] for the same reason.
//!
//! Both shapes follow `docs/ipc-protocol-claude-aligned.md` §3 verbatim.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use deepseek_tools::permission::{PermissionResult, PermissionUpdate};

/// Approval request emitted to the frontend as the `approval:request` event.
///
/// Fields mirror Claude Code's `tool_use` payload exactly:
/// - `tool_use_id` (not `toolCallId`)
/// - `input` (not `args`)
/// - `description` is a short human-readable label for the tool call
///   (currently the tool name) shown in the approval dialog header
///
/// Serialized as camelCase per IPC v2 (`docs/ipc-protocol-claude-aligned.md`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    /// Thread the tool call belongs to.
    pub thread_id: String,
    /// Stable id of the originating `tool_use` block.
    pub tool_use_id: String,
    /// Tool name (e.g. `"write_file"`).
    pub tool_name: String,
    /// Original tool input as the model produced it.
    pub input: serde_json::Value,
    /// Human-readable label for the approval dialog (currently the tool name).
    pub description: String,
    /// Working directory for the tool call, if relevant.
    pub cwd: Option<String>,
    /// The full `Ask` result that triggered this prompt — the dialog
    /// renders `message`, `decisionReason`, and `suggestions` from here.
    pub permission_result: PermissionResult,
}

/// User decision delivered via the `approve_tool` invoke.
///
/// `tag = "behavior"` + lowercase variant names produces JSON like
/// `{"behavior":"allow", ...}` matching Claude's `PermissionDecision` shape.
/// `rename_all_fields = "camelCase"` ensures inner fields serialize as
/// `updatedInput` and `permissionUpdates` (not snake_case).
///
/// "Allow once" produces `permission_updates = []`.
/// "Allow always (this session)" produces a single
/// [`PermissionUpdate::AddRules`] with `destination = Session`.
#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "behavior",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum ApprovalDecision {
    /// User approved the call (possibly with edited input + remembered rules).
    Allow {
        /// Possibly edited tool input. P4 frontend does not surface editing,
        /// so this normally equals the original `input`.
        #[serde(default)]
        updated_input: serde_json::Value,
        /// Rules / mode changes captured at approval time. Empty for
        /// "Allow once".
        #[serde(default)]
        permission_updates: Vec<PermissionUpdate>,
    },
    /// User rejected the call. Optional `message` is fed back to the model
    /// as a `tool_result` so it can self-correct.
    Deny {
        /// Optional rejection feedback for the model.
        #[serde(default)]
        message: Option<String>,
    },
}

/// Errors raised by a [`PermissionGate`] implementation.
#[derive(Debug, Error)]
pub enum GateError {
    /// The frontend channel is gone (e.g. window closed before responding).
    #[error("approval channel closed")]
    Closed,
    /// Emitting the approval event failed (Tauri serialization / IPC error).
    #[error("emit failed: {0}")]
    Emit(String),
    /// The current turn was aborted before the user responded.
    #[error("aborted")]
    Aborted,
}

/// Async approval protocol used by the agent loop whenever a tool call needs
/// user confirmation.
///
/// Implementations:
/// - `TauriPermissionGate` (production, task 4.2): emits the `approval:request`
///   event and awaits the matching `approve_tool` invoke from the frontend.
/// - mock gates in tests: return canned decisions without any IPC.
#[async_trait]
pub trait PermissionGate: Send + Sync {
    /// Display the approval UI and wait for the user's decision.
    ///
    /// `abort` is the current turn's cancellation token; when it fires the
    /// gate should drop any pending request and return [`GateError::Aborted`].
    async fn ask(
        &self,
        req: ApprovalRequest,
        abort: CancellationToken,
    ) -> Result<ApprovalDecision, GateError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use deepseek_tools::permission::PermissionResult;
    use serde_json::json;

    #[test]
    fn approval_decision_allow_form() {
        let json = json!({
            "behavior": "allow",
            "updatedInput": { "x": 1 },
            "permissionUpdates": []
        });
        let dec: ApprovalDecision = serde_json::from_value(json).unwrap();
        match dec {
            ApprovalDecision::Allow {
                updated_input,
                permission_updates,
            } => {
                assert_eq!(updated_input, json!({ "x": 1 }));
                assert!(permission_updates.is_empty());
            }
            _ => panic!("expected allow"),
        }
    }

    #[test]
    fn approval_decision_deny_form() {
        let json = json!({ "behavior": "deny", "message": null });
        let dec: ApprovalDecision = serde_json::from_value(json).unwrap();
        assert!(matches!(dec, ApprovalDecision::Deny { message: None }));
    }

    #[test]
    fn approval_decision_deny_with_feedback() {
        let json = json!({ "behavior": "deny", "message": "don't do that" });
        let dec: ApprovalDecision = serde_json::from_value(json).unwrap();
        if let ApprovalDecision::Deny { message: Some(m) } = dec {
            assert_eq!(m, "don't do that");
        } else {
            panic!("expected deny with feedback");
        }
    }

    #[test]
    fn approval_request_camelcase() {
        let req = ApprovalRequest {
            thread_id: "t1".into(),
            tool_use_id: "u1".into(),
            tool_name: "write_file".into(),
            input: json!({"path": "/tmp"}),
            description: "write to /tmp".into(),
            cwd: Some("/repo".into()),
            permission_result: PermissionResult::Ask {
                message: "approve?".into(),
                decision_reason: None,
                suggestions: vec![],
            },
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["threadId"], "t1");
        assert_eq!(v["toolUseId"], "u1");
        assert_eq!(v["toolName"], "write_file");
    }
}
