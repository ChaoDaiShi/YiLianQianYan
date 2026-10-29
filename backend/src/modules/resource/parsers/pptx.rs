//! PresentationML text in presentation relationship order, including speaker notes.
use super::{archive::zip_entries, LimitedText};
use crate::modules::resource::limits::MAX_PREVIEW_CHARS;
use quick_xml::events::Event;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, PartialEq)]
pub(crate) struct SlideText {
    pub page: usize,
    pub part: String,
    pub text: String,
    pub notes: String,
}
#[derive(Debug)]
pub(super) struct Relationship {
    pub(super) target: String,
    pub(super) kind: String,
    pub(super) external: bool,
}

pub(super) fn elements(xml: &[u8], name: &[u8]) -> Result<Vec<HashMap<String, String>>, String> {
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut found = Vec::new();
    let mut depth = 0usize;
    loop {
        let event = reader.read_event().map_err(|_| "演示文稿XML无效")?;
        match &event {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.checked_sub(1).ok_or("演示文稿XML层级无效")?,
            Event::Eof if depth != 0 => return Err("演示文稿XML未闭合".into()),
            _ => {}
        }
        match event {
            Event::DocType(_) => return Err("不支持含DTD的演示文稿".into()),
            Event::Start(event) | Event::Empty(event) => {
                // Relationship IDs retain their namespace prefix.
                if event.local_name().as_ref() == name {
                    let mut attributes = HashMap::new();
                    for attribute in event.attributes() {
                        let attribute = attribute.map_err(|_| "演示文稿属性无效")?;
                        attributes.insert(
                            String::from_utf8(attribute.key.as_ref().to_vec())
                                .map_err(|_| "演示文稿属性编码无效")?,
                            attribute
                                .decode_and_unescape_value(reader.decoder())
                                .map_err(|_| "演示文稿属性编码无效")?
                                .into_owned(),
                        );
                    }
                    found.push(attributes);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(found)
}
pub(super) fn relationships(xml: &[u8]) -> Result<HashMap<String, Relationship>, String> {
    let mut map = HashMap::new();
    for attributes in elements(xml, b"Relationship")? {
        let id = attributes.get("Id").ok_or("演示文稿关系缺少ID")?.clone();
        let relationship = Relationship {
            target: attributes
                .get("Target")
                .ok_or("演示文稿关系缺少目标")?
                .clone(),
            kind: attributes
                .get("Type")
                .ok_or("演示文稿关系缺少类型")?
                .clone(),
            external: attributes
                .get("TargetMode")
                .is_some_and(|mode| mode == "External"),
        };
        if map.insert(id, relationship).is_some() {
            return Err("演示文稿关系ID重复".into());
        }
    }
    Ok(map)
}
pub(super) fn part_path(source: &str, target: &str) -> Result<String, String> {
    if target.contains('\\') || target.contains(':') || target.contains('#') || target.contains('?')
    {
        return Err("演示文稿部件路径无效".into());
    }
    let mut parts = if target.starts_with('/') {
        Vec::new()
    } else {
        source
            .rsplit_once('/')
            .map(|(parent, _)| parent.split('/').collect::<Vec<_>>())
            .unwrap_or_default()
    };
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err("演示文稿关系越出文档容器".into());
                }
            }
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        return Err("演示文稿部件路径为空".into());
    }
    Ok(parts.join("/"))
}
pub(super) fn rels_path(part: &str) -> String {
    let (parent, name) = part.rsplit_once('/').unwrap_or(("", part));
    if parent.is_empty() {
        format!("_rels/{name}.rels")
    } else {
        format!("{parent}/_rels/{name}.rels")
    }
}
fn drawing_text(xml: &[u8], limit: usize) -> Result<String, String> {
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut in_text = false;
    let mut text = LimitedText::new(limit);
    loop {
        match reader.read_event().map_err(|_| "幻灯片XML无效")? {
            Event::DocType(_) => return Err("不支持含DTD的幻灯片".into()),
            Event::Start(event) if event.local_name().as_ref() == b"t" => in_text = true,
            Event::End(event) => {
                if event.local_name().as_ref() == b"t" {
                    in_text = false;
                }
                if event.local_name().as_ref() == b"p" {
                    text.push("\n");
                }
            }
            Event::Empty(event) if event.local_name().as_ref() == b"br" => text.push("\n"),
            Event::Text(event) if in_text => {
                let decoded = event.decode().map_err(|_| "幻灯片文字编码无效")?;
                text.push(
                    &quick_xml::escape::unescape(&decoded).map_err(|_| "幻灯片文字实体无效")?,
                );
            }
            Event::GeneralRef(event) if in_text => {
                let reference = format!("&{};", event.decode().map_err(|_| "幻灯片实体编码无效")?);
                text.push(&quick_xml::escape::unescape(&reference).map_err(|_| "幻灯片实体无效")?);
            }
            Event::Eof => break,
            _ => {}
        }
        if text.overflow {
            break;
        }
    }
    Ok(text.value)
}
pub(crate) fn slides(bytes: &[u8], limit: usize) -> Result<Vec<SlideText>, String> {
    let entries = zip_entries(bytes)?;
    let root = relationships(entries.get("_rels/.rels").ok_or("缺少PPTX根关系")?)?;
    let main = root
        .values()
        .find(|relation| relation.kind.ends_with("/officeDocument"))
        .ok_or("缺少演示文稿主文档关系")?;
    if main.external {
        return Err("演示文稿不能使用外部主文档".into());
    }
    let presentation = part_path("", &main.target)?;
    let xml = entries.get(&presentation).ok_or("缺少演示文稿主文档")?;
    if elements(xml, b"presentation")?.len() != 1 {
        return Err("PPTX主部件不是演示文稿".into());
    }
    let rels = relationships(
        entries
            .get(&rels_path(&presentation))
            .ok_or("缺少演示文稿页面关系")?,
    )?;
    let ids = elements(xml, b"sldId")?;
    if ids.is_empty() {
        return Err("演示文稿没有幻灯片".into());
    }
    let mut result = Vec::new();
    for (index, id) in ids.into_iter().enumerate() {
        let relationship_id = id
            .iter()
            .find(|(key, _)| key.ends_with(":id"))
            .map(|(_, value)| value)
            .ok_or("幻灯片缺少关系ID")?;
        let relationship = rels.get(relationship_id).ok_or("幻灯片关系不存在")?;
        if relationship.external || !relationship.kind.ends_with("/slide") {
            return Err("幻灯片关系类型无效".into());
        }
        let part = part_path(&presentation, &relationship.target)?;
        let slide_xml = entries.get(&part).ok_or("幻灯片部件不存在")?;
        if elements(slide_xml, b"sld")?.len() != 1 {
            return Err("PPTX页面部件无效".into());
        }
        let text = drawing_text(slide_xml, limit)?;
        let notes = notes_text(&entries, &part, limit)?;
        result.push(SlideText {
            page: index + 1,
            part,
            text,
            notes,
        });
    }
    Ok(result)
}
fn notes_text(
    entries: &BTreeMap<String, Vec<u8>>,
    slide: &str,
    limit: usize,
) -> Result<String, String> {
    let Some(xml) = entries.get(&rels_path(slide)) else {
        return Ok(String::new());
    };
    let relationships = relationships(xml)?;
    let Some(notes) = relationships
        .values()
        .find(|relationship| relationship.kind.ends_with("/notesSlide"))
    else {
        return Ok(String::new());
    };
    if notes.external {
        return Err("不支持外部幻灯片备注".into());
    }
    let path = part_path(slide, &notes.target)?;
    drawing_text(entries.get(&path).ok_or("幻灯片备注部件不存在")?, limit)
}
pub(crate) fn parse(bytes: &[u8]) -> Result<String, String> {
    parse_with_limit(bytes, MAX_PREVIEW_CHARS)
}
pub(crate) fn parse_with_limit(bytes: &[u8], limit: usize) -> Result<String, String> {
    let slides = slides(bytes, limit)?;
    if slides
        .iter()
        .all(|slide| slide.text.trim().is_empty() && slide.notes.trim().is_empty())
    {
        return Ok(String::new());
    }
    let mut text = LimitedText::new(limit);
    for slide in slides {
        text.push(&format!("第 {} 页\n{}", slide.page, slide.text));
        if !slide.notes.trim().is_empty() {
            text.push(&format!("\n备注：\n{}", slide.notes));
        }
        text.push("\n");
        if text.overflow {
            break;
        }
    }
    Ok(text.value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    fn package(slide_target: &str) -> Vec<u8> {
        let root = r#"<Relationships><Relationship Id="main" Type="x/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#;
        let presentation = r#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="256" r:id="second-file"/><p:sldId id="257" r:id="first-file"/></p:sldIdLst></p:presentation>"#;
        let rels = format!(
            r#"<Relationships><Relationship Id="first-file" Type="x/slide" Target="slides/slide1.xml"/><Relationship Id="second-file" Type="x/slide" Target="{slide_target}"/></Relationships>"#
        );
        let entries = [
            ("_rels/.rels", root),
            ("ppt/presentation.xml", presentation),
            ("ppt/_rels/presentation.xml.rels", rels.as_str()),
            (
                "ppt/slides/slide1.xml",
                r#"<p:sld xmlns:p="p" xmlns:a="a"><a:p><a:r><a:t>后展示页面</a:t></a:r></a:p></p:sld>"#,
            ),
            (
                "ppt/slides/slide2.xml",
                r#"<p:sld xmlns:p="p" xmlns:a="a"><a:p><a:r><a:t>先展示 &amp; 真实文本</a:t></a:r></a:p></p:sld>"#,
            ),
            (
                "ppt/slides/_rels/slide2.xml.rels",
                r#"<Relationships><Relationship Id="note" Type="x/notesSlide" Target="../notesSlides/notesSlide2.xml"/></Relationships>"#,
            ),
            (
                "ppt/notesSlides/notesSlide2.xml",
                r#"<p:notes xmlns:p="p" xmlns:a="a"><a:p><a:r><a:t>讲者备注证据</a:t></a:r></a:p></p:notes>"#,
            ),
        ];
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, xml) in entries {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(xml.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
    #[test]
    fn presentation_relationships_define_page_order_and_notes_follow_the_slide() {
        let bytes = package("slides/slide2.xml");
        let result = slides(&bytes, 1000).unwrap();
        assert_eq!(result[0].part, "ppt/slides/slide2.xml");
        assert_eq!(result[0].page, 1);
        assert!(result[0].text.contains("先展示 & 真实文本"));
        assert!(result[0].notes.contains("讲者备注证据"));
        assert!(result[1].text.contains("后展示页面"));
        let full = parse_with_limit(&bytes, 1000).unwrap();
        assert!(full.find("先展示").unwrap() < full.find("后展示").unwrap());
        assert!(full.contains("第 2 页"));
        assert!(full.contains("备注"));
    }
    #[test]
    fn pptx_container_paths_and_text_budget_are_enforced() {
        assert!(parse(&package("../../outside.xml")).is_err());
        assert!(parse(&package("https://example.test/slide.xml")).is_err());
        assert_eq!(
            parse_with_limit(&package("slides/slide2.xml"), 5)
                .unwrap()
                .chars()
                .count(),
            6
        );
        assert!(elements(
            b"<!DOCTYPE p [<!ENTITY x SYSTEM 'file:///secret'>]><p/>",
            b"p"
        )
        .is_err());
        assert!(elements(b"<p><x/>", b"p").is_err());
    }
}
