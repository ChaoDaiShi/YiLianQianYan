//! Bounded parsers: bounded bytes in, normalized content out.
//!
//! Every parser here is a pure function of the bytes it is given. None of them
//! reads or writes a Task, touches the database, or decides policy — the
//! upload policy runs first in `limits`, and the preview projection in
//! `preview` decides what the parsed content means.
//!
//! `archive` is not a format: it is the shared bounded OOXML container reader
//! that `docx` and `xlsx` both build on.

pub(crate) mod archive;
pub(crate) mod docx;
pub(crate) mod image;
pub(crate) mod pdf;
pub(crate) mod text;
pub(crate) mod xlsx;

/// Keeps one overflow character so callers can distinguish complete text from
/// output that exceeded their budget. Character counts are tracked incrementally.
pub(super) struct LimitedText {
    pub value: String,
    pub overflow: bool,
    limit: usize,
    count: usize,
}

impl LimitedText {
    pub fn new(limit: usize) -> Self {
        Self {
            value: String::new(),
            overflow: false,
            limit,
            count: 0,
        }
    }

    pub fn push(&mut self, value: &str) {
        let remaining = self.limit.saturating_add(1).saturating_sub(self.count);
        for character in value.chars().take(remaining) {
            self.value.push(character);
            self.count += 1;
        }
        self.overflow = self.count > self.limit;
    }
}

impl std::fmt::Write for LimitedText {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        self.push(value);
        if self.overflow {
            Err(std::fmt::Error)
        } else {
            Ok(())
        }
    }
}
