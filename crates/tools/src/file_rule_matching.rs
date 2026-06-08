//! File path glob permission matching.
//!
//! Mirrors the glob-based file rule matching from Claude Code's
//! `src/utils/permissions/filesystem.ts:matchingRuleForInput`.
//!
//! Permission rules for filesystem tools can contain glob patterns:
//! - `/src/**/*.rs` — relative to settings source root (project root)
//! - `//home/user/project/**` — absolute path (double-slash prefix = root)
//! - `~/.config/**` — relative to user home directory
//! - `*.toml` — matches anywhere (no root prefix)
//!
//! The `globset` crate is used for matching since it supports full
//! gitignore-style globs.

/// Match a file path against a permission rule's glob content.
///
/// # Arguments
/// - `rule_content` — the glob pattern from the rule (e.g., `/src/**/*.rs`)
/// - `file_path` — the actual file path being accessed (absolute or relative)
/// - `source_root` — the project root where settings are located (for `/`-prefixed patterns)
///
/// # Pattern Prefix Semantics:
/// - `//path` → absolute (root = filesystem root, or drive root on Windows)
/// - `~/path` → relative to user's home directory
/// - `/path` → relative to `source_root` (project root)
/// - `path` (no prefix) → matches anywhere in the path (basename or relative)
///
/// Returns `true` if the file path matches the glob pattern.
pub fn matches_file_rule(rule_content: &str, file_path: &str, source_root: Option<&str>) -> bool {
    // Normalize path separators
    let normalized_path = file_path.replace('\\', "/");
    let normalized_rule = rule_content.replace('\\', "/");

    // Determine the pattern root and effective pattern
    let (root, effective_pattern) = classify_pattern(&normalized_rule, source_root);

    // Resolve the file path relative to the root for matching
    let match_path = match root {
        PatternRoot::Absolute => {
            // Pattern is absolute — match against the full absolute path
            normalized_path.clone()
        }
        PatternRoot::Home => {
            // Pattern is relative to home — match against path relative to home
            let home = dirs::home_dir()
                .map(|h| h.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if let Some(rel) = strip_prefix_normalized(&normalized_path, &home) {
                rel
            } else {
                // Path is not under home dir — can't match
                return false;
            }
        }
        PatternRoot::ProjectRoot(ref root_path) => {
            // Pattern is relative to project root
            if let Some(rel) = strip_prefix_normalized(&normalized_path, root_path) {
                rel
            } else {
                // Path not under project root — try as-is (might be relative already)
                normalized_path.clone()
            }
        }
        PatternRoot::Anywhere => {
            // No root — match against the path as-is (usually relative)
            normalized_path.clone()
        }
    };

    // Build glob and match
    glob_matches(&effective_pattern, &match_path)
}

/// Classification of pattern root.
#[derive(Debug)]
enum PatternRoot {
    /// `//` prefix — absolute path from filesystem root
    Absolute,
    /// `~/` prefix — relative to home directory
    Home,
    /// `/` prefix — relative to project/settings root
    ProjectRoot(String),
    /// No prefix — matches anywhere
    Anywhere,
}

/// Classify a pattern into its root type and effective glob string.
fn classify_pattern(pattern: &str, source_root: Option<&str>) -> (PatternRoot, String) {
    if pattern.starts_with("//") {
        // Absolute path — strip the leading // to get the actual path pattern
        let effective = pattern[1..].to_string(); // keep one leading /
        (PatternRoot::Absolute, effective)
    } else if pattern.starts_with("~/") {
        // Home-relative
        let effective = pattern[1..].to_string(); // keep leading /
        (PatternRoot::Home, effective)
    } else if pattern.starts_with('/') {
        // Project-root-relative
        let root = source_root.unwrap_or(".").replace('\\', "/");
        let effective = pattern.to_string();
        (PatternRoot::ProjectRoot(root), effective)
    } else {
        // Anywhere (no root)
        // Normalize: strip leading "./" if present
        let effective = if let Some(stripped) = pattern.strip_prefix("./") {
            stripped.to_string()
        } else {
            pattern.to_string()
        };
        (PatternRoot::Anywhere, effective)
    }
}

/// Strip a prefix from a path, returning the remainder with a leading `/`.
fn strip_prefix_normalized(path: &str, prefix: &str) -> Option<String> {
    let clean_prefix = prefix.trim_end_matches('/');
    if path == clean_prefix {
        return Some("/".to_string());
    }
    if path.starts_with(&format!("{}/", clean_prefix)) {
        let remainder = &path[clean_prefix.len()..];
        Some(remainder.to_string())
    } else {
        None
    }
}

/// Use globset to check if the path matches the pattern.
fn glob_matches(pattern: &str, path: &str) -> bool {
    // Remove trailing /** — globset treats the base path as matching
    // both itself and children
    let (clean_pattern, has_doublestar) = if let Some(stripped) = pattern.strip_suffix("/**") {
        (stripped, true)
    } else {
        (pattern, false)
    };

    // Strip leading / from both pattern and path for consistent matching
    let pat = clean_pattern.trim_start_matches('/');
    let p = path.trim_start_matches('/');

    // Direct path match (for /** patterns — dir itself matches)
    if has_doublestar && p == pat {
        return true;
    }

    // Check if path starts with the base dir (for /** patterns)
    if has_doublestar && p.starts_with(&format!("{}/", pat)) {
        return true;
    }

    // Full glob match
    let full_pattern = pattern.trim_start_matches('/');
    match globset::Glob::new(full_pattern) {
        Ok(glob) => {
            let matcher = glob.compile_matcher();
            matcher.is_match(p)
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_relative_glob_matches() {
        // /src/**/*.rs relative to project root
        assert!(matches_file_rule("/src/**/*.rs", "src/main.rs", Some(".")));
        assert!(matches_file_rule(
            "/src/**/*.rs",
            "src/lib/foo.rs",
            Some(".")
        ));
    }

    #[test]
    fn project_relative_glob_does_not_match_outside() {
        assert!(!matches_file_rule(
            "/src/**/*.rs",
            "tests/foo.rs",
            Some(".")
        ));
        assert!(!matches_file_rule("/src/**/*.rs", "src/main.py", Some(".")));
    }

    #[test]
    fn anywhere_glob_matches_basename() {
        assert!(matches_file_rule("*.toml", "Cargo.toml", None));
        assert!(matches_file_rule("*.toml", "crates/tools/Cargo.toml", None));
    }

    #[test]
    fn anywhere_glob_subdir_pattern() {
        assert!(matches_file_rule("**/*.rs", "src/main.rs", None));
        assert!(matches_file_rule(
            "**/*.rs",
            "crates/tools/src/lib.rs",
            None
        ));
    }

    #[test]
    fn dir_pattern_with_doublestar_matches_children() {
        assert!(matches_file_rule("/src/**", "src/main.rs", Some(".")));
        assert!(matches_file_rule(
            "/src/**",
            "src/nested/deep/file.rs",
            Some(".")
        ));
    }

    #[test]
    fn dir_pattern_matches_dir_itself() {
        assert!(matches_file_rule("/src/**", "src", Some(".")));
    }

    #[test]
    fn dotfile_pattern() {
        assert!(matches_file_rule(".env", ".env", None));
        assert!(matches_file_rule("*.env", "production.env", None));
    }

    #[test]
    fn no_match_different_extension() {
        assert!(!matches_file_rule("/src/**/*.rs", "src/main.ts", Some(".")));
    }
}
