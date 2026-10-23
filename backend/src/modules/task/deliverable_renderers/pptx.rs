use super::{
    escape_xml, zip_package, DocumentContent, RenderError, RenderedDeliverable, PPTX_MIME,
};
const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const GROUP: &str = r#"<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>"#;

fn paragraph(text: &str, size: usize, color: &str) -> String {
    format!(
        r#"<a:p><a:pPr><a:lnSpc><a:spcPct val="110000"/></a:lnSpc><a:spcAft><a:spcPts val="400"/></a:spcAft></a:pPr><a:r><a:rPr lang="zh-CN" sz="{size}" dirty="0"><a:solidFill><a:srgbClr val="{color}"/></a:solidFill><a:latin typeface="Calibri"/><a:ea typeface="Microsoft YaHei"/></a:rPr><a:t xml:space="preserve">{}</a:t></a:r><a:endParaRPr lang="zh-CN" sz="{size}"/></a:p>"#,
        escape_xml(text)
    )
}
fn shape(
    id: usize,
    name: &str,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    paragraphs: &str,
) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{width}" cy="{height}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln><a:noFill/></a:ln></p:spPr><p:txBody><a:bodyPr wrap="square" lIns="0" tIns="0" rIns="0" bIns="0"><a:normAutofit/></a:bodyPr><a:lstStyle/>{paragraphs}</p:txBody></p:sp>"#
    )
}
fn slide(title: &str, lines: &[String], page: usize) -> String {
    let title = shape(
        2,
        "Editable title",
        600000,
        450000,
        10992000,
        1000000,
        &paragraph(title, 3000, "7050B3"),
    );
    let body_text = if lines.is_empty() {
        paragraph("", 2200, "302D42")
    } else {
        lines
            .iter()
            .map(|line| paragraph(line, 2200, "302D42"))
            .collect::<String>()
    };
    let body = shape(
        3,
        "Editable body",
        600000,
        1600000,
        10992000,
        4500000,
        &body_text,
    );
    let footer = shape(
        4,
        "Page number",
        11000000,
        6350000,
        600000,
        300000,
        &paragraph(&page.to_string(), 1200, "706A83"),
    );
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld xmlns:p="{P}" xmlns:a="{A}" xmlns:r="{R}"><p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val="FBFAFF"/></a:solidFill><a:effectLst/></p:bgPr></p:bg><p:spTree>{GROUP}{title}{body}{footer}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#
    )
}
fn wrap(text: &str) -> Vec<String> {
    let mut output = Vec::new();
    for line in text.lines() {
        let chars = line.chars().collect::<Vec<_>>();
        if chars.is_empty() {
            output.push(String::new());
        } else {
            for part in chars.chunks(36) {
                output.push(part.iter().collect());
            }
        }
    }
    output
}
fn relationships(items: &[(String, String, String)]) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{REL}">{}</Relationships>"#,
        items
            .iter()
            .map(|(id, kind, target)| format!(
                r#"<Relationship Id="{id}" Type="{R}/{kind}" Target="{target}"/>"#
            ))
            .collect::<String>()
    )
}

pub fn render_pptx(
    file_name: &str,
    document: &DocumentContent,
) -> Result<RenderedDeliverable, RenderError> {
    if document.title.trim().is_empty() || document.title.chars().count() > 70 {
        return Err(RenderError::InvalidPayload(
            "演示文稿标题须为1-70个字符".into(),
        ));
    }
    let mut slides = vec![(document.title.clone(), Vec::<String>::new())];
    for section in &document.sections {
        if section.heading.chars().count() > 70 {
            return Err(RenderError::InvalidPayload(
                "演示文稿章节标题不能超过70个字符".into(),
            ));
        }
        let mut lines = section
            .paragraphs
            .iter()
            .flat_map(|paragraph| wrap(paragraph))
            .collect::<Vec<_>>();
        if let Some(table) = &section.table {
            for row in table {
                lines.extend(wrap(&row.join(" | ")));
            }
        }
        if lines.is_empty() {
            slides.push((section.heading.clone(), vec![]));
        } else {
            for (index, part) in lines.chunks(10).enumerate() {
                slides.push((
                    if index == 0 {
                        section.heading.clone()
                    } else {
                        format!("{}（续{}）", section.heading, index + 1)
                    },
                    part.to_vec(),
                ));
            }
        }
    }
    let source_lines = document
        .sources
        .iter()
        .flat_map(|source| wrap(source))
        .collect::<Vec<_>>();
    for (index, part) in source_lines.chunks(10).enumerate() {
        slides.push((
            if index == 0 {
                "资料来源".into()
            } else {
                format!("资料来源（续{}）", index + 1)
            },
            part.to_vec(),
        ));
    }
    if slides.len() > 100 {
        return Err(RenderError::InvalidPayload(
            "演示文稿超过100页，请减少内容或拆分导出".into(),
        ));
    }
    let invalid_xml = |text: &str| {
        text.chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    };
    if invalid_xml(&document.title)
        || document.sources.iter().any(|text| invalid_xml(text))
        || document.sections.iter().any(|section| {
            invalid_xml(&section.heading)
                || section.paragraphs.iter().any(|text| invalid_xml(text))
                || section
                    .table
                    .iter()
                    .flatten()
                    .flatten()
                    .any(|text| invalid_xml(text))
        })
    {
        return Err(RenderError::InvalidPayload(
            "演示文稿内容包含XML不支持的控制字符".into(),
        ));
    }
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let overrides=slides.iter().enumerate().map(|(index,_)|format!(r#"<Override PartName="/ppt/slides/slide{}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#,index+1)).collect::<String>();
    files.push(("[Content_Types].xml".into(),format!(r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>{overrides}</Types>"#).into_bytes()));
    files.push((
        "_rels/.rels".into(),
        relationships(&[(
            "rId1".into(),
            "officeDocument".into(),
            "ppt/presentation.xml".into(),
        )])
        .into_bytes(),
    ));
    let ids = slides
        .iter()
        .enumerate()
        .map(|(index, _)| format!(r#"<p:sldId id="{}" r:id="rId{}"/>"#, 256 + index, index + 2))
        .collect::<String>();
    files.push(("ppt/presentation.xml".into(),format!(r#"<?xml version="1.0" encoding="UTF-8"?><p:presentation xmlns:p="{P}" xmlns:a="{A}" xmlns:r="{R}"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx="12192000" cy="6858000" type="screen16x9"/><p:notesSz cx="6858000" cy="9144000"/><p:defaultTextStyle><a:defPPr><a:defRPr lang="zh-CN"/></a:defPPr></p:defaultTextStyle></p:presentation>"#).into_bytes()));
    let mut rels = vec![(
        "rId1".into(),
        "slideMaster".into(),
        "slideMasters/slideMaster1.xml".into(),
    )];
    for (index, (title, lines)) in slides.iter().enumerate() {
        rels.push((
            format!("rId{}", index + 2),
            "slide".into(),
            format!("slides/slide{}.xml", index + 1),
        ));
        files.push((
            format!("ppt/slides/slide{}.xml", index + 1),
            slide(title, lines, index + 1).into_bytes(),
        ));
        files.push((
            format!("ppt/slides/_rels/slide{}.xml.rels", index + 1),
            relationships(&[(
                "rId1".into(),
                "slideLayout".into(),
                "../slideLayouts/slideLayout1.xml".into(),
            )])
            .into_bytes(),
        ));
    }
    files.push((
        "ppt/_rels/presentation.xml.rels".into(),
        relationships(&rels).into_bytes(),
    ));
    let mapping = r#"<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>"#;
    files.push(("ppt/slideMasters/slideMaster1.xml".into(),format!(r#"<?xml version="1.0" encoding="UTF-8"?><p:sldMaster xmlns:p="{P}" xmlns:a="{A}" xmlns:r="{R}"><p:cSld><p:spTree>{GROUP}</p:spTree></p:cSld>{mapping}<p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles></p:sldMaster>"#).into_bytes()));
    files.push((
        "ppt/slideMasters/_rels/slideMaster1.xml.rels".into(),
        relationships(&[
            (
                "rId1".into(),
                "slideLayout".into(),
                "../slideLayouts/slideLayout1.xml".into(),
            ),
            ("rId2".into(), "theme".into(), "../theme/theme1.xml".into()),
        ])
        .into_bytes(),
    ));
    files.push(("ppt/slideLayouts/slideLayout1.xml".into(),format!(r#"<?xml version="1.0" encoding="UTF-8"?><p:sldLayout xmlns:p="{P}" xmlns:a="{A}" xmlns:r="{R}" type="blank" preserve="1"><p:cSld name="Editable document"><p:spTree>{GROUP}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#).into_bytes()));
    files.push((
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels".into(),
        relationships(&[(
            "rId1".into(),
            "slideMaster".into(),
            "../slideMasters/slideMaster1.xml".into(),
        )])
        .into_bytes(),
    ));
    files.push(("ppt/theme/theme1.xml".into(), theme().into_bytes()));
    let bytes = zip_package(
        files
            .iter()
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice())),
    )?;
    Ok(RenderedDeliverable {
        file_name: file_name.into(),
        mime_type: PPTX_MIME.into(),
        bytes,
    })
}

fn theme() -> String {
    let colors = [
        ("dk1", "302D42"),
        ("lt1", "FFFFFF"),
        ("dk2", "51486B"),
        ("lt2", "FBFAFF"),
        ("accent1", "7050B3"),
        ("accent2", "C58DB5"),
        ("accent3", "7CA7A2"),
        ("accent4", "D2AE73"),
        ("accent5", "8A9FD1"),
        ("accent6", "B6A3D6"),
        ("hlink", "7050B3"),
        ("folHlink", "9B6EA3"),
    ]
    .iter()
    .map(|(name, value)| format!(r#"<a:{name}><a:srgbClr val="{value}"/></a:{name}>"#))
    .collect::<String>();
    let fill = r#"<a:solidFill><a:schemeClr val="phClr"/></a:solidFill>"#;
    let lines=[6350,12700,19050].iter().map(|width|format!(r#"<a:ln w="{width}" cap="flat" cmpd="sng" algn="ctr">{fill}<a:prstDash val="solid"/><a:miter lim="800000"/></a:ln>"#)).collect::<String>();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><a:theme xmlns:a="{A}" name="YiLian"><a:themeElements><a:clrScheme name="YiLian">{colors}</a:clrScheme><a:fontScheme name="Readable"><a:majorFont><a:latin typeface="Calibri"/><a:ea typeface="Microsoft YaHei"/><a:cs typeface="Arial"/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface="Microsoft YaHei"/><a:cs typeface="Arial"/></a:minorFont></a:fontScheme><a:fmtScheme name="Simple"><a:fillStyleLst>{fill}{fill}{fill}</a:fillStyleLst><a:lnStyleLst>{lines}</a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst>{fill}{fill}{fill}</a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>"#
    )
}
