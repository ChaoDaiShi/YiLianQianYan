//! Bounded Excel (`.xlsx`) cell extraction.
//!
//! Worksheet XML is pre-scanned and rejected before the workbook is handed to
//! calamine: a DTD is refused, and every cell or dimension reference must name
//! a column within 256 and a row within 10,000. That keeps a hostile range
//! declaration from turning into a huge allocation downstream. Sheet count and
//! total cell count are capped too, and the text budget ends extraction early.

use calamine::Reader;
use quick_xml::events::Event;
use std::io::Cursor;

use super::archive::zip_entries;
use crate::modules::resource::limits::MAX_PREVIEW_CHARS;

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

pub(crate) fn parse(bytes: &[u8]) -> Result<String, String> {
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
