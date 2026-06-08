//! SKILL.md YAML frontmatter parsing + validation per the official Agent
//! Skills spec (agentskills.io, 2025-12).

use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::collections::BTreeMap;

/// Parsed, validated frontmatter.
#[derive(Debug, Clone)]
pub struct Frontmatter {
    pub name: String,
    pub description: String,
    pub license: Option<String>,
    pub compatibility: Option<String>,
    pub allowed_tools: Vec<String>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct RawFm {
    name: String,
    description: String,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    compatibility: Option<String>,
    #[serde(default, rename = "allowed-tools")]
    allowed_tools: Option<String>,
    #[serde(default)]
    metadata: Option<BTreeMap<String, String>>,
}

/// Split a SKILL.md into `(frontmatter, body)` and validate per spec.
pub fn parse_skill_md(s: &str) -> Result<(Frontmatter, String)> {
    let (yaml, body) = split_frontmatter(s)?;
    let raw: RawFm =
        serde_yaml::from_str(&yaml).map_err(|e| anyhow!("invalid frontmatter YAML: {e}"))?;

    validate_name(&raw.name)?;
    validate_description(&raw.description)?;
    if let Some(c) = &raw.compatibility {
        if c.len() > 500 {
            return Err(anyhow!("compatibility must be <= 500 chars"));
        }
    }

    let allowed_tools = raw
        .allowed_tools
        .map(|s| s.split_whitespace().map(String::from).collect())
        .unwrap_or_default();

    Ok((
        Frontmatter {
            name: raw.name,
            description: raw.description,
            license: raw.license,
            compatibility: raw.compatibility,
            allowed_tools,
            metadata: raw.metadata.unwrap_or_default(),
        },
        body,
    ))
}

/// Validate the `name` field: 1-64 chars, `[a-z0-9-]`, no leading/trailing
/// hyphen, no consecutive hyphens.
pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 64 {
        return Err(anyhow!("skill name must be 1-64 characters"));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(anyhow!("skill name may only contain a-z, 0-9, and hyphens"));
    }
    if name.starts_with('-') || name.ends_with('-') {
        return Err(anyhow!("skill name must not start or end with a hyphen"));
    }
    if name.contains("--") {
        return Err(anyhow!("skill name must not contain consecutive hyphens"));
    }
    Ok(())
}

fn validate_description(desc: &str) -> Result<()> {
    if desc.trim().is_empty() {
        return Err(anyhow!("description must be non-empty"));
    }
    if desc.len() > 1024 {
        return Err(anyhow!("description must be <= 1024 chars"));
    }
    Ok(())
}

/// Split leading `---\n...\n---\n` frontmatter from the body.
fn split_frontmatter(s: &str) -> Result<(String, String)> {
    let s = s.strip_prefix('\u{feff}').unwrap_or(s); // tolerate BOM
    let trimmed = s.trim_start_matches(['\r', '\n']);
    let rest = trimmed
        .strip_prefix("---")
        .ok_or_else(|| anyhow!("SKILL.md must start with YAML frontmatter (---)"))?;
    // rest begins right after the opening ---; find the closing --- on its own line.
    let rest = rest.trim_start_matches(['\r']).trim_start_matches('\n');
    let end = find_closing_fence(rest)
        .ok_or_else(|| anyhow!("SKILL.md frontmatter is not closed with ---"))?;
    let yaml = rest[..end].to_string();
    let after = &rest[end..];
    // skip the closing --- line
    let body = after
        .trim_start_matches("---")
        .trim_start_matches(['\r'])
        .trim_start_matches('\n')
        .to_string();
    Ok((yaml, body))
}

/// Find the byte offset of a line that is exactly `---`.
fn find_closing_fence(s: &str) -> Option<usize> {
    let mut offset = 0;
    for line in s.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "---" {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal() {
        let md = "---\nname: pdf-processing\ndescription: Extract PDF text. Use for PDFs.\n---\n\nBody here.";
        let (fm, body) = parse_skill_md(md).unwrap();
        assert_eq!(fm.name, "pdf-processing");
        assert!(fm.description.starts_with("Extract"));
        assert!(body.contains("Body here"));
    }

    #[test]
    fn rejects_uppercase_name() {
        let md = "---\nname: PDF\ndescription: x\n---\n";
        assert!(parse_skill_md(md).is_err());
    }

    #[test]
    fn rejects_name_over_64() {
        let long = "a".repeat(65);
        let md = format!("---\nname: {long}\ndescription: x\n---\n");
        assert!(parse_skill_md(&md).is_err());
    }

    #[test]
    fn rejects_consecutive_hyphens() {
        let md = "---\nname: pdf--proc\ndescription: x\n---\n";
        assert!(parse_skill_md(md).is_err());
    }

    #[test]
    fn rejects_empty_description() {
        let md = "---\nname: ok\ndescription: \"\"\n---\n";
        assert!(parse_skill_md(md).is_err());
    }

    #[test]
    fn parses_optional_fields() {
        let md = "---\nname: pdf\ndescription: x\nlicense: MIT\nallowed-tools: \"Read Bash(git:*)\"\nmetadata:\n  author: me\n---\n";
        let (fm, _) = parse_skill_md(md).unwrap();
        assert_eq!(fm.license.as_deref(), Some("MIT"));
        assert_eq!(fm.allowed_tools, vec!["Read", "Bash(git:*)"]);
        assert_eq!(fm.metadata.get("author").map(String::as_str), Some("me"));
    }

    #[test]
    fn rejects_missing_frontmatter() {
        assert!(parse_skill_md("just body, no frontmatter").is_err());
    }
}
