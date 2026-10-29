//! Bounded PDF text extraction.
//!
//! Page count and object count are capped, encrypted documents are refused,
//! and output is bounded *while* it is produced — `LimitedText` aborts the
//! page walk as soon as the requested text budget is exceeded rather than extracting
//! a whole document and truncating afterwards.

use super::LimitedText;
use crate::modules::resource::limits::MAX_PREVIEW_CHARS;

impl<'a> pdf_extract::ConvertToFmt for &'a mut LimitedText {
    type Writer = &'a mut LimitedText;
    fn convert(self) -> Self::Writer {
        self
    }
}

pub(crate) fn page_count(bytes: &[u8]) -> Result<usize, String> {
    let document = pdf_extract::Document::load_mem(bytes).map_err(|_| "无法解析 PDF")?;
    if document.is_encrypted() {
        return Err("不支持加密 PDF".into());
    }
    let count = document.get_pages().len();
    if count > 128 || document.objects.len() > 50_000 {
        return Err("PDF 页数或对象数量超过限制".into());
    }
    Ok(count)
}

pub(crate) fn parse(bytes: &[u8]) -> Result<String, String> {
    parse_with_limit(bytes, MAX_PREVIEW_CHARS)
}

pub(crate) fn parse_with_limit(bytes: &[u8], max_chars: usize) -> Result<String, String> {
    let document = pdf_extract::Document::load_mem(bytes).map_err(|_| "无法解析 PDF")?;
    if document.is_encrypted() {
        return Err("不支持加密 PDF".into());
    }
    let pages = document.get_pages();
    if pages.len() > 128 || document.objects.len() > 50_000 {
        return Err("PDF 页数或对象数量超过限制".into());
    }
    let mut text = LimitedText::new(max_chars);
    for page in pages.keys() {
        let result = {
            let mut output = pdf_extract::PlainTextOutput::new(&mut text);
            pdf_extract::output_doc_page(&document, &mut output, *page)
        };
        if text.overflow {
            break;
        }
        result.map_err(|_| "PDF 文本提取失败")?;
    }
    Ok(text.value)
}
