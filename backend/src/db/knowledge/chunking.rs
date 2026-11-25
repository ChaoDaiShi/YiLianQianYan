//! Choose boundaries in canonical text without rewriting or duplicating it.
use std::ops::Range;

pub(super) const CHUNK_CHARS: usize = 1200;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Layout {
    Text,
    Markdown,
    Slides,
    Workbook,
    DelimitedTable,
}

impl Layout {
    pub(super) fn from_mime(mime: &str) -> Self {
        match mime {
            "text/markdown" => Self::Markdown,
            "application/vnd.openxmlformats-officedocument.presentationml.presentation" => {
                Self::Slides
            }
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => Self::Workbook,
            "text/csv" | "text/tab-separated-values" => Self::DelimitedTable,
            _ => Self::Text,
        }
    }

    fn tabular(self) -> bool {
        matches!(self, Self::Workbook | Self::DelimitedTable)
    }

    fn has_sections(self) -> bool {
        matches!(self, Self::Markdown | Self::Slides | Self::Workbook)
    }

    fn heading(self, line: &str) -> Option<&str> {
        match self {
            Self::Markdown => {
                let title = line.trim_start_matches('#');
                ((1..=6).contains(&(line.len() - title.len()))
                    && title.starts_with(char::is_whitespace))
                .then(|| title.trim())
            }
            Self::Slides => {
                let page = line.strip_prefix("第 ")?.strip_suffix(" 页")?;
                (!page.is_empty() && page.bytes().all(|byte| byte.is_ascii_digit())).then_some(line)
            }
            Self::Workbook => line.strip_prefix('[')?.strip_suffix(']'),
            _ => None,
        }
        .filter(|heading| !heading.is_empty())
    }
}

fn headings(text: &str, layout: Layout) -> impl Iterator<Item = (Range<usize>, &str)> {
    let mut start = 0;
    let mut fence = None;
    text.split_inclusive('\n').filter_map(move |line| {
        let line_range = start..start + line.len();
        start = line_range.end;
        let line = line.trim();
        if layout == Layout::Markdown {
            if let Some(marker @ (b'`' | b'~')) = line.as_bytes().first().copied() {
                let run = line.bytes().take_while(|&byte| byte == marker).count();
                match fence {
                    Some((opened, width))
                        if marker == opened && run >= width && line[run..].trim().is_empty() =>
                    {
                        fence = None;
                        return None;
                    }
                    None if run >= 3 => {
                        fence = Some((marker, run));
                        return None;
                    }
                    _ => {}
                }
            }
            if fence.is_some() {
                return None;
            }
        }
        layout.heading(line).map(|heading| (line_range, heading))
    })
}

fn byte_at(text: &str, character: usize) -> usize {
    text.char_indices()
        .nth(character)
        .map_or(text.len(), |(byte, _)| byte)
}

fn structural_break(text: &str, tabular: bool) -> Option<usize> {
    let paragraph = (!tabular)
        .then(|| {
            text.rfind("\n\n")
                .map(|start| start + 2)
                .max(text.rfind("\r\n\r\n").map(|start| start + 4))
        })
        .flatten();
    paragraph
        .or_else(|| text.rfind('\n').map(|start| start + 1))
        .or_else(|| {
            let punctuation = text
                .char_indices()
                .rev()
                .find(|&(_, character)| {
                    matches!(character, '。' | '！' | '？' | '；' | '!' | '?' | ';')
                })
                .map(|(start, character)| start + character.len_utf8());
            punctuation.max(text.rfind(". ").map(|start| start + 1))
        })
}

fn bounded_end(text: &str, max_chars: usize, minimum: usize, tabular: bool) -> usize {
    let mut limit = byte_at(text, max_chars);
    if limit == text.len() {
        return limit;
    }
    if minimum < max_chars && text.as_bytes()[limit - 1] == b'\r' && text.as_bytes()[limit] == b'\n'
    {
        limit -= 1;
    }
    let window = &text[..limit];
    let offset = byte_at(window, minimum.saturating_sub(1));
    let possible = &window[offset..];
    structural_break(possible, tabular)
        .or_else(|| {
            possible
                .char_indices()
                .rev()
                .find(|&(_, character)| character.is_whitespace())
                .map(|(start, character)| start + character.len_utf8())
        })
        .map_or(limit, |end| offset + end)
}

pub(super) fn split(text: &str, layout: Layout) -> Vec<String> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let minimum = if layout.tabular() { 1 } else { CHUNK_CHARS / 2 };
    let mut add_section = |mut section: &str| {
        while !section.is_empty() {
            let end = bounded_end(section, CHUNK_CHARS, minimum, layout.tabular());
            chunks.push(section[..end].to_owned());
            section = &section[end..];
        }
    };
    let mut section_start = 0;
    let mut previous_heading_end = 0;
    if layout.has_sections() {
        for (heading, _) in headings(text, layout) {
            // Consecutive titles (for example an H1 followed by an H2)
            // stay with the first body rather than making a title-only chunk.
            if !text[previous_heading_end..heading.start].trim().is_empty() {
                add_section(&text[section_start..heading.start]);
                section_start = heading.start;
            }
            previous_heading_end = heading.end;
        }
    }
    add_section(&text[section_start..]);
    chunks
}

#[derive(Clone, Copy)]
pub(super) struct TextMatch {
    pub byte: usize,
    pub character: usize,
    pub length: usize,
}

impl TextMatch {
    pub(super) fn at_character(text: &str, character: usize, length: usize) -> Self {
        Self {
            byte: byte_at(text, character),
            character,
            length,
        }
    }
}

pub(super) struct Excerpt<'a> {
    pub content: &'a str,
    pub start_char: usize,
    pub section: Option<&'a str>,
}

pub(super) fn excerpt(
    text: &str,
    layout: Layout,
    hit: TextMatch,
    preceding: usize,
    max_chars: usize,
) -> Excerpt<'_> {
    let mut section = None;
    let mut bounds = 0..text.len();
    if layout.has_sections() {
        for (range, heading) in headings(text, layout) {
            if range.start > hit.byte {
                bounds.end = range.start;
                break;
            }
            bounds.start = range.start;
            section = Some(heading);
        }
    }
    let matched = hit.length.min(max_chars);
    let look_back = preceding.min(max_chars - matched);
    let floor = text[..hit.byte]
        .char_indices()
        .rev()
        .take(look_back)
        .last()
        .map_or(hit.byte, |(byte, _)| byte);
    let start = if bounds.start >= floor {
        bounds.start
    } else {
        floor + structural_break(&text[floor..hit.byte], layout.tabular()).unwrap_or(0)
    };
    let match_end = hit.byte + byte_at(&text[hit.byte..], matched);
    let body = &text[start..bounds.end.max(match_end)];
    let before_match = text[start..hit.byte].chars().count();
    let minimum = (before_match + matched).max(if layout.tabular() { 1 } else { max_chars / 2 });
    let end = bounded_end(body, max_chars, minimum, layout.tabular());
    Excerpt {
        content: &body[..end],
        start_char: hit.character - before_match,
        section,
    }
}
