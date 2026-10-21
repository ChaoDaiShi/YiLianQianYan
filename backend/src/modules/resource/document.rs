//! Complete textual document extraction for knowledge ingestion.
//! Upload and container limits are shared with previews; a document exceeding
//! the explicit text budget is rejected rather than indexed as a partial file.

use super::{parsers, validate_upload};

pub const MAX_DOCUMENT_CHARS: usize = 2_000_000;

pub fn extract_document_text(name: &str, mime: &str, bytes: &[u8]) -> Result<String, String> {
    let mime = validate_upload(name, mime, bytes.len(), 1)?;
    let text = match mime.as_str() {
        mime if mime.starts_with("text/") => parsers::text::parse(bytes)?,
        "application/pdf" => parsers::pdf::parse_with_limit(bytes, MAX_DOCUMENT_CHARS)?,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
            parsers::docx::parse_with_limit(bytes, MAX_DOCUMENT_CHARS)?
        }
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => {
            parsers::xlsx::parse_with_limit(bytes, MAX_DOCUMENT_CHARS)?
        }
        _ => return Err("此文件不提供可提取文本；图像 OCR 暂不支持".into()),
    };
    if text.chars().count() > MAX_DOCUMENT_CHARS {
        return Err(format!(
            "文档提取文本超过 {MAX_DOCUMENT_CHARS} 字符限制，未导入部分内容"
        ));
    }
    if text.trim().is_empty() {
        return Err(if mime == "application/pdf" {
            "PDF 无文本层；扫描件 OCR 暂不支持"
        } else {
            "文档未包含可提取文本"
        }
        .into());
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::resource::{extract_preview, MAX_PREVIEW_CHARS};
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

    fn pdf(content: &str) -> Vec<u8> {
        use pdf_extract::{dictionary, Document, Object, Stream};
        let mut document = Document::with_version("1.5");
        let pages = document.new_object_id();
        let font = document
            .add_object(dictionary! {"Type"=>"Font", "Subtype"=>"Type1", "BaseFont"=>"Helvetica"});
        let resources = document.add_object(dictionary! {"Font"=>dictionary!{"F1"=>font}});
        let stream = document.add_object(Stream::new(
            dictionary! {},
            format!("BT /F1 12 Tf 72 720 Td ({content}) Tj ET").into_bytes(),
        ));
        let page =
            document.add_object(dictionary! {"Type"=>"Page", "Parent"=>pages, "Contents"=>stream});
        document.objects.insert(pages, Object::Dictionary(dictionary!{"Type"=>"Pages", "Kids"=>vec![Object::Reference(page)], "Count"=>1, "Resources"=>resources, "MediaBox"=>vec![0.into(),0.into(),595.into(),842.into()]}));
        let catalog = document.add_object(dictionary! {"Type"=>"Catalog", "Pages"=>pages});
        document.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn knowledge_text_keeps_tail_beyond_preview_and_rejects_over_limit_or_nontext() {
        let text = format!("{}TAIL_MARKER", "涟".repeat(MAX_PREVIEW_CHARS + 100));
        for name in ["notes.txt", "notes.md", "table.csv", "source.rs"] {
            let full = extract_document_text(name, "text/plain", text.as_bytes()).unwrap();
            assert_eq!(full, text);
            assert!(full.ends_with("TAIL_MARKER"));
            let preview = extract_preview(name, "text/plain", text.as_bytes());
            assert!(preview.truncated);
            assert_eq!(preview.text.unwrap().chars().count(), MAX_PREVIEW_CHARS);
        }
        assert!(extract_document_text(
            "large.txt",
            "text/plain",
            "a".repeat(MAX_DOCUMENT_CHARS + 1).as_bytes()
        )
        .unwrap_err()
        .contains("未导入部分内容"));
        assert!(extract_document_text("bad.txt", "text/plain", &[255, 254]).is_err());
        assert!(extract_document_text("empty.txt", "text/plain", b" \n ").is_err());
        assert!(
            extract_document_text("scan.png", "image/png", b"image bytes")
                .unwrap_err()
                .contains("OCR")
        );
    }

    #[test]
    fn full_docx_and_pdf_keep_real_tail_without_changing_preview_budget() {
        let long = format!("{}TAIL_MARKER", "a".repeat(MAX_PREVIEW_CHARS + 100));
        let word = format!("<w:document xmlns:w=\"word\"><w:body><w:p><w:r><w:t>{long}</w:t></w:r></w:p></w:body></w:document>");
        let docx = archive(&[("word/document.xml", &word)]);
        let pdf_bytes = pdf(&long);
        for (name, mime, bytes) in [
            (
                "notes.docx",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                docx.as_slice(),
            ),
            ("notes.pdf", "application/pdf", pdf_bytes.as_slice()),
        ] {
            let full = extract_document_text(name, mime, bytes).unwrap();
            assert!(full.contains("TAIL_MARKER"), "{name} lost document tail");
            assert!(full.chars().count() > MAX_PREVIEW_CHARS);
            let preview = extract_preview(name, mime, bytes);
            assert!(preview.truncated, "{name}: {:?}", preview.error);
            assert!(!preview.text.unwrap().contains("TAIL_MARKER"));
        }
        assert!(
            extract_document_text("scan.pdf", "application/pdf", &pdf(""))
                .unwrap_err()
                .contains("无文本层")
        );
    }

    #[test]
    fn full_xlsx_keeps_cell_tail_and_preserves_worksheet_limits() {
        let long = format!("{}TAIL_MARKER", "a".repeat(MAX_PREVIEW_CHARS + 100));
        let sheet = format!("<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><dimension ref=\"A1:A1\"/><sheetData><row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>{long}</t></is></c></row></sheetData></worksheet>");
        let xlsx = archive(&[
            ("_rels/.rels", "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>"),
            ("[Content_Types].xml", "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/></Types>"),
            ("xl/workbook.xml", "<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets><sheet name=\"Data\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>"),
            ("xl/_rels/workbook.xml.rels", "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/></Relationships>"),
            ("xl/worksheets/sheet1.xml", &sheet),
        ]);
        let mime = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
        assert!(extract_document_text("sheet.xlsx", mime, &xlsx)
            .unwrap()
            .contains("TAIL_MARKER"));
        let preview = extract_preview("sheet.xlsx", mime, &xlsx);
        assert!(preview.truncated);
        assert!(!preview.text.unwrap().contains("TAIL_MARKER"));
        let dangerous = archive(&[(
            "xl/worksheets/sheet1.xml",
            "<worksheet><dimension ref=\"A1:XFD1048576\"/></worksheet>",
        )]);
        assert!(extract_document_text("dangerous.xlsx", mime, &dangerous).is_err());
    }
}
