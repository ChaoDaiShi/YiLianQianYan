//! Actual-byte format inspection for goal deliverables.
use super::contracts::ProofError;
use calamine::Reader;
use quick_xml::events::Event;
use std::io::Cursor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatInspection {
    pub mime_type: String,
    pub extracted_text: Option<String>,
    pub text_complete: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

fn mismatch(message: impl ToString) -> ProofError {
    ProofError::new("proofloop_deliverable_format_mismatch", message.to_string())
}

pub fn inspect_deliverable_format(
    mime: &str,
    _name: &str,
    bytes: &[u8],
) -> Result<FormatInspection, ProofError> {
    let mut result = FormatInspection {
        mime_type: mime.into(),
        extracted_text: None,
        text_complete: true,
        width: None,
        height: None,
    };
    match mime {
        "application/json" => {
            let value: serde_json::Value = serde_json::from_slice(bytes).map_err(mismatch)?;
            result.extracted_text = Some(value.to_string());
        }
        "text/plain" | "text/markdown" | "text/csv" | "text/html" => {
            let text = std::str::from_utf8(bytes).map_err(mismatch)?;
            if text.trim().is_empty() {
                return Err(mismatch("交付物正文为空"));
            }
            result.extracted_text = Some(text.into());
        }
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
            let entries =
                crate::modules::resource::parsers::archive::zip_entries(bytes).map_err(mismatch)?;
            for name in ["[Content_Types].xml", "_rels/.rels", "word/document.xml"] {
                if !entries.contains_key(name) {
                    return Err(mismatch(format!("DOCX缺少{name}")));
                }
            }
            let mut reader =
                quick_xml::Reader::from_reader(entries["word/document.xml"].as_slice());
            let mut text = String::new();
            let mut in_text = false;
            let mut depth = 0usize;
            loop {
                match reader.read_event().map_err(mismatch)? {
                    Event::Start(e) => {
                        depth += 1;
                        if e.local_name().as_ref() == b"t" {
                            in_text = true;
                        }
                    }
                    Event::End(e) => {
                        depth = depth
                            .checked_sub(1)
                            .ok_or_else(|| mismatch("XML层级无效"))?;
                        if e.local_name().as_ref() == b"t" {
                            in_text = false;
                        }
                        if e.local_name().as_ref() == b"p" {
                            text.push('\n');
                        }
                    }
                    Event::Text(e) if in_text => {
                        let decoded = e.decode().map_err(mismatch)?;
                        text.push_str(&quick_xml::escape::unescape(&decoded).map_err(mismatch)?);
                    }
                    Event::GeneralRef(e) if in_text => {
                        let escaped = format!("&{};", e.decode().map_err(mismatch)?);
                        text.push_str(&quick_xml::escape::unescape(&escaped).map_err(mismatch)?);
                    }
                    Event::Eof => break,
                    _ => {}
                }
            }
            if depth != 0 || text.trim().is_empty() {
                return Err(mismatch("Word正文缺失或XML不完整"));
            }
            result.extracted_text = Some(text);
        }
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => {
            let mut book: calamine::Xlsx<_> =
                calamine::open_workbook_from_rs(Cursor::new(bytes)).map_err(mismatch)?;
            let sheets = book.sheet_names().to_vec();
            if sheets.is_empty() {
                return Err(mismatch("工作簿没有工作表"));
            }
            let mut text = String::new();
            for name in sheets {
                let range = book.worksheet_range(&name).map_err(mismatch)?;
                text.push_str(&format!("[{name}]\n"));
                for row in range.rows() {
                    text.push_str(
                        &row.iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join("\t"),
                    );
                    text.push('\n');
                }
            }
            result.extracted_text = Some(text);
        }
        "application/pdf" => {
            let document = pdf_extract::Document::load_mem(bytes).map_err(mismatch)?;
            let pages = document.get_pages();
            if pages.is_empty() {
                return Err(mismatch("PDF没有页面"));
            }
            let mut text = String::new();
            for page in pages.keys() {
                let mut output = pdf_extract::PlainTextOutput::new(&mut text);
                pdf_extract::output_doc_page(&document, &mut output, *page).map_err(mismatch)?;
            }
            result.extracted_text = Some(text);
        }
        "image/png" | "image/jpeg" => {
            let format = if mime == "image/png" {
                image::ImageFormat::Png
            } else {
                image::ImageFormat::Jpeg
            };
            let image = image::load_from_memory_with_format(bytes, format).map_err(mismatch)?;
            if image.width() == 0 || image.height() == 0 {
                return Err(mismatch("图像尺寸为空"));
            }
            result.width = Some(image.width());
            result.height = Some(image.height());
        }
        _ => {
            return Err(ProofError::new(
                "proofloop_deliverable_format_unsupported",
                format!("尚未支持交付物格式 {mime}"),
            ))
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn declared_formats_require_real_bytes() {
        assert!(inspect_deliverable_format("application/pdf", "fake.pdf", b"plain text").is_err());
        assert!(
            inspect_deliverable_format("image/png", "broken.png", b"\x89PNG\r\n\x1a\n").is_err()
        );
        assert!(inspect_deliverable_format("application/json", "bad.json", b"{broken").is_err());
        assert!(
            inspect_deliverable_format("application/json", "ok.json", br#"{"ok":true}"#).is_ok()
        );
    }
    #[test]
    fn unsupported_is_distinct_from_invalid() {
        assert_eq!(
            inspect_deliverable_format("application/unknown", "x", b"data")
                .unwrap_err()
                .code,
            "proofloop_deliverable_format_unsupported"
        );
    }
}
