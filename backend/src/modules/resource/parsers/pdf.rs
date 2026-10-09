//! Bounded PDF text extraction.
//!
//! Page count and object count are capped, encrypted documents are refused,
//! and output is bounded *while* it is produced — `BoundedText` aborts the
//! page walk as soon as the preview budget is exceeded rather than extracting
//! a whole document and truncating afterwards.

use crate::modules::resource::limits::MAX_PREVIEW_CHARS;

struct BoundedText {
    value: String,
    overflow: bool,
}
impl std::fmt::Write for BoundedText {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        let remaining = (MAX_PREVIEW_CHARS + 1).saturating_sub(self.value.chars().count());
        self.value.extend(value.chars().take(remaining));
        if value.chars().count() > remaining {
            self.overflow = true;
            return Err(std::fmt::Error);
        }
        Ok(())
    }
}
impl<'a> pdf_extract::ConvertToFmt for &'a mut BoundedText {
    type Writer = &'a mut BoundedText;
    fn convert(self) -> Self::Writer {
        self
    }
}

pub(crate) fn parse(bytes: &[u8]) -> Result<String, String> {
    let document = pdf_extract::Document::load_mem(bytes).map_err(|_| "无法解析 PDF")?;
    if document.is_encrypted() {
        return Err("不支持加密 PDF".into());
    }
    let pages = document.get_pages();
    if pages.len() > 128 || document.objects.len() > 50_000 {
        return Err("PDF 页数或对象数量超过限制".into());
    }
    let mut text = BoundedText {
        value: String::new(),
        overflow: false,
    };
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
