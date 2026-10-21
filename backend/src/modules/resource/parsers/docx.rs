//! Bounded Word (`.docx`) text extraction.
//!
//! Only `word/document.xml` is read. A DTD is refused outright, which is what
//! stops an XXE payload from resolving an external entity during parsing.

use quick_xml::events::Event;

use super::{archive::zip_entries, LimitedText};
use crate::modules::resource::limits::MAX_PREVIEW_CHARS;

pub(crate) fn parse(bytes: &[u8]) -> Result<String, String> {
    parse_with_limit(bytes, MAX_PREVIEW_CHARS)
}

pub(crate) fn parse_with_limit(bytes: &[u8], max_chars: usize) -> Result<String, String> {
    let entries = zip_entries(bytes)?;
    let xml = entries.get("word/document.xml").ok_or("缺少 Word 正文")?;
    let mut reader = quick_xml::Reader::from_reader(xml.as_slice());
    let mut in_text = false;
    let mut text = LimitedText::new(max_chars);
    loop {
        match reader.read_event() {
            Ok(Event::DocType(_)) => return Err("不支持含 DTD 的文档".into()),
            Ok(Event::Start(event)) => {
                if event.local_name().as_ref() == b"t" {
                    in_text = true;
                }
            }
            Ok(Event::End(event)) => {
                if event.local_name().as_ref() == b"t" {
                    in_text = false;
                }
                if event.local_name().as_ref() == b"p" {
                    text.push("\n");
                }
            }
            Ok(Event::Text(event)) if in_text => {
                let decoded = event.decode().map_err(|_| "Word 正文编码无效")?;
                let decoded =
                    quick_xml::escape::unescape(&decoded).map_err(|_| "Word 正文实体无效")?;
                text.push(&decoded);
            }
            Ok(Event::GeneralRef(event)) if in_text => {
                let name = event.decode().map_err(|_| "Word 正文实体无效")?;
                let escaped = format!("&{name};");
                text.push(
                    &quick_xml::escape::unescape(&escaped).map_err(|_| "不支持的 Word 实体")?,
                );
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err("Word 正文 XML 无效".into()),
            _ => {}
        }
        if text.overflow {
            break;
        }
    }
    Ok(text.value)
}
