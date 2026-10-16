pub fn parse_markdown_headings(content: &str) -> Vec<String> {
    let mut headings = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            let heading_text = trimmed.trim_start_matches('#').trim();
            if !heading_text.is_empty() {
                headings.push(heading_text.to_string());
            }
        }
    }
    headings
}

pub(super) fn normalize_heading(s: &str) -> String {
    s.trim().to_lowercase().replace(' ', "")
}
