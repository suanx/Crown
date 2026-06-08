//! Cancellation utilities — extends tokio_util::sync::CancellationToken with
//! ergonomic helpers used by the agent engine and shell tool.

use tokio_util::sync::CancellationToken;

/// Wrapper around `parent.child_token()` for clarity at call sites.
///
/// Aborting the parent cancels the child; aborting the child does NOT cancel
/// the parent. tokio_util internally uses weak references, so dropping the
/// child does not leak listeners on the parent (verified in tokio_util docs).
pub fn child_token(parent: &CancellationToken) -> CancellationToken {
    parent.child_token()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parent_cancel_propagates_to_child() {
        let parent = CancellationToken::new();
        let child = child_token(&parent);
        parent.cancel();
        assert!(child.is_cancelled());
    }

    #[tokio::test]
    async fn child_cancel_does_not_affect_parent() {
        let parent = CancellationToken::new();
        let child = child_token(&parent);
        child.cancel();
        assert!(!parent.is_cancelled());
    }

    #[tokio::test]
    async fn cancel_during_select_unblocks() {
        let token = CancellationToken::new();
        let t2 = token.clone();
        let handle = tokio::spawn(async move {
            tokio::select! {
                _ = t2.cancelled() => "cancelled",
                _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => "timeout",
            }
        });
        token.cancel();
        let result = handle.await.unwrap();
        assert_eq!(result, "cancelled");
    }
}
