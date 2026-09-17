use super::MAX_PREVIEW_CHARS;
use calamine::Reader;
use quick_xml::events::Event;
use std::io::{Cursor, Read};

const MAX_ZIP_ENTRIES: usize = 512;
const MAX_ENTRY_BYTES: u64 = 8 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 64 * 1024 * 1024;

fn zip_entries(bytes: &[u8]) -> Result<std::collections::BTreeMap<String, Vec<u8>>, String> {
    let mut zip =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "无法读取 Office 文档容器")?;
    if zip.len() > MAX_ZIP_ENTRIES {
        return Err("文档条目数超过限制".into());
    }
    let mut total = 0u64;
    let mut entries = std::collections::BTreeMap::new();
    for index in 0..zip.len() {
        let file = zip.by_index(index).map_err(|_| "Office 文档条目无效")?;
        if file.is_dir() {
            continue;
        }
        total = total.checked_add(file.size()).ok_or("文档展开大小无效")?;
        if file.size() > MAX_ENTRY_BYTES || total > MAX_EXPANDED_BYTES {
            return Err("文档展开大小超过限制".into());
        }
        if !file.name().ends_with(".xml") && !file.name().ends_with(".rels") {
            continue;
        }
        let name = file.name().to_string();
        let mut content = Vec::new();
        file.take(MAX_ENTRY_BYTES + 1)
            .read_to_end(&mut content)
            .map_err(|_| "Office 文档读取失败")?;
        if content.len() as u64 > MAX_ENTRY_BYTES {
            return Err("文档条目超过限制".into());
        }
        if entries.insert(name, content).is_some() {
            return Err("文档有重复条目".into());
        }
    }
    Ok(entries)
}

pub(super) fn docx(bytes: &[u8]) -> Result<String, String> {
    let entries = zip_entries(bytes)?;
    let xml = entries.get("word/document.xml").ok_or("缺少 Word 正文")?;
    let mut reader = quick_xml::Reader::from_reader(xml.as_slice());
    let mut in_text = false;
    let mut text = String::new();
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
                    text.push('\n');
                }
            }
            Ok(Event::Text(event)) if in_text => {
                let decoded = event.decode().map_err(|_| "Word 正文编码无效")?;
                let decoded =
                    quick_xml::escape::unescape(&decoded).map_err(|_| "Word 正文实体无效")?;
                text.extend(
                    decoded.chars().take(
                        MAX_PREVIEW_CHARS + 1 - text.chars().count().min(MAX_PREVIEW_CHARS + 1),
                    ),
                );
            }
            Ok(Event::GeneralRef(event)) if in_text => {
                let name = event.decode().map_err(|_| "Word 正文实体无效")?;
                let escaped = format!("&{name};");
                text.push_str(
                    &quick_xml::escape::unescape(&escaped).map_err(|_| "不支持的 Word 实体")?,
                );
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err("Word 正文 XML 无效".into()),
            _ => {}
        }
        if text.chars().count() > MAX_PREVIEW_CHARS {
            break;
        }
    }
    Ok(text)
}

fn check_cell_reference(raw: &[u8]) -> Result<(), String> {
    let text = std::str::from_utf8(raw).map_err(|_| "单元格坐标无效")?;
    for cell in text.split(':') {
        let mut column = 0usize;
        let mut row = String::new();
        for c in cell.chars().filter(|c| *c != '$') {
            if c.is_ascii_alphabetic() && row.is_empty() {
                column = column
                    .saturating_mul(26)
                    .saturating_add(c.to_ascii_uppercase() as usize - 'A' as usize + 1);
            } else if c.is_ascii_digit() {
                row.push(c);
            } else {
                return Err("单元格坐标无效".into());
            }
        }
        if column > 256 || row.parse::<usize>().unwrap_or(usize::MAX) > 10_000 {
            return Err("工作表行列范围超过解析限制".into());
        }
    }
    Ok(())
}

pub(super) fn xlsx(bytes: &[u8]) -> Result<String, String> {
    let entries = zip_entries(bytes)?;
    for (name, xml) in &entries {
        if !name.starts_with("xl/worksheets/") {
            continue;
        }
        let mut reader = quick_xml::Reader::from_reader(xml.as_slice());
        let mut cells = 0;
        loop {
            match reader.read_event() {
                Ok(Event::DocType(_)) => return Err("不支持含 DTD 的工作表".into()),
                Ok(Event::Start(event)) | Ok(Event::Empty(event)) => {
                    let local = event.local_name();
                    if local.as_ref() == b"c" {
                        cells += 1;
                        if cells > 50_000 {
                            return Err("单元格数量超过限制".into());
                        }
                    }
                    for attribute in event.attributes() {
                        let attribute = attribute.map_err(|_| "工作表 XML 属性无效")?;
                        if (local.as_ref() == b"c" && attribute.key.as_ref() == b"r")
                            || (local.as_ref() == b"dimension" && attribute.key.as_ref() == b"ref")
                        {
                            check_cell_reference(&attribute.value)?;
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(_) => return Err("工作表 XML 无效".into()),
                _ => {}
            }
        }
    }
    let mut workbook: calamine::Xlsx<_> =
        calamine::open_workbook_from_rs(Cursor::new(bytes)).map_err(|_| "无法解析工作簿")?;
    let sheets = workbook.sheet_names().to_vec();
    if sheets.len() > 16 {
        return Err("工作表数量超过限制".into());
    }
    let mut text = String::new();
    for name in sheets {
        text.push_str(&format!("[{name}]\n"));
        let range = workbook
            .worksheet_range(&name)
            .map_err(|_| "无法读取工作表")?;
        for row in range.rows() {
            for (index, cell) in row.iter().enumerate() {
                if index > 0 {
                    text.push('\t');
                }
                text.extend(cell.to_string().chars().take(MAX_PREVIEW_CHARS + 1));
                if text.chars().count() > MAX_PREVIEW_CHARS {
                    return Ok(text);
                }
            }
            text.push('\n');
        }
    }
    Ok(text)
}

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

pub(super) fn pdf(bytes: &[u8]) -> Result<String, String> {
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
