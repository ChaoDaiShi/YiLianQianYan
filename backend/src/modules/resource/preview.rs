//! Preview projection: bounded bytes in, `ResourcePreview` out.
//!
//! Dispatch is by canonical MIME rather than by the name the uploader typed.
//! Stored bytes are re-validated on the way out too — a record read back from
//! storage is not more trustworthy than the upload that produced it.

use crate::shared::resource::ResourceService;

use super::{
    limits::{validate_upload, MAX_PREVIEW_CHARS},
    model::{Presentation, ResourcePreview},
    parsers,
};

pub fn resource_preview(service: &ResourceService, id: &str) -> Result<ResourcePreview, String> {
    let resource = service
        .get(id)
        .map_err(|_| "资源记录不可用")?
        .ok_or("资源不存在")?;
    let bytes = service
        .read_bytes(id)
        .map_err(|_| "资源文件缺失或完整性检查失败")?;
    Ok(extract_preview(&resource.name, &resource.mime_type, &bytes))
}

pub fn extract_preview(name: &str, mime: &str, bytes: &[u8]) -> ResourcePreview {
    let mut preview = ResourcePreview {
        status: "failed".into(),
        kind: "unknown".into(),
        mime_type: mime.into(),
        text: None,
        truncated: false,
        width: None,
        height: None,
        error: None,
        presentation: None,
    };
    let mime = match validate_upload(name, mime, bytes.len(), 1) {
        Ok(mime) => mime,
        Err(error) => {
            preview.error = Some(error);
            return preview;
        }
    };
    preview.mime_type = mime.clone();
    let extracted: Result<Option<String>, String> = if mime.starts_with("text/") {
        preview.kind = "text".into();
        parsers::text::parse(bytes).map(Some)
    } else if mime.starts_with("image/") {
        preview.kind = "image".into();
        if mime == "image/gif" || mime == "image/webp" {
            Ok(None)
        } else {
            parsers::image::dimensions(&mime, bytes).map(|(width, height)| {
                preview.width = Some(width);
                preview.height = Some(height);
                None
            })
        }
    } else if mime.starts_with("audio/") || mime.starts_with("video/") {
        preview.kind = if mime.starts_with("audio/") {
            "audio"
        } else {
            "video"
        }
        .into();
        Ok(None)
    } else if mime == "application/pdf" {
        preview.kind = "pdf".into();
        parsers::pdf::page_count(bytes).and_then(|count| {
            preview.presentation = Some(Presentation {
                page_count: Some(count),
                ..Default::default()
            });
            parsers::pdf::parse(bytes).map(Some)
        })
    } else if mime == "application/vnd.openxmlformats-officedocument.presentationml.presentation" {
        preview.kind = "pptx".into();
        parsers::structured::slides(bytes).and_then(|p| {
            preview.presentation = Some(p);
            parsers::pptx::parse(bytes).map(Some)
        })
    } else if mime == "application/vnd.openxmlformats-officedocument.wordprocessingml.document" {
        preview.kind = "docx".into();
        parsers::structured::word(bytes).and_then(|p| {
            preview.presentation = Some(p);
            parsers::docx::parse(bytes).map(Some)
        })
    } else if mime == "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" {
        preview.kind = "xlsx".into();
        parsers::structured::workbook(bytes, false).and_then(|p| {
            preview.presentation = Some(p);
            parsers::xlsx::parse(bytes).map(Some)
        })
    } else if mime == "application/vnd.ms-excel" {
        preview.kind = "xls".into();
        parsers::structured::workbook(bytes, true).map(|p| {
            let text = p
                .sheets
                .iter()
                .map(|sheet| {
                    format!(
                        "[{}]\n{}",
                        sheet.name,
                        sheet
                            .rows
                            .iter()
                            .map(|r| r.join("\t"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            preview.presentation = Some(p);
            Some(text)
        })
    } else if mime == "application/rtf" {
        preview.kind = "rtf".into();
        parsers::rtf::parse(bytes).map(|p| {
            let text = p
                .blocks
                .iter()
                .filter_map(|block| block.text.as_deref())
                .collect::<Vec<_>>()
                .join("\n");
            preview.presentation = Some(p);
            Some(text)
        })
    } else if mime == "application/zip" {
        preview.kind = "zip".into();
        parsers::structured::zip_list(bytes).map(|p| {
            preview.presentation = Some(p);
            None
        })
    } else {
        preview.kind = match mime.as_str() {
            "application/msword" => "doc",
            "application/vnd.ms-powerpoint" => "ppt",
            "application/vnd.ms-excel" => "xls",
            "application/rtf" => "rtf",
            _ => "unknown",
        }
        .into();
        preview.status = "unsupported".into();
        preview.error = Some("此格式暂不支持内容解析".into());
        return preview;
    };
    match extracted {
        Ok(text) => {
            preview.status = "ready".into();
            if let Some(text) = text {
                preview.truncated = text.chars().count() > MAX_PREVIEW_CHARS;
                preview.text = Some(text.chars().take(MAX_PREVIEW_CHARS).collect());
            }
        }
        Err(error) => preview.error = Some(error),
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docx_structure_preserves_heading_runs_table_and_tail() {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        let xml = format!(
            r#"<w:document xmlns:w="w"><w:body><w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:rPr><w:b/><w:i/></w:rPr><w:t>Heading</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>{}Tail</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#,
            "a".repeat(MAX_PREVIEW_CHARS)
        );
        zip.write_all(xml.as_bytes()).unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let preview = extract_preview(
            "test.docx",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            &bytes,
        );
        let blocks = preview.presentation.unwrap().blocks;
        assert_eq!(blocks[0].kind, "heading");
        assert!(blocks[0].runs.as_ref().unwrap()[0].bold);
        assert!(blocks[0].runs.as_ref().unwrap()[0].italic);
        assert!(blocks[1].rows.as_ref().unwrap()[0][0].ends_with("Tail"));
    }

    #[test]
    fn rtf_preview_decodes_unicode_codepage_styles_and_groups() {
        let input = br"{\rtf1\ansi\ansicpg936{\fonttbl{\f0 FONT-NOT-BODY;}}\uc1 \u20013?\u25991? \'d6\'d0\'ce\'c4\par \b Bold\b0 \i italic\i0 \{brace\} \\ \par TAIL}";
        let preview = extract_preview("sample.rtf", "application/rtf", input);
        assert_eq!(preview.status, "ready", "{:?}", preview.error);
        let blocks = preview.presentation.unwrap().blocks;
        assert_eq!(blocks[0].text.as_deref(), Some("中文 中文"));
        assert!(blocks[1]
            .runs
            .as_ref()
            .unwrap()
            .iter()
            .any(|run| run.bold && run.text.contains("Bold")));
        assert!(blocks[1]
            .runs
            .as_ref()
            .unwrap()
            .iter()
            .any(|run| run.italic && run.text.contains("italic")));
        assert!(blocks[1].text.as_ref().unwrap().contains("{brace} \\"));
        assert_eq!(blocks[2].text.as_deref(), Some("TAIL"));
    }

    #[test]
    fn bounded_text_csv_and_code_previews_are_real_and_utf8_safe() {
        for name in ["notes.txt", "notes.md", "code.rs", "data.csv"] {
            let preview = extract_preview(name, "text/plain", "姓名,结果\n涟,完成".as_bytes());
            assert_eq!(preview.status, "ready");
            assert!(preview.text.unwrap().contains("涟,完成"));
        }
        let preview = extract_preview(
            "long.txt",
            "text/plain",
            "涟".repeat(MAX_PREVIEW_CHARS + 1).as_bytes(),
        );
        assert!(preview.truncated);
        assert_eq!(preview.text.unwrap().chars().count(), MAX_PREVIEW_CHARS);
        assert_eq!(
            extract_preview("bad.txt", "text/plain", &[255, 254]).status,
            "failed"
        );
    }

    #[test]
    fn office_previews_extract_document_text_and_cells() {
        use std::io::Write;
        fn archive(entries: &[(&str, &str)]) -> Vec<u8> {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            for (name, data) in entries {
                writer
                    .start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(data.as_bytes()).unwrap();
            }
            writer.finish().unwrap().into_inner()
        }
        let docx = archive(&[("word/document.xml", "<w:document xmlns:w=\"word\"><w:body><w:p><w:r><w:t>Actual &amp; bounded text</w:t></w:r></w:p></w:body></w:document>")]);
        let preview = extract_preview(
            "report.docx",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            &docx,
        );
        assert_eq!(preview.status, "ready");
        assert!(preview.text.unwrap().contains("Actual & bounded text"));
        let xlsx = archive(&[
            ("_rels/.rels", "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>"),
            ("[Content_Types].xml", "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/></Types>"),
            ("xl/workbook.xml", "<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets><sheet name=\"Data\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>"),
            ("xl/_rels/workbook.xml.rels", "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/></Relationships>"),
            ("xl/worksheets/sheet1.xml", "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><dimension ref=\"A1:B1\"/><sheetData><row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>Actual cell</t></is></c><c r=\"B1\"><v>42</v></c></row></sheetData></worksheet>")]);
        let preview = extract_preview(
            "sheet.xlsx",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            &xlsx,
        );
        assert_eq!(preview.status, "ready", "{:?}", preview.error);
        assert_eq!(
            preview.presentation.as_ref().unwrap().sheets[0].rows[0],
            vec!["Actual cell", "42"]
        );
        assert!(preview.text.unwrap().contains("42"));
        let dangerous = archive(&[(
            "word/document.xml",
            "<!DOCTYPE x [<!ENTITY e SYSTEM 'file:///ignored'>]><w:t>&e;</w:t>",
        )]);
        assert_eq!(
            extract_preview(
                "unsafe.docx",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                &dangerous
            )
            .status,
            "failed"
        );
    }

    #[test]
    fn pdf_preview_extracts_real_text_and_does_not_claim_scanned_ocr() {
        use pdf_extract::{dictionary, Document, Object, Stream};
        fn pdf(text: bool) -> Vec<u8> {
            let mut document = Document::with_version("1.5");
            let pages = document.new_object_id();
            let font = document.add_object(
                dictionary! {"Type"=>"Font", "Subtype"=>"Type1", "BaseFont"=>"Helvetica"},
            );
            let resources = document.add_object(dictionary! {"Font"=>dictionary!{"F1"=>font}});
            let content = document.add_object(Stream::new(
                dictionary! {},
                if text {
                    b"BT /F1 12 Tf 72 720 Td (Actual PDF text) Tj ET".to_vec()
                } else {
                    Vec::new()
                },
            ));
            let page = document
                .add_object(dictionary! {"Type"=>"Page", "Parent"=>pages, "Contents"=>content});
            document.objects.insert(pages, Object::Dictionary(dictionary!{"Type"=>"Pages", "Kids"=>vec![Object::Reference(page)], "Count"=>1, "Resources"=>resources, "MediaBox"=>vec![0.into(),0.into(),595.into(),842.into()]}));
            let catalog = document.add_object(dictionary! {"Type"=>"Catalog", "Pages"=>pages});
            document.trailer.set("Root", catalog);
            let mut bytes = Vec::new();
            document.save_to(&mut bytes).unwrap();
            bytes
        }
        let preview = extract_preview("text.pdf", "application/pdf", &pdf(true));
        assert_eq!(preview.status, "ready");
        assert_eq!(preview.presentation.as_ref().unwrap().page_count, Some(1));
        assert!(preview.text.unwrap().contains("Actual PDF text"));
        assert_eq!(
            extract_preview("scan.pdf", "application/pdf", &pdf(false)).status,
            "ready"
        );
    }

    #[test]
    fn image_preview_decodes_real_bounded_pixels() {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(3, 2)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let preview = extract_preview("paste.png", "image/png", bytes.get_ref());
        assert_eq!(preview.status, "ready");
        assert_eq!((preview.width, preview.height), (Some(3), Some(2)));
        assert!(preview.text.is_none());
        assert_eq!(
            extract_preview("bad.png", "image/png", b"not an image").status,
            "failed"
        );
    }
}
