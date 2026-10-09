//! Preview projection: bounded bytes in, `ResourcePreview` out.
//!
//! Dispatch is by canonical MIME rather than by the name the uploader typed.
//! Stored bytes are re-validated on the way out too — a record read back from
//! storage is not more trustworthy than the upload that produced it.

use crate::shared::resource::ResourceService;

use super::{
    limits::{validate_upload, MAX_PREVIEW_CHARS},
    model::ResourcePreview,
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
        parsers::image::dimensions(&mime, bytes).map(|(width, height)| {
            preview.width = Some(width);
            preview.height = Some(height);
            None
        })
    } else if mime == "application/pdf" {
        preview.kind = "pdf".into();
        match parsers::pdf::parse(bytes) {
            Ok(text) if text.trim().is_empty() => {
                preview.status = "unsupported".into();
                preview.error = Some("PDF 无文本层；扫描件 OCR 暂不支持".into());
                return preview;
            }
            other => other.map(Some),
        }
    } else if name.to_ascii_lowercase().ends_with(".docx") {
        preview.kind = "docx".into();
        parsers::docx::parse(bytes).map(Some)
    } else {
        preview.kind = "xlsx".into();
        parsers::xlsx::parse(bytes).map(Some)
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
        assert!(preview.text.unwrap().contains("Actual PDF text"));
        assert_eq!(
            extract_preview("scan.pdf", "application/pdf", &pdf(false)).status,
            "unsupported"
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
