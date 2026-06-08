//! Main permission decision flow.
//!
//! Mirrors `claude-code-rev/src/utils/permissions/permissions.ts:hasPermissionsToUseToolInner`.
//! The 9-step ordering is preserved verbatim — see the P4 design doc §4.2.
//!
//! The flow is:
//!
//! 1. Bypass-immune checks
//!    1a. Whole-tool deny rule wins outright.
//!    1b. Whole-tool ask rule short-circuits to `Ask`.
//!    1c. Tool-specific `check_permissions` runs.
//!    1d. Tool returned `Deny` → propagate.
//!    1e. Tool with `requires_user_interaction` + `Ask` → force ask.
//!    1f. Tool returned `Ask` driven by an `Ask` rule → respect (bypass-immune).
//!    1g. Tool returned `Ask` driven by `SafetyCheck` → respect (bypass-immune).
//! 2. Mode / rule short-circuits
//!    2a. `BypassPermissions` mode → allow.
//!    2b. Whole-tool allow rule → allow.
//! 3. Convert remaining `Passthrough` results to `Ask`.
//! 4. `DontAsk` mode converts any remaining `Ask` to `Deny`.

use deepseek_tools::permission::{
    DecisionReason, PermissionBehavior, PermissionMode, PermissionResult,
};
use deepseek_tools::Tool;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use super::context::ToolPermissionContext;

/// Decision flow error.
#[derive(Debug, Error)]
pub enum PermissionError {
    /// Aborted via cancellation token.
    #[error("aborted")]
    Aborted,
}

/// Run the 9-step decision flow against a tool.
pub async fn check_tool_permission(
    tool: &dyn Tool,
    input: &serde_json::Value,
    ctx: &ToolPermissionContext,
    abort: &CancellationToken,
) -> Result<PermissionResult, PermissionError> {
    if abort.is_cancelled() {
        return Err(PermissionError::Aborted);
    }

    // Extract the tool's permission-relevant content for content-aware rule
    // matching: the shell command string for run_command, the target path for
    // filesystem tools (via `get_path`). `None` → only whole-tool rules apply.
    let content = extract_match_content(tool, input);

    // 1a. Deny rule (content-aware: shell commands match per sub-command).
    if let Some(rule) = ctx.find_deny_rule_for_content(tool.name(), content.as_deref()) {
        return Ok(PermissionResult::Deny {
            message: format!("Permission to use {} has been denied.", tool.name()),
            decision_reason: Some(DecisionReason::Rule { rule: rule.clone() }),
        });
    }

    // 1b. Whole-tool ask rule
    if let Some(rule) = ctx.find_ask_rule_for_tool(tool.name()) {
        let reason = DecisionReason::Rule { rule: rule.clone() };
        return Ok(PermissionResult::Ask {
            message: build_request_message(tool.name(), Some(&reason)),
            decision_reason: Some(reason),
            suggestions: vec![],
        });
    }

    // 1c. Tool-specific check
    let tool_result = tool.check_permissions(input, ctx.mode).await;

    // 1d. Tool says deny
    if matches!(tool_result, PermissionResult::Deny { .. }) {
        return Ok(tool_result);
    }

    // 1e. Tool requires user interaction + ask → force ask
    if tool.requires_user_interaction() {
        if let PermissionResult::Ask { .. } = &tool_result {
            return Ok(tool_result);
        }
    }

    // 1f. Tool ask + DecisionReason::Rule(ask) → respect ask rule (bypass-immune)
    if let PermissionResult::Ask {
        decision_reason: Some(DecisionReason::Rule { rule }),
        ..
    } = &tool_result
    {
        if rule.rule_behavior == PermissionBehavior::Ask {
            return Ok(tool_result);
        }
    }

    // 1g. SafetyCheck → bypass-immune (P4 cannot trigger this; placeholder for P5).
    if let PermissionResult::Ask {
        decision_reason: Some(DecisionReason::SafetyCheck { .. }),
        ..
    } = &tool_result
    {
        return Ok(tool_result);
    }

    // 2a. Mode-based bypass (only applies after the bypass-immune checks above).
    if ctx.mode == PermissionMode::BypassPermissions {
        return Ok(PermissionResult::Allow {
            updated_input: extract_updated_input(&tool_result, input),
            decision_reason: Some(DecisionReason::Mode { mode: ctx.mode }),
            user_modified: None,
        });
    }

    // 2b. Whole-tool allow rule (content-aware: a shell allow must cover EVERY
    // sub-command, and never applies when command substitution is present).
    if let Some(rule) = ctx.find_allow_rule_for_content(tool.name(), content.as_deref()) {
        return Ok(PermissionResult::Allow {
            updated_input: extract_updated_input(&tool_result, input),
            decision_reason: Some(DecisionReason::Rule { rule: rule.clone() }),
            user_modified: None,
        });
    }

    // 3. Passthrough → ask (let the user decide).
    let result = match tool_result {
        PermissionResult::Passthrough { .. } => PermissionResult::Ask {
            message: build_request_message(tool.name(), None),
            decision_reason: None,
            suggestions: vec![],
        },
        other => other,
    };

    // 4. dontAsk: ask → deny
    if ctx.mode == PermissionMode::DontAsk {
        if let PermissionResult::Ask { message, .. } = result {
            return Ok(PermissionResult::Deny {
                message,
                decision_reason: Some(DecisionReason::Mode {
                    mode: PermissionMode::DontAsk,
                }),
            });
        }
    }

    Ok(result)
}

/// Pull the (possibly edited) input out of an `Allow` tool result, falling
/// back to the original input for non-allow shapes.
fn extract_updated_input(
    tool_result: &PermissionResult,
    fallback: &serde_json::Value,
) -> serde_json::Value {
    if let PermissionResult::Allow { updated_input, .. } = tool_result {
        updated_input.clone()
    } else {
        fallback.clone()
    }
}

/// Extract the content used for content-aware rule matching:
/// - `run_command` → the `command` string.
/// - filesystem tools → the target path (via `Tool::get_path`).
/// - others → `None` (only whole-tool rules apply).
fn extract_match_content(tool: &dyn Tool, input: &serde_json::Value) -> Option<String> {
    if tool.name() == "run_command" {
        return input
            .get("command")
            .and_then(|v| v.as_str())
            .map(str::to_string);
    }
    tool.get_path(input)
}

/// Build the prompt message shown for an `Ask` decision.
fn build_request_message(tool_name: &str, reason: Option<&DecisionReason>) -> String {
    match reason {
        Some(DecisionReason::Mode {
            mode: PermissionMode::Plan,
        }) => format!("Plan mode: {tool_name} requires approval to mutate state."),
        Some(DecisionReason::Rule { rule }) => format!(
            "Rule from `{:?}` requires approval for {tool_name}.",
            rule.source
        ),
        _ => format!("Permission to use {tool_name} is required, but you haven't granted it yet."),
    }
}

#[cfg(test)]
mod check_tests {
    use super::*;
    use crate::permission::ToolPermissionContext;
    use async_trait::async_trait;
    use deepseek_tools::permission::{
        PermissionBehavior, PermissionMode, PermissionResult, PermissionRule, PermissionRuleSource,
        PermissionRuleValue,
    };
    use deepseek_tools::types::ToolError;
    use deepseek_tools::Tool;
    use serde_json::{json, Value};
    use tokio_util::sync::CancellationToken;

    /// Mock tool that returns a fixed `PermissionResult` from
    /// `check_permissions` and never executes anything meaningful.
    struct MockTool {
        name: &'static str,
        result: PermissionResult,
    }

    #[async_trait]
    impl Tool for MockTool {
        fn name(&self) -> &str {
            self.name
        }
        fn is_read_only(&self) -> bool {
            false
        }
        async fn execute(
            &self,
            _args: Value,
            _ctx: &deepseek_tools::ToolContext,
        ) -> Result<String, ToolError> {
            Ok(String::new())
        }
        async fn check_permissions(
            &self,
            _input: &Value,
            _mode: PermissionMode,
        ) -> PermissionResult {
            self.result.clone()
        }
    }

    fn passthrough() -> PermissionResult {
        PermissionResult::Passthrough {
            message: "p".into(),
        }
    }

    fn rule(behavior: PermissionBehavior, tool_name: &str) -> PermissionRule {
        PermissionRule {
            source: PermissionRuleSource::Session,
            rule_behavior: behavior,
            rule_value: PermissionRuleValue {
                tool_name: tool_name.into(),
                rule_content: None,
            },
        }
    }

    #[tokio::test]
    async fn step_1a_deny_rule_wins() {
        let mut ctx = ToolPermissionContext::default();
        ctx.add_rule(rule(PermissionBehavior::Deny, "x"));
        let tool = MockTool {
            name: "x",
            result: passthrough(),
        };
        let r = check_tool_permission(&tool, &json!({}), &ctx, &CancellationToken::new())
            .await
            .unwrap();
        assert!(matches!(r, PermissionResult::Deny { .. }));
    }

    #[tokio::test]
    async fn step_1d_tool_deny_propagates() {
        let ctx = ToolPermissionContext::default();
        let tool = MockTool {
            name: "x",
            result: PermissionResult::Deny {
                message: "no".into(),
                decision_reason: None,
            },
        };
        let r = check_tool_permission(&tool, &json!({}), &ctx, &CancellationToken::new())
            .await
            .unwrap();
        assert!(matches!(r, PermissionResult::Deny { .. }));
    }

    #[tokio::test]
    async fn step_2a_bypass_mode_allows_passthrough_tools() {
        let ctx = ToolPermissionContext::new(PermissionMode::BypassPermissions);
        let tool = MockTool {
            name: "x",
            result: passthrough(),
        };
        let r = check_tool_permission(&tool, &json!({"k":"v"}), &ctx, &CancellationToken::new())
            .await
            .unwrap();
        match r {
            PermissionResult::Allow {
                updated_input,
                decision_reason,
                ..
            } => {
                assert_eq!(updated_input, json!({"k": "v"}));
                assert!(matches!(
                    decision_reason,
                    Some(DecisionReason::Mode {
                        mode: PermissionMode::BypassPermissions
                    })
                ));
            }
            other => panic!("expected allow, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn bypass_does_not_override_deny_rule() {
        let mut ctx = ToolPermissionContext::new(PermissionMode::BypassPermissions);
        ctx.add_rule(rule(PermissionBehavior::Deny, "x"));
        let tool = MockTool {
            name: "x",
            result: passthrough(),
        };
        let r = check_tool_permission(&tool, &json!({}), &ctx, &CancellationToken::new())
            .await
            .unwrap();
        assert!(
            matches!(r, PermissionResult::Deny { .. }),
            "deny rule must win over bypass"
        );
    }

    #[tokio::test]
    async fn step_2b_allow_rule_grants_passthrough_tools() {
        let mut ctx = ToolPermissionContext::default();
        ctx.add_rule(rule(PermissionBehavior::Allow, "x"));
        let tool = MockTool {
            name: "x",
            result: passthrough(),
        };
        let r = check_tool_permission(&tool, &json!({}), &ctx, &CancellationToken::new())
            .await
            .unwrap();
        match r {
            PermissionResult::Allow {
                decision_reason: Some(DecisionReason::Rule { rule }),
                ..
            } => {
                assert_eq!(rule.rule_behavior, PermissionBehavior::Allow);
                assert_eq!(rule.rule_value.tool_name, "x");
            }
            other => panic!("expected allow via rule, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn step_3_passthrough_becomes_ask() {
        let ctx = ToolPermissionContext::default();
        let tool = MockTool {
            name: "x",
            result: passthrough(),
        };
        let r = check_tool_permission(&tool, &json!({}), &ctx, &CancellationToken::new())
            .await
            .unwrap();
        assert!(matches!(r, PermissionResult::Ask { .. }));
    }

    #[tokio::test]
    async fn step_4_dontask_converts_ask_to_deny() {
        let ctx = ToolPermissionContext::new(PermissionMode::DontAsk);
        let tool = MockTool {
            name: "x",
            result: passthrough(),
        };
        let r = check_tool_permission(&tool, &json!({}), &ctx, &CancellationToken::new())
            .await
            .unwrap();
        match r {
            PermissionResult::Deny {
                decision_reason:
                    Some(DecisionReason::Mode {
                        mode: PermissionMode::DontAsk,
                    }),
                ..
            } => {}
            other => panic!("expected deny via dontAsk, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn aborted_token_returns_error() {
        let ctx = ToolPermissionContext::default();
        let tool = MockTool {
            name: "x",
            result: passthrough(),
        };
        let abort = CancellationToken::new();
        abort.cancel();
        let r = check_tool_permission(&tool, &json!({}), &ctx, &abort).await;
        assert!(matches!(r, Err(PermissionError::Aborted)));
    }

    // ── content-aware shell rule matching (P1-2 / P1-3) ──────────────────

    fn content_rule(
        behavior: PermissionBehavior,
        tool_name: &str,
        content: &str,
    ) -> PermissionRule {
        PermissionRule {
            source: PermissionRuleSource::Session,
            rule_behavior: behavior,
            rule_value: PermissionRuleValue {
                tool_name: tool_name.into(),
                rule_content: Some(content.into()),
            },
        }
    }

    /// An allow rule for `git status` must NOT green-light a compound command
    /// whose other fragment (`rm -rf /`) is uncovered. This is the core
    /// anti-bypass guarantee — it must fall through to Ask, not Allow.
    #[tokio::test]
    async fn allow_rule_does_not_cover_uncovered_compound_fragment() {
        let mut ctx = ToolPermissionContext::default();
        ctx.add_rule(content_rule(
            PermissionBehavior::Allow,
            "run_command",
            "git status",
        ));
        let tool = MockTool {
            name: "run_command",
            result: passthrough(),
        };
        let r = check_tool_permission(
            &tool,
            &json!({ "command": "git status && rm -rf /" }),
            &ctx,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert!(
            matches!(r, PermissionResult::Ask { .. }),
            "compound command with an uncovered fragment must NOT be auto-allowed, got {r:?}"
        );
    }

    /// When EVERY fragment is covered by an allow rule, the compound command
    /// is allowed.
    #[tokio::test]
    async fn allow_rule_covers_all_fragments() {
        let mut ctx = ToolPermissionContext::default();
        ctx.add_rule(content_rule(
            PermissionBehavior::Allow,
            "run_command",
            "git:*",
        ));
        let tool = MockTool {
            name: "run_command",
            result: passthrough(),
        };
        let r = check_tool_permission(
            &tool,
            &json!({ "command": "git status && git log" }),
            &ctx,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert!(
            matches!(r, PermissionResult::Allow { .. }),
            "all fragments covered by 'git:*' should allow, got {r:?}"
        );
    }

    /// A deny rule matching ANY sub-command denies the whole compound command.
    #[tokio::test]
    async fn deny_rule_matches_any_subcommand() {
        let mut ctx = ToolPermissionContext::default();
        ctx.add_rule(content_rule(
            PermissionBehavior::Deny,
            "run_command",
            "rm:*",
        ));
        let tool = MockTool {
            name: "run_command",
            result: passthrough(),
        };
        let r = check_tool_permission(
            &tool,
            &json!({ "command": "echo hi && rm -rf /tmp/x" }),
            &ctx,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert!(
            matches!(r, PermissionResult::Deny { .. }),
            "deny rule matching a sub-command must deny the whole line, got {r:?}"
        );
    }

    /// Command substitution can't be statically proven safe, so an allow rule
    /// must never auto-allow a command containing `$(…)`.
    #[tokio::test]
    async fn allow_rule_rejected_with_command_substitution() {
        let mut ctx = ToolPermissionContext::default();
        ctx.add_rule(content_rule(
            PermissionBehavior::Allow,
            "run_command",
            "echo:*",
        ));
        let tool = MockTool {
            name: "run_command",
            result: passthrough(),
        };
        let r = check_tool_permission(
            &tool,
            &json!({ "command": "echo $(rm -rf /)" }),
            &ctx,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert!(
            matches!(r, PermissionResult::Ask { .. }),
            "command substitution must block auto-allow, got {r:?}"
        );
    }
}
