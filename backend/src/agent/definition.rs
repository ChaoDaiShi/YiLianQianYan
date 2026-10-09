// ============================================================
// AGENT.md definition parsing
// ============================================================
//
// Subagent definitions are discovered from `AGENT.md` files. Parsing lives in
// the agent domain rather than the application composition root.

use std::collections::HashSet;

/// A strictly-parsed AGENT.md definition.
pub(crate) struct ParsedAgentDefinition {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) tools: Vec<String>,
    pub(crate) model: Option<String>,
    pub(crate) workdir: Option<String>,
    pub(crate) instructions: String,
}

/// Parse AGENT.md frontmatter strictly.
///
/// The first `---` line opens the frontmatter, the next `---` closes it.
/// Everything after the closing delimiter is the instructions body. Fields are
/// only read inside the frontmatter; `name:` / `tools:` in the body are ignored.
pub(crate) fn parse_agent_definition(
    content: &str,
    dir_name: &str,
) -> Option<ParsedAgentDefinition> {
    let mut lines = content.lines();
    // Opening delimiter must be the very first line.
    if lines.next()?.trim() != "---" {
        return None;
    }

    let mut fields: Vec<(String, String)> = Vec::new();
    let mut saw_closing = false;
    for line in lines.by_ref() {
        let t = line.trim();
        if t == "---" {
            saw_closing = true;
            break;
        }
        if let Some((key, value)) = t.split_once(':') {
            let key = key.trim();
            if !key.is_empty() && !key.contains(' ') {
                fields.push((key.to_string(), value.trim().to_string()));
            }
        }
    }
    if !saw_closing {
        return None;
    }

    let instructions = lines.collect::<Vec<_>>().join("\n").trim().to_string();
    if instructions.is_empty() {
        return None;
    }

    // The trusted identity is the directory name. An explicit frontmatter name
    // must match it; otherwise the definition is rejected.
    let fm_name = find_field(&fields, "name");
    let name = fm_name.as_deref().unwrap_or(dir_name).trim();
    if name.is_empty() || name != dir_name {
        return None;
    }

    Some(ParsedAgentDefinition {
        name: name.to_string(),
        description: find_field(&fields, "description").unwrap_or_default(),
        tools: find_field(&fields, "tools")
            .map(|raw| parse_allowed_tools(&raw))
            .unwrap_or_default(),
        model: find_field(&fields, "model").and_then(trimmed_optional),
        workdir: find_field(&fields, "workdir").and_then(trimmed_optional),
        instructions,
    })
}

fn find_field(fields: &[(String, String)], key: &str) -> Option<String> {
    fields
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
}

fn trimmed_optional(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Parse a `tools:` value that may be `[a, b, c]`, `a, b, c`, or a mix with
/// quoted items. Trims, strips surrounding quotes, drops empty / invalid
/// entries, and de-duplicates preserving first-seen order.
fn parse_allowed_tools(raw: &str) -> Vec<String> {
    let inner = raw.trim().trim_start_matches('[').trim_end_matches(']');
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in inner.split(',') {
        let cleaned = item.trim().trim_matches('"').trim_matches('\'').trim();
        if cleaned.is_empty() || !is_valid_tool_name(cleaned) {
            continue;
        }
        if seen.insert(cleaned.to_string()) {
            out.push(cleaned.to_string());
        }
    }
    out
}

fn is_valid_tool_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(content: &str, dir_name: &str) -> Option<ParsedAgentDefinition> {
        parse_agent_definition(content, dir_name)
    }

    #[test]
    fn parses_inline_bracket_tools() {
        let d = def(
            "---\nname: researcher\ntools: [read_file, grep, glob]\n---\nbody",
            "researcher",
        )
        .unwrap();
        assert_eq!(d.tools, vec!["read_file", "grep", "glob"]);
    }

    #[test]
    fn parses_comma_separated_tools() {
        let d = def("---\ntools: read_file, grep\n---\nbody", "researcher").unwrap();
        assert_eq!(d.tools, vec!["read_file", "grep"]);
    }

    #[test]
    fn parses_quoted_tool_items() {
        let d = def(
            "---\ntools: [read_file, \"grep\", 'glob']\n---\nbody",
            "researcher",
        )
        .unwrap();
        assert_eq!(d.tools, vec!["read_file", "grep", "glob"]);
    }

    #[test]
    fn deduplicates_tool_items() {
        let d = def("---\ntools: [read_file, read_file, grep]\n---\nbody", "x").unwrap();
        assert_eq!(d.tools, vec!["read_file", "grep"]);
    }

    #[test]
    fn skips_invalid_tool_entries() {
        let d = def("---\ntools: [read_file, ???, grep]\n---\nbody", "x").unwrap();
        assert_eq!(d.tools, vec!["read_file", "grep"]);
    }

    #[test]
    fn frontmatter_name_mismatch_rejects_subagent() {
        let content = "---\nname: destructive-agent\ndescription: x\n---\nbody";
        assert!(def(content, "researcher").is_none());
    }

    #[test]
    fn missing_frontmatter_name_uses_directory_name() {
        let content = "---\ndescription: x\ntools: [read_file]\n---\nbody";
        let d = def(content, "researcher").unwrap();
        assert_eq!(d.name, "researcher");
    }

    #[test]
    fn missing_closing_delimiter_rejects() {
        let content = "---\nname: researcher\ndescription: x\nbody without close";
        assert!(def(content, "researcher").is_none());
    }

    #[test]
    fn empty_instructions_rejects() {
        let content = "---\nname: researcher\n---\n\n   \n";
        assert!(def(content, "researcher").is_none());
    }

    #[test]
    fn instructions_equal_body_and_exclude_frontmatter() {
        let content = "---\nname: researcher\ndescription: x\n---\n\n# Title\n\nactual body";
        let d = def(content, "researcher").unwrap();
        assert!(d.instructions.contains("# Title"));
        assert!(d.instructions.contains("actual body"));
        assert!(!d.instructions.contains("name: researcher"));
        assert!(!d.instructions.contains("description: x"));
        assert!(!d.instructions.contains("---"));
    }

    #[test]
    fn instructions_body_metadata_is_not_parsed_as_fields() {
        // A `name:` / `tools:` in the body must not leak into metadata.
        let content = "---\nname: researcher\n---\n\nname: impostor\ntools: [evil_tool]";
        let d = def(content, "researcher").unwrap();
        assert_eq!(d.name, "researcher");
        assert!(d.tools.is_empty());
        assert!(d.instructions.contains("name: impostor"));
    }
}
