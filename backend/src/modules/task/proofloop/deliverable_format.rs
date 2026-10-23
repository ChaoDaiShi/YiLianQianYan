//! Actual-byte format inspection for goal deliverables.
use super::contracts::ProofError;
use calamine::Reader;
use quick_xml::events::Event;
use std::collections::HashMap;
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

struct ParsedXmlPart {
    root_name: Vec<u8>,
    root_attributes: HashMap<Vec<u8>, Vec<u8>>,
    elements: Vec<(Vec<u8>, HashMap<Vec<u8>, Vec<u8>>, usize)>,
}

fn parse_xml_part(bytes: &[u8]) -> Result<ParsedXmlPart, ProofError> {
    let mut reader = quick_xml::Reader::from_reader(bytes);
    let mut depth = 0usize;
    let mut open_elements: Vec<Vec<u8>> = Vec::new();
    let mut root_name = None;
    let mut root_attributes = HashMap::new();
    let mut elements = Vec::new();
    let mut root_closed = false;
    loop {
        match reader.read_event().map_err(mismatch)? {
            Event::Start(event) => {
                let element_name = event.name().as_ref().to_vec();
                if depth == 0 {
                    if root_name.is_some() || root_closed {
                        return Err(mismatch("XML根结构无效"));
                    }
                    root_name = Some(element_name.clone());
                    root_attributes = xml_attributes(&event)?;
                } else {
                    elements.push((element_name.clone(), xml_attributes(&event)?, depth));
                }
                open_elements.push(element_name);
                depth += 1;
            }
            Event::Empty(event) => {
                if depth == 0 {
                    if root_name.is_some() || root_closed {
                        return Err(mismatch("XML根结构无效"));
                    }
                    root_name = Some(event.name().as_ref().to_vec());
                    root_attributes = xml_attributes(&event)?;
                    root_closed = true;
                } else {
                    elements.push((
                        event.name().as_ref().to_vec(),
                        xml_attributes(&event)?,
                        depth,
                    ));
                }
            }
            Event::End(event) => {
                let opened = open_elements
                    .pop()
                    .ok_or_else(|| mismatch("XML结束标签没有对应开始标签"))?;
                if opened.as_slice() != event.name().as_ref() {
                    return Err(mismatch("XML开始与结束标签不匹配"));
                }
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| mismatch("XML层级无效"))?;
                if depth == 0 {
                    root_closed = true;
                }
            }
            Event::DocType(_) => return Err(mismatch("OOXML部件不能包含DTD")),
            Event::Text(event) if depth == 0 => {
                if !event.decode().map_err(mismatch)?.trim().is_empty() {
                    return Err(mismatch("XML根结构外含有文本"));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if depth != 0 || !open_elements.is_empty() || !root_closed {
        return Err(mismatch("XML根结构不完整"));
    }
    Ok(ParsedXmlPart {
        root_name: root_name.ok_or_else(|| mismatch("XML根结构缺失"))?,
        root_attributes,
        elements,
    })
}

fn xml_attributes(
    event: &quick_xml::events::BytesStart<'_>,
) -> Result<HashMap<Vec<u8>, Vec<u8>>, ProofError> {
    event
        .attributes()
        .map(|attribute| {
            let attribute = attribute.map_err(mismatch)?;
            Ok((
                attribute.key.as_ref().to_vec(),
                attribute.value.as_ref().to_vec(),
            ))
        })
        .collect()
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
            let content_types = parse_xml_part(&entries["[Content_Types].xml"])?;
            let types_namespace = content_types
                .root_attributes
                .get(b"xmlns".as_slice())
                .is_some_and(|value| {
                    value.as_slice()
                        == b"http://schemas.openxmlformats.org/package/2006/content-types"
                });
            let has_document_content_type = content_types.elements.iter().any(|(name, attrs, _)| {
                name.as_slice() == b"Override"
                    && attrs
                        .get(b"PartName".as_slice())
                        .is_some_and(|value| value.as_slice() == b"/word/document.xml")
                    && attrs
                        .get(b"ContentType".as_slice())
                        .is_some_and(|value| {
                            value.as_slice()
                                == b"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
                        })
            });
            if content_types.root_name.as_slice() != b"Types"
                || !types_namespace
                || !has_document_content_type
            {
                return Err(mismatch("DOCX内容类型未声明Word主文档部件"));
            }

            let root_relationships = parse_xml_part(&entries["_rels/.rels"])?;
            let rels_namespace = root_relationships
                .root_attributes
                .get(b"xmlns".as_slice())
                .is_some_and(|value| {
                    value.as_slice()
                        == b"http://schemas.openxmlformats.org/package/2006/relationships"
                });
            let has_office_document_relationship =
                root_relationships.elements.iter().any(|(name, attrs, _)| {
                    name.as_slice() == b"Relationship"
                        && attrs.get(b"Id".as_slice()).is_some_and(|value| !value.is_empty())
                        && attrs
                            .get(b"Type".as_slice())
                            .is_some_and(|value| {
                                value.as_slice()
                                    == b"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"
                            })
                        && attrs
                            .get(b"Target".as_slice())
                            .is_some_and(|value| value.as_slice() == b"word/document.xml")
                });
            if root_relationships.root_name.as_slice() != b"Relationships"
                || !rels_namespace
                || !has_office_document_relationship
            {
                return Err(mismatch("DOCX根关系未指向Word主文档部件"));
            }

            let document_part = parse_xml_part(&entries["word/document.xml"])?;
            let word_namespace = document_part
                .root_attributes
                .get(b"xmlns:w".as_slice())
                .is_some_and(|value| {
                    value.as_slice()
                        == b"http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                });
            let has_word_body = document_part
                .elements
                .iter()
                .any(|(name, _, depth)| name.as_slice() == b"w:body" && *depth == 1);
            let has_word_paragraph = document_part
                .elements
                .iter()
                .any(|(name, _, depth)| name.as_slice() == b"w:p" && *depth >= 2);
            let has_word_text = document_part
                .elements
                .iter()
                .any(|(name, _, depth)| name.as_slice() == b"w:t" && *depth >= 3);
            if document_part.root_name.as_slice() != b"w:document"
                || !word_namespace
                || !has_word_body
                || !has_word_paragraph
                || !has_word_text
            {
                return Err(mismatch("DOCX不包含有效的Word文档正文结构"));
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
                        if e.name().as_ref() == b"w:t" {
                            in_text = true;
                        }
                    }
                    Event::End(e) => {
                        depth = depth
                            .checked_sub(1)
                            .ok_or_else(|| mismatch("XML层级无效"))?;
                        if e.name().as_ref() == b"w:t" {
                            in_text = false;
                        }
                        if e.name().as_ref() == b"w:p" {
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
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => {
            let entries =
                crate::modules::resource::parsers::archive::zip_entries(bytes).map_err(mismatch)?;
            let types = parse_xml_part(
                entries
                    .get("[Content_Types].xml")
                    .ok_or_else(|| mismatch("PPTX缺少内容类型声明"))?,
            )?;
            if !types.elements.iter().any(|(_,attributes,_)|attributes.get(b"ContentType".as_slice()).is_some_and(|value|value.as_slice()==b"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml")) {
                return Err(mismatch("PPTX未声明演示文稿主部件"));
            }
            let text =
                crate::modules::resource::extract_document_text("deliverable.pptx", mime, bytes)
                    .map_err(mismatch)?;
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
    use std::io::Write;

    fn docx_package(parts: &[(&str, &[u8])]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut archive = zip::ZipWriter::new(cursor);
        for (name, contents) in parts {
            archive
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            archive.write_all(contents).unwrap();
        }
        archive.finish().unwrap().into_inner()
    }

    fn valid_docx_parts() -> [(&'static str, &'static [u8]); 3] {
        [
            (
                "[Content_Types].xml",
                br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
            ),
            (
                "_rels/.rels",
                br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
            ),
            (
                "word/document.xml",
                br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>hello</w:t></w:r></w:p></w:body></w:document>"#,
            ),
        ]
    }

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

    #[test]
    fn docx_requires_word_content_type_relationship_and_document_structure() {
        let valid_parts = valid_docx_parts();
        let valid = docx_package(&valid_parts);
        assert!(inspect_deliverable_format(
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "valid.docx",
            &valid,
        )
        .is_ok());

        let mut invalid_content_types = valid_docx_parts();
        invalid_content_types[0].1 = b"not xml";
        let invalid_content_types = docx_package(&invalid_content_types);
        assert!(inspect_deliverable_format(
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "invalid-content-types.docx",
            &invalid_content_types,
        )
        .is_err());

        let mut invalid_relationships = valid_docx_parts();
        invalid_relationships[1].1 =
            br#"<Relationships><Relationship Target="word/other.xml"/></Relationships>"#;
        let invalid_relationships = docx_package(&invalid_relationships);
        assert!(inspect_deliverable_format(
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "invalid-relationships.docx",
            &invalid_relationships,
        )
        .is_err());

        let mut invalid_document = valid_docx_parts();
        invalid_document[2].1 = br#"<doc><t>hello</t></doc>"#;
        let invalid_document = docx_package(&invalid_document);
        assert!(inspect_deliverable_format(
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "invalid-document.docx",
            &invalid_document,
        )
        .is_err());
    }
}
