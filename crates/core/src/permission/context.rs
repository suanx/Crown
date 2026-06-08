//! ToolPermissionContext — per-thread permission state.
//!
//! Mirrors `claude-code-rev/src/Tool.ts:ToolPermissionContext`. Rules are
//! bucketed by `(behavior, source)` for fast lookup. Within a bucket, rules
//! match by `tool_name`. P4 only matches when `rule_content.is_none()`;
//! sub-content matching is Roadmap GAP-PERM-001.

use std::collections::HashMap;

use deepseek_tools::permission::{
    PermissionBehavior, PermissionMode, PermissionRule, PermissionRuleSource, PermissionUpdate,
};

/// Per-thread permission context.
#[derive(Debug, Clone, Default)]
pub struct ToolPermissionContext {
    /// Currently active mode for the owning thread.
    pub mode: PermissionMode,
    /// Rules grouped by (behavior, source).
    rules: HashMap<(PermissionBehavior, PermissionRuleSource), Vec<PermissionRule>>,
    /// Additional working directories. Roadmap GAP-PERM-008.
    pub additional_working_directories: Vec<String>,
    /// Whether bypass mode is permitted. Roadmap GAP-PERM-007.
    pub is_bypass_permissions_mode_available: bool,
    /// Current working directory (project root). Used by file rule matching
    /// to resolve `/`-prefixed patterns relative to the project.
    pub cwd: Option<String>,
}

impl ToolPermissionContext {
    /// Construct with the given mode (and otherwise default state).
    pub fn new(mode: PermissionMode) -> Self {
        Self {
            mode,
            ..Default::default()
        }
    }

    /// Add a rule into its `(behavior, source)` bucket.
    pub fn add_rule(&mut self, rule: PermissionRule) {
        let key = (rule.rule_behavior, rule.source);
        self.rules.entry(key).or_default().push(rule);
    }

    /// Find an allow rule matching `tool_name`. Returns the first hit per
    /// Claude's source priority.
    pub fn find_allow_rule_for_tool(&self, tool_name: &str) -> Option<&PermissionRule> {
        self.find_rule_in_bucket(PermissionBehavior::Allow, tool_name)
    }

    /// Find a deny rule matching `tool_name`.
    pub fn find_deny_rule_for_tool(&self, tool_name: &str) -> Option<&PermissionRule> {
        self.find_rule_in_bucket(PermissionBehavior::Deny, tool_name)
    }

    /// Find an ask rule matching `tool_name`.
    pub fn find_ask_rule_for_tool(&self, tool_name: &str) -> Option<&PermissionRule> {
        self.find_rule_in_bucket(PermissionBehavior::Ask, tool_name)
    }

    /// Find a deny rule that applies to `tool_name` given the tool's actual
    /// content (shell command or file path). For shell commands, the command
    /// is split into sub-commands and a deny matches if it covers ANY
    /// sub-command (deny is conservative — one dangerous fragment denies the
    /// whole line). Falls back to whole-tool deny rules.
    pub fn find_deny_rule_for_content(
        &self,
        tool_name: &str,
        content: Option<&str>,
    ) -> Option<&PermissionRule> {
        // Whole-tool deny (no content) always applies.
        if let Some(r) =
            self.find_rule_in_bucket_with_content(PermissionBehavior::Deny, tool_name, None)
        {
            return Some(r);
        }
        let content = content?;
        for frag in self.fragments_for(tool_name, content) {
            if let Some(r) = self.find_rule_with_content(PermissionBehavior::Deny, tool_name, &frag)
            {
                return Some(r);
            }
        }
        None
    }

    /// Find an allow rule that fully covers `tool_name` for the given content.
    ///
    /// Safety: for shell commands the command is split into sub-commands and
    /// the allow only holds when EVERY sub-command is individually covered by
    /// an allow rule AND the command contains no command substitution
    /// (`$(…)` / backticks) — otherwise an allow for `git status` would green-
    /// light `git status && rm -rf /` or `echo $(rm -rf /)`. Returns the rule
    /// matching the FIRST fragment (for reason reporting) only when all
    /// fragments are covered.
    pub fn find_allow_rule_for_content(
        &self,
        tool_name: &str,
        content: Option<&str>,
    ) -> Option<&PermissionRule> {
        // Whole-tool allow (no content) applies to any input.
        if let Some(r) =
            self.find_rule_in_bucket_with_content(PermissionBehavior::Allow, tool_name, None)
        {
            return Some(r);
        }
        let content = content?;

        // Command substitution can't be statically proven safe → never allow.
        if tool_name == "run_command"
            && deepseek_tools::shell_rule_matching::has_command_substitution(content)
        {
            return None;
        }

        let fragments = self.fragments_for(tool_name, content);
        if fragments.is_empty() {
            return None;
        }
        // EVERY fragment must be covered by an allow rule.
        let mut first_hit: Option<&PermissionRule> = None;
        for frag in &fragments {
            match self.find_rule_with_content(PermissionBehavior::Allow, tool_name, frag) {
                Some(r) => {
                    if first_hit.is_none() {
                        first_hit = Some(r);
                    }
                }
                None => return None, // an uncovered fragment → not fully allowed
            }
        }
        first_hit
    }

    /// Decompose a tool's content into the fragments that must each be matched.
    /// Shell commands split on operators; everything else is a single fragment.
    fn fragments_for(&self, tool_name: &str, content: &str) -> Vec<String> {
        if tool_name == "run_command" {
            deepseek_tools::shell_rule_matching::split_shell_command(content)
        } else {
            vec![content.to_string()]
        }
    }

    /// Source-priority lookup. Matches by tool_name AND optionally by
    /// ruleContent using the appropriate matcher for the tool type:
    /// - Shell tools (run_command) → shell_rule_matching
    /// - Filesystem tools → file_rule_matching
    /// - Others → exact string comparison
    fn find_rule_in_bucket(
        &self,
        behavior: PermissionBehavior,
        tool_name: &str,
    ) -> Option<&PermissionRule> {
        self.find_rule_in_bucket_with_content(behavior, tool_name, None)
    }

    /// Extended lookup that also matches against ruleContent when provided.
    /// `content_to_match` is the tool's actual input content (e.g., the shell
    /// command string for run_command, or the file path for write_file).
    pub fn find_rule_with_content(
        &self,
        behavior: PermissionBehavior,
        tool_name: &str,
        content: &str,
    ) -> Option<&PermissionRule> {
        self.find_rule_in_bucket_with_content(behavior, tool_name, Some(content))
    }

    /// Internal helper for rule lookup with optional content matching.
    fn find_rule_in_bucket_with_content(
        &self,
        behavior: PermissionBehavior,
        tool_name: &str,
        content_to_match: Option<&str>,
    ) -> Option<&PermissionRule> {
        const ORDER: [PermissionRuleSource; 8] = [
            PermissionRuleSource::PolicySettings,
            PermissionRuleSource::FlagSettings,
            PermissionRuleSource::UserSettings,
            PermissionRuleSource::ProjectSettings,
            PermissionRuleSource::LocalSettings,
            PermissionRuleSource::CliArg,
            PermissionRuleSource::Command,
            PermissionRuleSource::Session,
        ];
        for source in ORDER {
            if let Some(bucket) = self.rules.get(&(behavior, source)) {
                if let Some(hit) = bucket.iter().find(|r| {
                    if r.rule_value.tool_name != tool_name {
                        return false;
                    }
                    match (&r.rule_value.rule_content, content_to_match) {
                        // Tool-wide rule (no content) always matches
                        (None, _) => true,
                        // Rule has content but we have nothing to match against → skip
                        (Some(_), None) => false,
                        // Both have content → use appropriate matcher
                        (Some(rule_content), Some(actual)) => {
                            self.matches_rule_content(tool_name, rule_content, actual)
                        }
                    }
                }) {
                    return Some(hit);
                }
            }
        }
        None
    }

    /// Dispatch to the appropriate content matcher based on tool type.
    fn matches_rule_content(&self, tool_name: &str, rule_content: &str, actual: &str) -> bool {
        use deepseek_tools::file_rule_matching::matches_file_rule;
        use deepseek_tools::shell_rule_matching::{matches_shell_rule, parse_shell_rule};

        match tool_name {
            "run_command" => {
                let rule = parse_shell_rule(rule_content);
                matches_shell_rule(&rule, actual)
            }
            "write_file" | "edit_file" | "read_file" | "list_directory" | "grep" | "glob" => {
                matches_file_rule(rule_content, actual, self.cwd.as_deref())
            }
            _ => {
                // Generic: exact string comparison
                rule_content == actual
            }
        }
    }

    /// All rules across all buckets (for `getPermissionContext` / settings UI).
    pub fn list_rules(&self) -> Vec<PermissionRule> {
        self.rules.values().flatten().cloned().collect()
    }

    /// All allow rules (for the dto's `alwaysAllowRules` field).
    pub fn list_allow_rules(&self) -> Vec<PermissionRule> {
        self.rules
            .iter()
            .filter(|((b, _), _)| *b == PermissionBehavior::Allow)
            .flat_map(|(_, v)| v.clone())
            .collect()
    }

    /// All deny rules.
    pub fn list_deny_rules(&self) -> Vec<PermissionRule> {
        self.rules
            .iter()
            .filter(|((b, _), _)| *b == PermissionBehavior::Deny)
            .flat_map(|(_, v)| v.clone())
            .collect()
    }

    /// All ask rules.
    pub fn list_ask_rules(&self) -> Vec<PermissionRule> {
        self.rules
            .iter()
            .filter(|((b, _), _)| *b == PermissionBehavior::Ask)
            .flat_map(|(_, v)| v.clone())
            .collect()
    }

    /// Apply a permission update (e.g. from "Allow always" approval).
    ///
    /// P4 implements `AddRules`, `SetMode`, `RemoveRules`. Other variants log
    /// a warning and are no-ops; they map to roadmap items.
    pub fn apply_update(&mut self, update: &PermissionUpdate) {
        match update {
            PermissionUpdate::AddRules {
                rules,
                behavior,
                destination,
            } => {
                for rv in rules {
                    self.add_rule(PermissionRule {
                        source: *destination,
                        rule_behavior: *behavior,
                        rule_value: rv.clone(),
                    });
                }
            }
            PermissionUpdate::SetMode { mode, .. } => {
                self.mode = *mode;
            }
            PermissionUpdate::RemoveRules {
                rules,
                behavior,
                destination,
            } => {
                let key = (*behavior, *destination);
                if let Some(bucket) = self.rules.get_mut(&key) {
                    bucket.retain(|r| !rules.iter().any(|rv| rv == &r.rule_value));
                }
            }
            other => {
                tracing::warn!(
                    ?other,
                    "PermissionUpdate variant not yet implemented; ignored"
                );
            }
        }
    }

    /// Remove a specific rule (used by `removePermissionRule` command).
    pub fn remove_rule(&mut self, rule: &PermissionRule) {
        let key = (rule.rule_behavior, rule.source);
        if let Some(bucket) = self.rules.get_mut(&key) {
            bucket.retain(|r| r.rule_value != rule.rule_value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::{
        PermissionBehavior, PermissionRule, PermissionRuleSource, PermissionRuleValue,
        PermissionUpdate,
    };

    #[test]
    fn empty_context_has_no_rules() {
        let ctx = ToolPermissionContext::default();
        assert!(ctx.find_allow_rule_for_tool("read_file").is_none());
        assert!(ctx.find_deny_rule_for_tool("write_file").is_none());
        assert!(ctx.find_ask_rule_for_tool("run_command").is_none());
    }

    #[test]
    fn add_allow_rule_then_match() {
        let mut ctx = ToolPermissionContext::default();
        ctx.add_rule(PermissionRule {
            source: PermissionRuleSource::Session,
            rule_behavior: PermissionBehavior::Allow,
            rule_value: PermissionRuleValue {
                tool_name: "write_file".into(),
                rule_content: None,
            },
        });
        let hit = ctx
            .find_allow_rule_for_tool("write_file")
            .expect("allow rule");
        assert_eq!(hit.rule_value.tool_name, "write_file");
        // Different tool name should not match
        assert!(ctx.find_allow_rule_for_tool("read_file").is_none());
    }

    #[test]
    fn deny_rule_takes_precedence_in_separate_buckets() {
        let mut ctx = ToolPermissionContext::default();
        ctx.add_rule(PermissionRule {
            source: PermissionRuleSource::Session,
            rule_behavior: PermissionBehavior::Allow,
            rule_value: PermissionRuleValue {
                tool_name: "x".into(),
                rule_content: None,
            },
        });
        ctx.add_rule(PermissionRule {
            source: PermissionRuleSource::Session,
            rule_behavior: PermissionBehavior::Deny,
            rule_value: PermissionRuleValue {
                tool_name: "x".into(),
                rule_content: None,
            },
        });
        // Both buckets contain x; main decision flow checks deny first
        assert!(ctx.find_allow_rule_for_tool("x").is_some());
        assert!(ctx.find_deny_rule_for_tool("x").is_some());
    }

    #[test]
    fn apply_addrules_update() {
        let mut ctx = ToolPermissionContext::default();
        ctx.apply_update(&PermissionUpdate::AddRules {
            rules: vec![PermissionRuleValue {
                tool_name: "edit_file".into(),
                rule_content: None,
            }],
            behavior: PermissionBehavior::Allow,
            destination: PermissionRuleSource::Session,
        });
        assert!(ctx.find_allow_rule_for_tool("edit_file").is_some());
    }

    #[test]
    fn list_all_rules_session_only() {
        let mut ctx = ToolPermissionContext::default();
        ctx.add_rule(PermissionRule {
            source: PermissionRuleSource::Session,
            rule_behavior: PermissionBehavior::Allow,
            rule_value: PermissionRuleValue {
                tool_name: "a".into(),
                rule_content: None,
            },
        });
        let rules = ctx.list_rules();
        assert_eq!(rules.len(), 1);
    }
}
