//! TauriPermissionGate — emits the `approval:request` event and awaits the
//! matching `approve_tool` invoke from the frontend.
//!
//! The gate parks each pending request in a [`DashMap`] keyed by
//! `tool_use_id`. When the frontend calls `approve_tool`, the command
//! handler resolves the corresponding [`oneshot::Sender`] via
//! [`TauriPermissionGate::feed_response`].

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use deepseek_core::gate::{ApprovalDecision, ApprovalRequest, GateError, PermissionGate};

/// Maximum time the gate waits for a user decision before auto-denying.
/// Prevents engine threads from blocking indefinitely if the user walks away
/// or the frontend fails to render the approval dialog.
const GATE_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

/// Tauri-backed [`PermissionGate`] implementation.
///
/// Each call to [`PermissionGate::ask`]:
/// 1. Registers a [`oneshot::Sender`] under the request's `tool_use_id`.
/// 2. Emits `approval:request` so the frontend can render the dialog.
/// 3. Awaits either the matching `approve_tool` invoke or an abort signal.
pub struct TauriPermissionGate {
    app: AppHandle,
    pending: Arc<DashMap<String, oneshot::Sender<ApprovalDecision>>>,
}

impl TauriPermissionGate {
    /// Create a gate bound to the given [`AppHandle`].
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            pending: Arc::new(DashMap::new()),
        }
    }

    /// Deliver a frontend decision for the given `tool_use_id`.
    ///
    /// Returns `true` when a pending request was matched **and** the receiver
    /// was still listening. A decision for a non-pending request is rejected:
    /// permissions must fail closed and may never be pre-approved.
    pub fn feed_response(&self, tool_use_id: &str, decision: ApprovalDecision) -> bool {
        if let Some((_, sender)) = self.pending.remove(tool_use_id) {
            return sender.send(decision).is_ok();
        }
        tracing::warn!(
            tool_use_id,
            "approval decision ignored because no backend gate is pending"
        );
        false
    }
}

#[async_trait]
impl PermissionGate for TauriPermissionGate {
    async fn ask(
        &self,
        req: ApprovalRequest,
        abort: CancellationToken,
    ) -> Result<ApprovalDecision, GateError> {
        let id = req.tool_use_id.clone();
        if abort.is_cancelled() {
            return Err(GateError::Aborted);
        }
        let (tx, rx) = oneshot::channel();
        self.pending.insert(id.clone(), tx);
        if let Err(e) = self.app.emit("approval:request", &req) {
            self.pending.remove(&id);
            return Err(GateError::Emit(e.to_string()));
        }
        let result = tokio::select! {
            biased;
            _ = abort.cancelled() => {
                self.pending.remove(&id);
                Err(GateError::Aborted)
            }
            _ = tokio::time::sleep(GATE_TIMEOUT) => {
                self.pending.remove(&id);
                tracing::warn!(tool_use_id = %id, "approval gate timed out after 5min — auto-denying");
                Err(GateError::Closed)
            }
            r = rx => r.map_err(|_| GateError::Closed),
        };
        // Best-effort cleanup if `select` resolved via the receiver path.
        self.pending.remove(&id);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time assertion that [`TauriPermissionGate`] is a valid
    /// [`PermissionGate`] implementation. Runtime testing requires a real
    /// `AppHandle`, so we leave that to the integration phase.
    #[allow(dead_code)]
    fn _is_gate(g: TauriPermissionGate) -> Box<dyn PermissionGate> {
        Box::new(g)
    }
}
