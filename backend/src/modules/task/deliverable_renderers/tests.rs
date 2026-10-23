use super::*;
use calamine::{Data, Reader};
use std::io::{Cursor, Read};

#[test]
fn pptx_contains_editable_ooxml_slides_master_layout_theme_and_roundtrips_text() {
    let document = DocumentContent {
        title: "中文演示".into(),
        sections: vec![DocumentSection {
            heading: "关键结果".into(),
            paragraphs: vec!["原始证据 & 结果".into()],
            table: Some(vec![vec!["指标".into(), "7".into()]]),
        }],
        sources: vec!["source-marker".into()],
    };
    let rendered = render_pptx("presentation.pptx", &document).unwrap();
    assert_eq!(rendered.mime_type, PPTX_MIME);
    assert!(rendered.bytes.starts_with(b"PK"));
    let mut package = zip::ZipArchive::new(Cursor::new(&rendered.bytes)).unwrap();
    for part in [
        "[Content_Types].xml",
        "_rels/.rels",
        "ppt/presentation.xml",
        "ppt/_rels/presentation.xml.rels",
        "ppt/theme/theme1.xml",
        "ppt/slideMasters/slideMaster1.xml",
        "ppt/slideMasters/_rels/slideMaster1.xml.rels",
        "ppt/slideLayouts/slideLayout1.xml",
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
        "ppt/slides/_rels/slide2.xml.rels",
    ] {
        assert!(package.by_name(part).is_ok(), "missing {part}");
    }
    let mut slide = String::new();
    package
        .by_name("ppt/slides/slide2.xml")
        .unwrap()
        .read_to_string(&mut slide)
        .unwrap();
    assert!(slide.contains("Editable title"));
    assert!(slide.contains("Editable body"));
    assert!(slide.contains("<a:t"));
    assert!(slide.contains("&amp;"));
    assert!(!slide.contains("<p:pic"));
    let text = crate::modules::resource::extract_document_text(
        "presentation.pptx",
        PPTX_MIME,
        &rendered.bytes,
    )
    .unwrap();
    assert!(text.contains("中文演示"));
    assert!(text.contains("原始证据 & 结果"));
    assert!(text.contains("指标 | 7"));
    assert!(text.contains("source-marker"));
    let inspection =
        crate::modules::task::proofloop::deliverable_format::inspect_deliverable_format(
            PPTX_MIME,
            "presentation.pptx",
            &rendered.bytes,
        )
        .unwrap();
    assert!(inspection.extracted_text.unwrap().contains("关键结果"));
}

#[test]
fn pptx_format_contract_preserves_extension_and_paginates_long_body() {
    assert_eq!(
        DeliverableFormat::try_from("pptx").unwrap(),
        DeliverableFormat::Pptx
    );
    assert_eq!(
        file_name_for_format("report", DeliverableFormat::Pptx).unwrap(),
        "report.pptx"
    );
    assert!(file_name_for_format("report.pdf", DeliverableFormat::Pptx).is_err());
    let document = DocumentContent {
        title: "长篇正文".into(),
        sections: vec![DocumentSection {
            heading: "内容".into(),
            paragraphs: (0..23).map(|n| format!("独立段落{n}")).collect(),
            table: None,
        }],
        sources: vec![],
    };
    let rendered = render_pptx("long.pptx", &document).unwrap();
    let slides = crate::modules::resource::parsers::pptx::slides(&rendered.bytes, 20000).unwrap();
    assert_eq!(slides.len(), 4);
    assert!(slides[3].text.contains("独立段落22"));
}

#[test]
fn markdown_and_json_keep_the_structured_source_bytes() {
    let markdown = render_markdown("brief.md", "# 来源内容\n\n真实正文。".into());
    assert_eq!(markdown.mime_type, "text/markdown");
    assert_eq!(markdown.bytes, "# 来源内容\n\n真实正文。".as_bytes());

    let value = serde_json::json!({"report":{"title":"来源报告","count":3}});
    let rendered = render_json("report.json", &value).unwrap();
    assert_eq!(rendered.mime_type, "application/json");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&rendered.bytes).unwrap(),
        value
    );
}

#[test]
fn docx_is_an_ooxml_package_with_source_text_and_table_cells() {
    let document = DocumentContent {
        title: "实测报告".into(),
        sections: vec![DocumentSection {
            heading: "结果".into(),
            paragraphs: vec!["来自已完成执行的正文".into()],
            table: Some(vec![
                vec!["指标".into(), "值".into()],
                vec!["样本".into(), "7".into()],
            ]),
        }],
        sources: vec!["https://example.test/source".into()],
    };
    let rendered = render_docx("实测.docx", &document).unwrap();
    assert_eq!(rendered.mime_type, DOCX_MIME);

    let mut archive = zip::ZipArchive::new(Cursor::new(rendered.bytes)).unwrap();
    let mut document_xml = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut document_xml)
        .unwrap();
    assert!(document_xml.contains("实测报告"));
    assert!(document_xml.contains("来自已完成执行的正文"));
    assert!(document_xml.contains("7"));
    assert!(document_xml.contains("<w:tbl>"));
    for entry in [
        "[Content_Types].xml",
        "_rels/.rels",
        "word/_rels/document.xml.rels",
        "word/styles.xml",
    ] {
        assert!(archive.by_name(entry).is_ok());
    }
}

#[test]
fn xlsx_roundtrips_headers_strings_and_numeric_values() {
    let sheet = SheetContent {
        name: "销售数据".into(),
        headers: vec!["地区".into(), "数量".into()],
        rows: vec![vec![serde_json::json!("成都"), serde_json::json!(12)]],
    };
    let rendered = render_xlsx("sales.xlsx", &sheet).unwrap();
    assert_eq!(rendered.mime_type, XLSX_MIME);

    let mut workbook: calamine::Xlsx<_> =
        calamine::open_workbook_from_rs(Cursor::new(rendered.bytes)).unwrap();
    let range = workbook.worksheet_range("销售数据").unwrap();
    assert_eq!(range.get_value((0, 0)), Some(&Data::String("地区".into())));
    assert_eq!(range.get_value((1, 0)), Some(&Data::String("成都".into())));
    assert_eq!(range.get_value((1, 1)).unwrap().to_string(), "12");
}

#[test]
fn png_decodes_to_the_declared_chart_dimensions_and_contains_bars() {
    let chart = ChartContent {
        title: Some("按地区统计".into()),
        categories: vec!["A".into(), "B".into(), "C".into()],
        values: vec![2.0, 8.0, 4.0],
    };
    let rendered = render_png("chart.png", &chart).unwrap();
    assert_eq!(rendered.mime_type, "image/png");
    let image = image::load_from_memory(&rendered.bytes).unwrap().to_rgb8();
    assert_eq!((image.width(), image.height()), (PNG_WIDTH, PNG_HEIGHT));
    assert!(image.pixels().any(|pixel| pixel.0 == [36, 133, 151]));
}

#[test]
fn pdf_without_an_approved_renderer_hook_is_explicitly_unavailable() {
    let document = DocumentContent {
        title: "PDF".into(),
        sections: vec![],
        sources: vec![],
    };
    assert!(matches!(
        render_pdf("report.pdf", &document),
        Err(RenderError::PdfRendererIntegrationUnavailable(_))
    ));
}
