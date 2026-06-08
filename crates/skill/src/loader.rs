//! Load a skill's full body on demand (progressive disclosure level 2).

use anyhow::Result;

use crate::discovery::SkillMeta;
use crate::frontmatter::parse_skill_md;

/// Read the skill's SKILL.md and return its body with `$ARGUMENTS`
/// substituted. The body is what gets injected into the conversation when the
/// model invokes the skill.
pub fn load_skill_body(meta: &SkillMeta, args: Option<&str>) -> Result<String> {
    let raw = std::fs::read_to_string(&meta.path)?;
    let (_fm, body) = parse_skill_md(&raw)?;
    Ok(body.replace("$ARGUMENTS", args.unwrap_or("")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::{Scope, Source};

    #[test]
    fn loads_body_and_substitutes_args() {
        let tmp = tempfile::tempdir().unwrap();
        let sk = tmp.path().join("greet");
        std::fs::create_dir_all(&sk).unwrap();
        std::fs::write(
            sk.join("SKILL.md"),
            "---\nname: greet\ndescription: d\n---\nHello $ARGUMENTS!",
        )
        .unwrap();
        let meta = SkillMeta {
            name: "greet".into(),
            description: "d".into(),
            scope: Scope::Global,
            source: Source::Native,
            path: sk.join("SKILL.md"),
            allowed_tools: vec![],
        };
        let body = load_skill_body(&meta, Some("World")).unwrap();
        assert!(body.contains("Hello World!"), "got: {body}");
    }

    #[test]
    fn empty_args_substitutes_blank() {
        let tmp = tempfile::tempdir().unwrap();
        let sk = tmp.path().join("greet");
        std::fs::create_dir_all(&sk).unwrap();
        std::fs::write(
            sk.join("SKILL.md"),
            "---\nname: greet\ndescription: d\n---\nHi$ARGUMENTS.",
        )
        .unwrap();
        let meta = SkillMeta {
            name: "greet".into(),
            description: "d".into(),
            scope: Scope::Global,
            source: Source::Native,
            path: sk.join("SKILL.md"),
            allowed_tools: vec![],
        };
        let body = load_skill_body(&meta, None).unwrap();
        assert!(body.contains("Hi."), "got: {body}");
    }
}
