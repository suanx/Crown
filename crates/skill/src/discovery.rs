//! Skill discovery across the four source directories, with an mtime cache.

use std::path::{Path, PathBuf};

use crate::frontmatter::parse_skill_md;

/// Where a skill came from (precedence: Project > Global).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Project,
}

/// Origin format (precedence on name tie: Native > Claude; Mcp is separate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Native,
    Claude,
    Mcp,
}

/// Lightweight metadata for progressive disclosure (name + description loaded
/// up front; body loaded on demand by the loader).
#[derive(Debug, Clone)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    pub scope: Scope,
    pub source: Source,
    pub path: PathBuf,
    pub allowed_tools: Vec<String>,
}

/// Scan a single directory for `<name>/SKILL.md` skills.
pub fn scan_dir(dir: &Path, scope: Scope, source: Source) -> Vec<SkillMeta> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let sub = entry.path();
        if !sub.is_dir() {
            continue;
        }
        let skill_md = sub.join("SKILL.md");
        if !skill_md.is_file() {
            continue;
        }
        let raw = match std::fs::read_to_string(&skill_md) {
            Ok(s) => s,
            Err(_) => continue,
        };
        match parse_skill_md(&raw) {
            Ok((fm, _body)) => out.push(SkillMeta {
                name: fm.name,
                description: fm.description,
                scope,
                source,
                path: skill_md,
                allowed_tools: fm.allowed_tools,
            }),
            Err(e) => {
                tracing::warn!(path = %skill_md.display(), error = %e, "skipping invalid SKILL.md");
            }
        }
    }
    out
}

/// The directories scanned, in priority order (low → high).
pub fn skill_dirs(cwd: Option<&Path>) -> Vec<(PathBuf, Scope, Source)> {
    let mut dirs = Vec::new();
    let data = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

    // Global (lowest priority first).
    dirs.push((
        home.join(".claude").join("skills"),
        Scope::Global,
        Source::Claude,
    ));
    dirs.push((
        data.join("crown").join("skills"),
        Scope::Global,
        Source::Native,
    ));
    // Project (higher priority).
    if let Some(cwd) = cwd {
        dirs.push((
            cwd.join(".claude").join("skills"),
            Scope::Project,
            Source::Claude,
        ));
        dirs.push((
            cwd.join(".crown").join("skills"),
            Scope::Project,
            Source::Native,
        ));
    }
    dirs
}

/// Discover all skills, de-duplicated by name. Later entries (project, native)
/// override earlier ones (global, claude) on name collision.
pub fn discover_all(cwd: Option<&Path>) -> Vec<SkillMeta> {
    use std::collections::BTreeMap;
    let mut by_name: BTreeMap<String, SkillMeta> = BTreeMap::new();
    for (dir, scope, source) in skill_dirs(cwd) {
        for meta in scan_dir(&dir, scope, source) {
            by_name.insert(meta.name.clone(), meta); // later wins
        }
    }
    by_name.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn global_native_skill_dir_is_under_crown() {
        let dirs = skill_dirs(None);
        let has_crown = dirs.iter().any(|(p, scope, source)| {
            *scope == Scope::Global
                && *source == Source::Native
                && p.components().any(|c| c.as_os_str() == "crown")
        });
        assert!(
            has_crown,
            "expected a global native skills dir under crown: {dirs:?}"
        );
    }

    #[test]
    fn discovers_skill_in_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let sk = tmp.path().join("my-skill");
        fs::create_dir_all(&sk).unwrap();
        fs::write(
            sk.join("SKILL.md"),
            "---\nname: my-skill\ndescription: does things. use when X.\n---\nbody",
        )
        .unwrap();
        let metas = scan_dir(tmp.path(), Scope::Global, Source::Native);
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].name, "my-skill");
        assert_eq!(metas[0].description, "does things. use when X.");
    }

    #[test]
    fn skips_dir_without_skill_md() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("empty")).unwrap();
        assert_eq!(scan_dir(tmp.path(), Scope::Global, Source::Native).len(), 0);
    }

    #[test]
    fn skips_invalid_skill_md() {
        let tmp = tempfile::tempdir().unwrap();
        let sk = tmp.path().join("bad");
        fs::create_dir_all(&sk).unwrap();
        fs::write(sk.join("SKILL.md"), "no frontmatter at all").unwrap();
        assert_eq!(scan_dir(tmp.path(), Scope::Global, Source::Native).len(), 0);
    }
}
