use super::{archive::zip_entries, pptx};
use crate::modules::resource::model::{
    Presentation, PreviewBlock, PreviewEntry, PreviewRun, PreviewSheet, PreviewSlide,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use calamine::Reader;
use quick_xml::events::Event;
use std::{collections::BTreeMap, io::Cursor};

fn image_block(
    entries: &BTreeMap<String, Vec<u8>>,
    source: &str,
    id: &str,
) -> Result<Option<PreviewBlock>, String> {
    let Some(rels) = entries.get(&pptx::rels_path(source)) else {
        return Ok(None);
    };
    let relations = pptx::relationships(rels)?;
    let Some(rel) = relations.get(id) else {
        return Ok(None);
    };
    if rel.external || !rel.kind.ends_with("/image") {
        return Ok(None);
    }
    let path = pptx::part_path(source, &rel.target)?;
    let Some(data) = entries.get(&path) else {
        return Ok(None);
    };
    let mime = match path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return Ok(None),
    };
    Ok(Some(PreviewBlock {
        kind: "image".into(),
        src: Some(format!("data:{mime};base64,{}", STANDARD.encode(data))),
        alt: Some(path.rsplit('/').next().unwrap_or("").into()),
        ..Default::default()
    }))
}

pub(crate) fn word(bytes: &[u8]) -> Result<Presentation, String> {
    let entries = zip_entries(bytes)?;
    let xml = entries.get("word/document.xml").ok_or("缺少 Word 正文")?;
    // The existing parser checks DTD and applies the text budget.
    super::docx::parse(bytes)?;
    let mut r = quick_xml::Reader::from_reader(xml.as_slice());
    let mut result = Presentation::default();
    let mut paragraph = PreviewBlock::default();
    let mut run = PreviewRun::default();
    let mut in_text = false;
    let mut in_run = false;
    let mut in_table = false;
    let mut in_cell = false;
    let mut cell = String::new();
    let mut row = Vec::new();
    let mut table = Vec::new();
    loop {
        match r.read_event().map_err(|_| "Word 正文 XML 无效")? {
            Event::Start(e) => match e.local_name().as_ref() {
                b"tbl" => {
                    in_table = true;
                    table = Vec::new();
                }
                b"tr" if in_table => row = Vec::new(),
                b"tc" if in_table => {
                    in_cell = true;
                    cell.clear();
                }
                b"p" => {
                    paragraph = PreviewBlock {
                        kind: "paragraph".into(),
                        runs: Some(Vec::new()),
                        ..Default::default()
                    };
                }
                b"r" => {
                    in_run = true;
                    run = PreviewRun::default();
                }
                b"t" => in_text = true,
                b"blip" => {
                    for a in e.attributes() {
                        let a = a.map_err(|_| "图片关系无效")?;
                        if a.key.as_ref().ends_with(b":embed") {
                            let id = a
                                .decode_and_unescape_value(r.decoder())
                                .map_err(|_| "图片关系无效")?;
                            if let Some(image) = image_block(&entries, "word/document.xml", &id)? {
                                result.blocks.push(image);
                            }
                        }
                    }
                }
                _ => {}
            },
            Event::Empty(e) if e.local_name().as_ref() == b"blip" => {
                for a in e.attributes() {
                    let a = a.map_err(|_| "图片关系无效")?;
                    if a.key.as_ref().ends_with(b":embed") {
                        let id = a
                            .decode_and_unescape_value(r.decoder())
                            .map_err(|_| "图片关系无效")?;
                        if let Some(image) = image_block(&entries, "word/document.xml", &id)? {
                            result.blocks.push(image);
                        }
                    }
                }
            }
            Event::Empty(e) => match e.local_name().as_ref() {
                b"pStyle" => {
                    for a in e.attributes() {
                        let a = a.map_err(|_| "段落样式无效")?;
                        if a.key.as_ref().ends_with(b":val") {
                            let value = a
                                .decode_and_unescape_value(r.decoder())
                                .map_err(|_| "段落样式无效")?;
                            if let Some(level) = match value.as_ref() {
                                "Title" => Some(1),
                                "Subtitle" => Some(2),
                                _ => value
                                    .strip_prefix("Heading")
                                    .and_then(|x| x.parse::<u8>().ok()),
                            } {
                                paragraph.kind = "heading".into();
                                paragraph.level = Some(level);
                            }
                        }
                    }
                }
                b"b" if in_run => run.bold = true,
                b"i" if in_run => run.italic = true,
                b"u" if in_run => run.underline = true,
                b"tab" if in_run => run.text.push('\t'),
                b"br" if in_run => run.text.push('\n'),
                _ => {}
            },
            Event::DocType(_) => return Err("不支持含 DTD 的文档".into()),
            Event::Text(e) if in_text => {
                let decoded = e.decode().map_err(|_| "Word 正文编码无效")?;
                let text =
                    quick_xml::escape::unescape(&decoded).map_err(|_| "Word 正文实体无效")?;
                run.text.push_str(&text);
            }
            Event::GeneralRef(e) if in_text => {
                let name = e.decode().map_err(|_| "Word 正文实体无效")?;
                let escaped = format!("&{name};");
                let value =
                    quick_xml::escape::unescape(&escaped).map_err(|_| "不支持的 Word 实体")?;
                run.text.push_str(&value);
            }
            Event::End(e) => match e.local_name().as_ref() {
                b"t" => in_text = false,
                b"r" => {
                    in_run = false;
                    if !run.text.is_empty() {
                        paragraph
                            .runs
                            .as_mut()
                            .unwrap()
                            .push(std::mem::take(&mut run));
                    }
                }
                b"p" => {
                    let text = paragraph
                        .runs
                        .as_ref()
                        .unwrap()
                        .iter()
                        .map(|r| r.text.as_str())
                        .collect::<String>();
                    if in_cell {
                        if !cell.is_empty() {
                            cell.push('\n');
                        }
                        cell.push_str(&text);
                    } else {
                        paragraph.text = Some(text);
                        result.blocks.push(std::mem::take(&mut paragraph));
                    }
                }
                b"tc" => {
                    in_cell = false;
                    row.push(std::mem::take(&mut cell));
                }
                b"tr" => table.push(std::mem::take(&mut row)),
                b"tbl" => {
                    in_table = false;
                    result.blocks.push(PreviewBlock {
                        kind: "table".into(),
                        rows: Some(std::mem::take(&mut table)),
                        ..Default::default()
                    });
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(result)
}

fn collect_sheets<RS, R>(mut book: R) -> Result<Presentation, String>
where
    RS: std::io::Read + std::io::Seek,
    R: Reader<RS>,
{
    let mut result = Presentation::default();
    let names = book.sheet_names().to_vec();
    if names.len() > 16 {
        return Err("工作表数量超过限制".into());
    }
    for name in names {
        let range = book.worksheet_range(&name).map_err(|_| "无法读取工作表")?;
        if range.height() > 10_000 || range.width() > 256 {
            return Err("工作表行列或单元格数量超过限制".into());
        }
        let mut rows = Vec::new();
        for row in range.rows() {
            let mut cells = Vec::new();
            for cell in row {
                cells.push(cell.to_string());
            }
            rows.push(cells);
        }
        result.sheets.push(PreviewSheet { name, rows });
    }
    Ok(result)
}

pub(crate) fn workbook(bytes: &[u8], legacy: bool) -> Result<Presentation, String> {
    if legacy {
        let book: calamine::Xls<_> =
            calamine::open_workbook_from_rs(Cursor::new(bytes)).map_err(|_| "无法解析工作簿")?;
        collect_sheets(book)
    } else {
        super::xlsx::parse(bytes)?;
        let book: calamine::Xlsx<_> =
            calamine::open_workbook_from_rs(Cursor::new(bytes)).map_err(|_| "无法解析工作簿")?;
        collect_sheets(book)
    }
}

pub(crate) fn slides(bytes: &[u8]) -> Result<Presentation, String> {
    let entries = zip_entries(bytes)?;
    let mut result = Presentation::default();
    for slide in pptx::slides(bytes, usize::MAX)? {
        let xml = entries.get(&slide.part).ok_or("幻灯片部件不存在")?;
        let mut reader = quick_xml::Reader::from_reader(xml.as_slice());
        let mut blocks = Vec::new();
        let mut table = Vec::new();
        let mut row = Vec::new();
        let mut cell = String::new();
        let mut paragraph = String::new();
        let mut in_table = false;
        let mut in_text = false;
        loop {
            match reader.read_event().map_err(|_| "幻灯片 XML 无效")? {
                Event::Start(e) => match e.local_name().as_ref() {
                    b"tbl" => {
                        in_table = true;
                        table.clear();
                    }
                    b"tr" if in_table => row.clear(),
                    b"tc" if in_table => cell.clear(),
                    b"p" => paragraph.clear(),
                    b"t" => in_text = true,
                    _ => {}
                },
                Event::Empty(e) if e.local_name().as_ref() == b"blip" => {
                    for attr in e.attributes() {
                        let attr = attr.map_err(|_| "图片关系无效")?;
                        if attr.key.as_ref().ends_with(b":embed") {
                            let id = attr
                                .decode_and_unescape_value(reader.decoder())
                                .map_err(|_| "图片关系无效")?;
                            if let Some(image) = image_block(&entries, &slide.part, &id)? {
                                blocks.push(image);
                            }
                        }
                    }
                }
                Event::Text(e) if in_text => {
                    let decoded = e.decode().map_err(|_| "幻灯片文字编码无效")?;
                    paragraph.push_str(
                        &quick_xml::escape::unescape(&decoded).map_err(|_| "幻灯片文字实体无效")?,
                    );
                }
                Event::GeneralRef(e) if in_text => {
                    let reference = format!("&{};", e.decode().map_err(|_| "幻灯片实体编码无效")?);
                    paragraph.push_str(
                        &quick_xml::escape::unescape(&reference).map_err(|_| "幻灯片实体无效")?,
                    );
                }
                Event::End(e) => match e.local_name().as_ref() {
                    b"t" => in_text = false,
                    b"p" => {
                        if in_table {
                            if !cell.is_empty() {
                                cell.push('\n');
                            }
                            cell.push_str(&paragraph);
                        } else if !paragraph.is_empty() {
                            blocks.push(PreviewBlock {
                                kind: "paragraph".into(),
                                text: Some(std::mem::take(&mut paragraph)),
                                ..Default::default()
                            });
                        }
                    }
                    b"tc" if in_table => row.push(std::mem::take(&mut cell)),
                    b"tr" if in_table => table.push(std::mem::take(&mut row)),
                    b"tbl" => {
                        in_table = false;
                        blocks.push(PreviewBlock {
                            kind: "table".into(),
                            rows: Some(std::mem::take(&mut table)),
                            ..Default::default()
                        });
                    }
                    _ => {}
                },
                Event::Eof => break,
                _ => {}
            }
        }
        result.slides.push(PreviewSlide {
            number: slide.page,
            blocks,
        });
    }
    Ok(result)
}

pub(crate) fn zip_list(bytes: &[u8]) -> Result<Presentation, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "无法读取 ZIP")?;
    if zip.len() > 512 {
        return Err("文档条目数超过限制".into());
    }
    let mut result = Presentation::default();
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|_| "ZIP 条目无效")?;
        result.entries.push(PreviewEntry {
            name: entry.name().into(),
            size: entry.size(),
            directory: entry.is_dir(),
        });
    }
    Ok(result)
}
