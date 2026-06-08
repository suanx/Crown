//! Permission settings loader and persister.
//!
//! Reads and writes permission rules from layered settings files, mirroring
//! Claude Code's `src/utils/permissions/permissionsLoader.ts`.
//!
//! ## Settings Sources (precedence: later overrides earlier):
//! 1. User: `<config_dir>/crown/settings.json`
//! 2. Project: `<cwd>/.crown/settings.json`
//! 3. Local: `<cwd>/.crown/settings.local.json`
//!
//! Rules from ALL sources are merged (not overridden). Mode is taken from
//! the highest-priority source that defines it.

use std::path::{Path, PathBuf};

use deepseek_tools::permission::{
    PermissionBehavior, PermissionMode, PermissionRule, PermissionRuleSource, PermissionUpdate,
};
use deepseek_tools::rule_parser::{format_rule_value, parse_rule_value};
use serde::{Deserialize, Serialize};
use tracing::warn;

/// Permissions section of settings.json.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionsSettings {
    /// Rules that auto-allow (e.g., `["run_command(git:*)", "write_file"]`)
    #[serde(default)]
    pub allow: Vec<String>,
    /// Rules that auto-deny
    #[serde(default)]
    pub deny: Vec<String>,
    /// Rules that force ask
    #[serde(default)]
    pub ask: Vec<String>,
    /// Default permission mode for new threads
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_mode: Option<String>,
    /// Additional working directories
    #[serde(default)]
    pub additional_directories: Vec<String>,
}

/// Top-level settings.json structure.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SettingsJson {
    /// Permission configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionsSettings>,
}

/// Get the settings file path for a given source.
pub fn settings_path_for_source(source: PermissionRuleSource, cwd: &Path) -> Option<PathBuf> {
    match source {
        PermissionRuleSource::UserSettings => {
            dirs::config_dir().map(|d| d.join("crown").join("settings.json"))
        }
        PermissionRuleSource::ProjectSettings => Some(cwd.join(".crown").join("settings.json")),
        PermissionRuleSource::LocalSettings => Some(cwd.join(".crown").join("settings.local.json")),
        _ => None, // Session, CliArg, etc. don't have files
    }
}

/// Load all permission rules from all settings files on disk.
///
/// Rules from all sources are merged (union). Each rule carries its source.
/// Returns the merged rules and the effective default mode (highest priority wins).
pub fn load_all_rules_from_disk(cwd: &Path) -> (Vec<PermissionRule>, Option<PermissionMode>) {
    let sources = [
        PermissionRuleSource::UserSettings,
        PermissionRuleSource::ProjectSettings,
        PermissionRuleSource::LocalSettings,
    ];

    let mut all_rules = Vec::new();
    let mut effective_mode: Option<PermissionMode> = None;

    for source in sources {
        let Some(path) = settings_path_for_source(source, cwd) else {
            continue;
        };
        if !path.exists() {
            continue;
        }

        match load_settings_file(&path) {
            Ok(settings) => {
                if let Some(perms) = &settings.permissions {
                    // Load rules
                    for rule_str in &perms.allow {
                        let rv = parse_rule_value(rule_str);
                        all_rules.push(PermissionRule {
                            source,
                            rule_behavior: PermissionBehavior::Allow,
                            rule_value: rv,
                        });
                    }
                    for rule_str in &perms.deny {
                        let rv = parse_rule_value(rule_str);
                        all_rules.push(PermissionRule {
                            source,
                            rule_behavior: PermissionBehavior::Deny,
                            rule_value: rv,
                        });
                    }
                    for rule_str in &perms.ask {
                        let rv = parse_rule_value(rule_str);
                        all_rules.push(PermissionRule {
                            source,
                            rule_behavior: PermissionBehavior::Ask,
                            rule_value: rv,
                        });
                    }
                    // Mode: later sources win
                    if let Some(mode_str) = &perms.default_mode {
                        effective_mode = Some(PermissionMode::from_str_lossy(mode_str));
                    }
                }
            }
            Err(e) => {
                warn!(?path, %e, "Failed to load settings file, skipping");
            }
        }
    }

    (all_rules, effective_mode)
}

/// Persist a permission update to the appropriate settings file.
///
/// Only persists updates with file-backed destinations (user/project/local).
/// Session and CliArg updates are in-memory only.
pub fn persist_permission_update(update: &PermissionUpdate, cwd: &Path) -> anyhow::Result<()> {
    match update {
        PermissionUpdate::SetMode { mode, destination } => {
            let path = match settings_path_for_source(*destination, cwd) {
                Some(p) => p,
                None => return Ok(()), // non-persistable
            };
            let mut settings = load_or_default(&path);
            let perms = settings.permissions.get_or_insert_with(Default::default);
            perms.default_mode = Some(mode.as_str().to_string());
            write_settings_file(&path, &settings)?;
            return Ok(());
        }
        PermissionUpdate::AddRules {
            rules,
            behavior,
            destination,
        } => {
            let path = match settings_path_for_source(*destination, cwd) {
                Some(p) => p,
                None => return Ok(()),
            };
            let mut settings = load_or_default(&path);
            let perms = settings.permissions.get_or_insert_with(Default::default);
            let target_vec = behavior_vec(perms, *behavior);

            for rv in rules {
                let formatted = format_rule_value(rv);
                if !target_vec.contains(&formatted) {
                    target_vec.push(formatted);
                }
            }

            write_settings_file(&path, &settings)?;
        }
        PermissionUpdate::RemoveRules {
            rules,
            behavior,
            destination,
        } => {
            let path = match settings_path_for_source(*destination, cwd) {
                Some(p) => p,
                None => return Ok(()),
            };
            let mut settings = load_or_default(&path);
            let perms = settings.permissions.get_or_insert_with(Default::default);
            let target_vec = behavior_vec(perms, *behavior);

            for rv in rules {
                let formatted = format_rule_value(rv);
                target_vec.retain(|s| s != &formatted);
            }

            write_settings_file(&path, &settings)?;
        }
        // AddDirectories, RemoveDirectories, ReplaceRules — TODO later
        _ => {}
    }

    Ok(())
}

/// Get the mutable rule list for a given behavior.
fn behavior_vec(perms: &mut PermissionsSettings, behavior: PermissionBehavior) -> &mut Vec<String> {
    match behavior {
        PermissionBehavior::Allow => &mut perms.allow,
        PermissionBehavior::Deny => &mut perms.deny,
        PermissionBehavior::Ask => &mut perms.ask,
    }
}

/// Load a settings file, returning Default on missing/malformed.
fn load_or_default(path: &Path) -> SettingsJson {
    if !path.exists() {
        return SettingsJson::default();
    }
    load_settings_file(path).unwrap_or_default()
}

/// Read and parse a settings JSON file.
fn load_settings_file(path: &Path) -> anyhow::Result<SettingsJson> {
    let content = std::fs::read_to_string(path)?;
    if content.trim().is_empty() {
        return Ok(SettingsJson::default());
    }
    let settings: SettingsJson = serde_json::from_str(&content)?;
    Ok(settings)
}

/// Write settings JSON to a file, creating parent directories as needed.
fn write_settings_file(path: &Path, settings: &SettingsJson) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(settings)?;
    std::fs::write(path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use deepseek_tools::permission::PermissionRuleValue;
    use tempfile::TempDir;

    fn setup_cwd() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn empty_filesystem_returns_no_rules() {
        let tmp = setup_cwd();
        let (rules, mode) = load_all_rules_from_disk(tmp.path());
        assert!(rules.is_empty());
        assert!(mode.is_none());
    }

    #[test]
    fn loads_rules_from_project_settings() {
        let tmp = setup_cwd();
        let settings_dir = tmp.path().join(".crown");
        std::fs::create_dir_all(&settings_dir).unwrap();
        std::fs::write(
            settings_dir.join("settings.json"),
            r#"{"permissions":{"allow":["run_command(git:*)","write_file"],"deny":["run_command(rm -rf /)"]}}"#,
        )
        .unwrap();

        let (rules, _) = load_all_rules_from_disk(tmp.path());
        assert_eq!(rules.len(), 3);

        let allow_rules: Vec<_> = rules
            .iter()
            .filter(|r| r.rule_behavior == PermissionBehavior::Allow)
            .collect();
        assert_eq!(allow_rules.len(), 2);
        assert_eq!(allow_rules[0].rule_value.tool_name, "run_command");
        assert_eq!(
            allow_rules[0].rule_value.rule_content,
            Some("git:*".to_string())
        );
        assert_eq!(allow_rules[0].source, PermissionRuleSource::ProjectSettings);
    }

    #[test]
    fn local_mode_overrides_project_mode() {
        let tmp = setup_cwd();
        let settings_dir = tmp.path().join(".crown");
        std::fs::create_dir_all(&settings_dir).unwrap();
        std::fs::write(
            settings_dir.join("settings.json"),
            r#"{"permissions":{"defaultMode":"plan"}}"#,
        )
        .unwrap();
        std::fs::write(
            settings_dir.join("settings.local.json"),
            r#"{"permissions":{"defaultMode":"bypassPermissions"}}"#,
        )
        .unwrap();

        let (_, mode) = load_all_rules_from_disk(tmp.path());
        assert_eq!(mode, Some(PermissionMode::BypassPermissions));
    }

    #[test]
    fn persist_add_rules_creates_file() {
        let tmp = setup_cwd();
        let update = PermissionUpdate::AddRules {
            rules: vec![PermissionRuleValue {
                tool_name: "run_command".into(),
                rule_content: Some("git:*".into()),
            }],
            behavior: PermissionBehavior::Allow,
            destination: PermissionRuleSource::LocalSettings,
        };
        persist_permission_update(&update, tmp.path()).unwrap();

        let path = tmp.path().join(".crown").join("settings.local.json");
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("run_command(git:*)"));
    }

    #[test]
    fn persist_remove_rules_removes_entry() {
        let tmp = setup_cwd();
        let settings_dir = tmp.path().join(".crown");
        std::fs::create_dir_all(&settings_dir).unwrap();
        std::fs::write(
            settings_dir.join("settings.local.json"),
            r#"{"permissions":{"allow":["run_command(git:*)","write_file"]}}"#,
        )
        .unwrap();

        let update = PermissionUpdate::RemoveRules {
            rules: vec![PermissionRuleValue {
                tool_name: "run_command".into(),
                rule_content: Some("git:*".into()),
            }],
            behavior: PermissionBehavior::Allow,
            destination: PermissionRuleSource::LocalSettings,
        };
        persist_permission_update(&update, tmp.path()).unwrap();

        let content = std::fs::read_to_string(settings_dir.join("settings.local.json")).unwrap();
        assert!(!content.contains("run_command(git:*)"));
        assert!(content.contains("write_file"));
    }

    #[test]
    fn persist_set_mode() {
        let tmp = setup_cwd();
        let update = PermissionUpdate::SetMode {
            mode: PermissionMode::Plan,
            destination: PermissionRuleSource::ProjectSettings,
        };
        persist_permission_update(&update, tmp.path()).unwrap();

        let path = tmp.path().join(".crown").join("settings.json");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("plan"));
    }

    #[test]
    fn malformed_file_returns_empty() {
        let tmp = setup_cwd();
        let settings_dir = tmp.path().join(".crown");
        std::fs::create_dir_all(&settings_dir).unwrap();
        std::fs::write(settings_dir.join("settings.json"), "not valid json {{{").unwrap();

        let (rules, mode) = load_all_rules_from_disk(tmp.path());
        assert!(rules.is_empty());
        assert!(mode.is_none());
    }

    #[test]
    fn empty_file_returns_empty() {
        let tmp = setup_cwd();
        let settings_dir = tmp.path().join(".crown");
        std::fs::create_dir_all(&settings_dir).unwrap();
        std::fs::write(settings_dir.join("settings.json"), "").unwrap();

        let (rules, mode) = load_all_rules_from_disk(tmp.path());
        assert!(rules.is_empty());
        assert!(mode.is_none());
    }

    #[test]
    fn persist_does_not_duplicate_rules() {
        let tmp = setup_cwd();
        let update = PermissionUpdate::AddRules {
            rules: vec![PermissionRuleValue {
                tool_name: "write_file".into(),
                rule_content: None,
            }],
            behavior: PermissionBehavior::Allow,
            destination: PermissionRuleSource::LocalSettings,
        };
        // Add same rule twice
        persist_permission_update(&update, tmp.path()).unwrap();
        persist_permission_update(&update, tmp.path()).unwrap();

        let path = tmp.path().join(".crown").join("settings.local.json");
        let content = std::fs::read_to_string(&path).unwrap();
        // Should only appear once
        assert_eq!(content.matches("write_file").count(), 1);
    }

    #[test]
    fn rules_from_multiple_sources_are_merged() {
        let tmp = setup_cwd();
        let settings_dir = tmp.path().join(".crown");
        std::fs::create_dir_all(&settings_dir).unwrap();
        std::fs::write(
            settings_dir.join("settings.json"),
            r#"{"permissions":{"allow":["run_command(git:*)"]}}"#,
        )
        .unwrap();
        std::fs::write(
            settings_dir.join("settings.local.json"),
            r#"{"permissions":{"allow":["write_file"]}}"#,
        )
        .unwrap();

        let (rules, _) = load_all_rules_from_disk(tmp.path());
        assert_eq!(rules.len(), 2);
        let tool_names: Vec<&str> = rules
            .iter()
            .map(|r| r.rule_value.tool_name.as_str())
            .collect();
        assert!(tool_names.contains(&"run_command"));
        assert!(tool_names.contains(&"write_file"));
    }

    #[test]
    fn settings_path_for_non_persistable_source_returns_none() {
        let tmp = setup_cwd();
        assert!(settings_path_for_source(PermissionRuleSource::Session, tmp.path()).is_none());
        assert!(settings_path_for_source(PermissionRuleSource::CliArg, tmp.path()).is_none());
    }
}
