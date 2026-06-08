//! Progressive skill disclosure — format the available-skills listing that's
//! injected as a system-reminder so the model knows what skills exist without
//! loading their full bodies. Mirrors Claude Code's `formatCommandsWithinBudget`.

use deepseek_skill::discovery::SkillMeta;

/// A skill entry for the listing (name + one-line description). Decouples the
/// formatter from `SkillMeta` so MCP prompts can be folded in as pseudo-skills.
#[derive(Debug, Clone)]
pub struct SkillListEntry {
    pub name: String,
    pub description: String,
}

impl From<&SkillMeta> for SkillListEntry {
    fn from(m: &SkillMeta) -> Self {
        SkillListEntry {
            name: m.name.clone(),
            description: m.description.clone(),
        }
    }
}

/// Per-entry hard cap on description length (matches Claude's MAX_LISTING_DESC_CHARS).
const MAX_DESC_CHARS: usize = 250;

/// Build the system-reminder listing of available skills, fitting within
/// `char_budget`. When the full listing exceeds the budget, descriptions are
/// truncated (newest behavior: trim per-entry, then names-only as last resort).
/// Returns an empty string when there are no skills.
pub fn format_skill_listing(entries: &[SkillListEntry], char_budget: usize) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let header = "The following skills are available. When a user request matches a skill, \
                  call the `skill` tool with its name BEFORE doing the work yourself:\n";

    // Try full (capped) descriptions first.
    let full: Vec<String> = entries
        .iter()
        .map(|e| format!("- {}: {}", e.name, clip(&e.description, MAX_DESC_CHARS)))
        .collect();
    let full_len: usize = full.iter().map(|l| l.len() + 1).sum::<usize>() + header.len();

    if full_len <= char_budget {
        return format!("{header}{}", full.join("\n"));
    }

    // Over budget: compute a per-entry description length that fits.
    let names_overhead: usize = entries
        .iter()
        .map(|e| e.name.len() + 4) // "- " + ": " + newline
        .sum::<usize>()
        + header.len();
    let avail = char_budget.saturating_sub(names_overhead);
    let per_desc = if entries.is_empty() {
        0
    } else {
        avail / entries.len()
    };

    if per_desc < 16 {
        // Extreme: names only.
        let names: Vec<String> = entries.iter().map(|e| format!("- {}", e.name)).collect();
        return format!("{header}{}", names.join("\n"));
    }

    let trimmed: Vec<String> = entries
        .iter()
        .map(|e| format!("- {}: {}", e.name, clip(&e.description, per_desc)))
        .collect();
    format!("{header}{}", trimmed.join("\n"))
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, desc: &str) -> SkillListEntry {
        SkillListEntry {
            name: name.into(),
            description: desc.into(),
        }
    }

    #[test]
    fn empty_yields_empty() {
        assert_eq!(format_skill_listing(&[], 8000), "");
    }

    #[test]
    fn listing_includes_name_and_desc() {
        let entries = vec![entry("a", "does a"), entry("b", "does b")];
        let s = format_skill_listing(&entries, 8000);
        assert!(s.contains("- a: does a"), "got: {s}");
        assert!(s.contains("- b: does b"), "got: {s}");
        assert!(s.contains("skill` tool"), "has guidance header");
    }

    #[test]
    fn listing_truncates_over_budget() {
        let entries: Vec<_> = (0..100)
            .map(|i| entry(&format!("s{i}"), &"x".repeat(300)))
            .collect();
        let s = format_skill_listing(&entries, 1500);
        // Must contain all names (discovery is the point) but stay near budget.
        assert!(s.contains("s0"));
        assert!(s.contains("s99"));
        assert!(s.len() <= 2200, "len {} should be near budget", s.len());
    }
}
