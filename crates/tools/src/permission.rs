//! Permission types — direct mirror of `claude-code-rev/src/types/permissions.ts`
//! and `claude-code-rev/src/utils/permissions/PermissionRule.ts`.
//!
//! All structs use `#[serde(rename_all = "camelCase")]`. JSON values match the
//! wire protocol defined in `docs/ipc-protocol-claude-aligned.md` §2.
//!
//! These types live in the `tools` crate (rather than `core`) because the
//! [`crate::Tool`] trait will surface a `check_permissions` hook in later
//! tasks; placing them here avoids a `core → tools` reverse dependency.
//! `core` re-exports the whole module via `pub use deepseek_tools::permission::*;`.

use serde::{Deserialize, Serialize};

/// Permission mode. JSON values: `default | plan | acceptEdits | bypassPermissions | dontAsk`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    /// Default mode. Mutating tools prompt for approval.
    #[default]
    Default,
    /// Plan mode. Read-only tools auto-allow; mutating tools blocked + system prompt
    /// instructs the model to only read.
    Plan,
    /// Accept-edits mode. File edits auto-allow within cwd; shell still prompts.
    AcceptEdits,
    /// Bypass permissions mode. Skips ask, but deny rules and safety checks still apply.
    BypassPermissions,
    /// Don't-ask mode. All `ask` results auto-convert to `deny`.
    DontAsk,
}

impl PermissionMode {
    /// String form used in DB and JSON. Matches the camelCase serialization.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Plan => "plan",
            Self::AcceptEdits => "acceptEdits",
            Self::BypassPermissions => "bypassPermissions",
            Self::DontAsk => "dontAsk",
        }
    }

    /// Parse from string form. Falls back to [`PermissionMode::Default`] on unknown input.
    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "plan" => Self::Plan,
            "acceptEdits" => Self::AcceptEdits,
            "bypassPermissions" => Self::BypassPermissions,
            "dontAsk" => Self::DontAsk,
            _ => Self::Default,
        }
    }
}

/// Rule behavior. JSON: `allow | deny | ask`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionBehavior {
    /// Auto-allow without prompting.
    Allow,
    /// Hard deny — never prompts the user.
    Deny,
    /// Prompt the user for approval.
    Ask,
}

/// Rule source. P4 only supports [`PermissionRuleSource::Session`]. Other variants
/// are protocol-stable placeholders for roadmap items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionRuleSource {
    /// In-memory rules added by "Allow always" during the session.
    Session,
    /// `~/.claude/settings.json` (roadmap).
    UserSettings,
    /// `<project>/.claude/settings.json` (roadmap).
    ProjectSettings,
    /// `<project>/.claude/settings.local.json` (roadmap).
    LocalSettings,
    /// CLI `--allow` / `--deny` flags (roadmap).
    CliArg,
    /// Enterprise policy settings, read-only (roadmap).
    PolicySettings,
    /// Feature flag settings, read-only (roadmap).
    FlagSettings,
    /// Temporary rule from `/allow` slash command (roadmap).
    Command,
}

/// Rule value: tool name + optional sub-content (for ruleContent matching).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRuleValue {
    /// Tool name to match (e.g. `"write_file"`).
    pub tool_name: String,
    /// Sub-content matcher. P4 always [`None`] — Roadmap GAP-PERM-001.
    pub rule_content: Option<String>,
}

/// Permission rule (source + behavior + value).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRule {
    /// Where the rule originated.
    pub source: PermissionRuleSource,
    /// Effect of the rule.
    pub rule_behavior: PermissionBehavior,
    /// Tool / content matcher.
    pub rule_value: PermissionRuleValue,
}

/// Decision reason. P4 emits `Rule | Mode | Other | WorkingDir`. Other variants
/// are protocol-stable placeholders for later phases.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DecisionReason {
    /// Decision was driven by an explicit [`PermissionRule`].
    Rule {
        /// Matching rule.
        rule: PermissionRule,
    },
    /// Decision was driven by the active [`PermissionMode`].
    Mode {
        /// Active mode at decision time.
        mode: PermissionMode,
    },
    /// Decision came from a hook (P5).
    Hook {
        /// Hook identifier.
        hook_name: String,
        /// Optional human-readable reason.
        reason: Option<String>,
    },
    /// Decision came from the safety classifier (P5).
    SafetyCheck {
        /// Human-readable reason.
        reason: String,
        /// Whether the classifier deems this approvable.
        classifier_approvable: bool,
    },
    /// Decision driven by working-directory boundary (P5).
    WorkingDir {
        /// Human-readable reason.
        reason: String,
    },
    /// Generic fallback reason.
    Other {
        /// Human-readable reason.
        reason: String,
    },
    /// Decision came from an async sub-agent (P6+).
    AsyncAgent {
        /// Human-readable reason.
        reason: String,
    },
}

/// Permission update emitted by an "Allow always" approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PermissionUpdate {
    /// Append rules to the destination scope.
    AddRules {
        /// Rule values being added.
        rules: Vec<PermissionRuleValue>,
        /// Behavior associated with the rules.
        behavior: PermissionBehavior,
        /// Where to persist the rules.
        destination: PermissionRuleSource,
    },
    /// Replace all rules at the destination scope.
    ReplaceRules {
        /// New rule set.
        rules: Vec<PermissionRuleValue>,
        /// Behavior associated with the rules.
        behavior: PermissionBehavior,
        /// Destination scope.
        destination: PermissionRuleSource,
    },
    /// Remove specified rules from the destination scope.
    RemoveRules {
        /// Rule values being removed.
        rules: Vec<PermissionRuleValue>,
        /// Behavior of the rules being removed.
        behavior: PermissionBehavior,
        /// Destination scope.
        destination: PermissionRuleSource,
    },
    /// Switch the active mode at the destination scope.
    SetMode {
        /// New mode.
        mode: PermissionMode,
        /// Destination scope.
        destination: PermissionRuleSource,
    },
    /// Append additional working directories.
    AddDirectories {
        /// Directories to add.
        directories: Vec<String>,
        /// Destination scope.
        destination: PermissionRuleSource,
    },
    /// Remove working directories.
    RemoveDirectories {
        /// Directories to remove.
        directories: Vec<String>,
        /// Destination scope.
        destination: PermissionRuleSource,
    },
}

/// Permission result. Mirrors Claude `PermissionResult`.
///
/// `Passthrough` is internal-only and used when a tool's own
/// `check_permissions` abstains. The main decision flow translates it to
/// [`PermissionResult::Ask`] before crossing the IPC boundary.
///
/// NOTE: `decision_reason` and `user_modified` carry
/// `#[serde(skip_serializing_if = "Option::is_none", default)]` as a temporary
/// concession during the v1 → v2 frontend migration. Per
/// `docs/ipc-protocol-claude-aligned.md` §6 these should become explicit
/// `null`s once the v2 frontend is fully verified — task 7.x will unify the
/// final field shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "behavior",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum PermissionResult {
    /// Allow execution (possibly with edited input).
    Allow {
        /// The (possibly edited) tool input that should actually be executed.
        updated_input: serde_json::Value,
        /// Why this decision was reached.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        decision_reason: Option<DecisionReason>,
        /// `true` when the user edited the input in the approval dialog.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        user_modified: Option<bool>,
    },
    /// Hard deny.
    Deny {
        /// Human-readable rejection message.
        message: String,
        /// Why this decision was reached.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        decision_reason: Option<DecisionReason>,
    },
    /// Prompt the user for approval.
    Ask {
        /// Human-readable approval prompt.
        message: String,
        /// Why approval is being requested.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        decision_reason: Option<DecisionReason>,
        /// Suggested permission updates (e.g. "Allow always" presets).
        #[serde(default)]
        suggestions: Vec<PermissionUpdate>,
    },
    /// Internal-only: tool abstains, decide via rules or fall back to ask.
    /// **Never crosses IPC** — main decision flow converts to [`PermissionResult::Ask`].
    Passthrough {
        /// Diagnostic message for logging.
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn permission_mode_serializes_camelcase() {
        for (mode, expected) in [
            (PermissionMode::Default, "default"),
            (PermissionMode::Plan, "plan"),
            (PermissionMode::AcceptEdits, "acceptEdits"),
            (PermissionMode::BypassPermissions, "bypassPermissions"),
            (PermissionMode::DontAsk, "dontAsk"),
        ] {
            let v = serde_json::to_value(mode).unwrap();
            assert_eq!(v, json!(expected));
            let back: PermissionMode = serde_json::from_value(v).unwrap();
            assert_eq!(back, mode);
        }
    }

    #[test]
    fn permission_rule_round_trip() {
        let rule = PermissionRule {
            source: PermissionRuleSource::Session,
            rule_behavior: PermissionBehavior::Allow,
            rule_value: PermissionRuleValue {
                tool_name: "write_file".into(),
                rule_content: None,
            },
        };
        let json = serde_json::to_value(&rule).unwrap();
        assert_eq!(
            json,
            json!({
                "source": "session",
                "ruleBehavior": "allow",
                "ruleValue": { "toolName": "write_file", "ruleContent": null }
            })
        );
        let back: PermissionRule = serde_json::from_value(json).unwrap();
        assert_eq!(back, rule);
    }

    #[test]
    fn decision_reason_tag_field() {
        let r = DecisionReason::Mode {
            mode: PermissionMode::Plan,
        };
        assert_eq!(
            serde_json::to_value(&r).unwrap(),
            json!({ "type": "mode", "mode": "plan" })
        );
    }

    #[test]
    fn permission_update_addrules_form() {
        let u = PermissionUpdate::AddRules {
            rules: vec![PermissionRuleValue {
                tool_name: "run_command".into(),
                rule_content: None,
            }],
            behavior: PermissionBehavior::Allow,
            destination: PermissionRuleSource::Session,
        };
        let v = serde_json::to_value(&u).unwrap();
        assert_eq!(
            v,
            json!({
                "type": "addRules",
                "rules": [{ "toolName": "run_command", "ruleContent": null }],
                "behavior": "allow",
                "destination": "session"
            })
        );
    }

    #[test]
    fn permission_result_ask_form() {
        let r = PermissionResult::Ask {
            message: "approve please".into(),
            decision_reason: Some(DecisionReason::Mode {
                mode: PermissionMode::Default,
            }),
            suggestions: vec![],
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["behavior"], "ask");
        assert_eq!(v["message"], "approve please");
        assert_eq!(v["decisionReason"]["type"], "mode");
    }
}
