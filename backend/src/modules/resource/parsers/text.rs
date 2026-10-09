//! The bounded UTF-8 text family: plain text, Markdown, CSV/TSV and code.
//!
//! Every type the upload policy canonicalises to `text/*` shares this one
//! extraction path — strict UTF-8 decoding, truncated later by the preview
//! projection. There is deliberately no separate `markdown` or `csv` parser
//! yet: neither has format-specific normalization to perform, and splitting
//! them now would produce files that only forward to this one. Add a parser
//! here when a format genuinely needs its own handling.

pub(crate) fn parse(bytes: &[u8]) -> Result<String, String> {
    std::str::from_utf8(bytes)
        .map(|text| text.to_string())
        .map_err(|_| "文件不是有效 UTF-8 文本".into())
}
