use super::schema::{valid_id, Block, Envelope};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};

pub(super) fn ag_ui(raw: &str) -> Option<Envelope> {
    let mut blocks = Vec::new();
    for event in messages(raw)? {
        if event["type"] != "CUSTOM" || event["name"] != "yilian.rich_content" {
            continue;
        }
        let envelope: Envelope = serde_json::from_value(event.get("value")?.clone()).ok()?;
        if envelope.schema_version != 1 {
            return None;
        }
        blocks.extend(envelope.blocks);
    }
    Some(Envelope {
        schema_version: 1,
        blocks,
    })
}

const CATALOG_1: &str = "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json";
const CATALOG_09: &str = "https://a2ui.org/specification/v0_9/catalogs/basic/catalog.json";

#[derive(Default)]
struct Surface {
    model: Value,
    components: BTreeMap<String, Value>,
}

impl Surface {
    fn update(&mut self, components: &[Value]) -> Option<()> {
        for component in components {
            let id = component["id"].as_str().filter(|id| valid_id(id))?;
            if let Some(catalog) = component.get("@catalogId") {
                if !matches!(catalog.as_str(), Some(CATALOG_1 | CATALOG_09)) {
                    return None;
                }
            }
            self.components.insert(id.into(), component.clone());
        }
        Some(())
    }

    fn string<'a>(&'a self, value: &'a Value) -> Option<&'a str> {
        value.as_str().or_else(|| {
            let path = value.get("@path").or_else(|| value.get("path"))?.as_str()?;
            self.model.pointer(path)?.as_str()
        })
    }

    fn render(
        &self,
        id: &str,
        prefix: &str,
        seen: &mut HashSet<String>,
        output: &mut Vec<Block>,
    ) -> Option<()> {
        if !seen.insert(id.to_owned()) || seen.len() > 80 {
            return None;
        }
        let component = self.components.get(id)?;
        let block_id = format!("{prefix}-{id}");
        match component["component"].as_str()? {
            "Column" | "Row" | "List" => {
                for child in component["children"].as_array()? {
                    self.render(child.as_str()?, prefix, seen, output)?;
                }
            }
            "Card" => self.render(component["child"].as_str()?, prefix, seen, output)?,
            "Text" => output.push(Block::Text {
                id: block_id,
                text: self.string(&component["text"])?.to_owned(),
            }),
            "Divider" => output.push(Block::Text {
                id: block_id,
                text: "---".into(),
            }),
            "Image" => output.push(Block::Image {
                id: block_id,
                resource_id: self
                    .string(&component["url"])?
                    .strip_prefix("yilian-resource:")?
                    .to_owned(),
            }),
            "Button" => {
                if component["action"]["event"]["name"] != "yilian.acknowledge" {
                    return None;
                }
                let label = self.components.get(component["child"].as_str()?)?;
                if label["component"] != "Text" {
                    return None;
                }
                let action_label = self.string(&label["text"])?.to_owned();
                output.push(Block::InteractiveCard {
                    id: block_id,
                    title: action_label.clone(),
                    text: String::new(),
                    action_label,
                });
            }
            _ => return None,
        }
        Some(())
    }
}

pub(super) fn a2ui(raw: &str) -> Option<Envelope> {
    let mut surfaces = Vec::<(String, Surface)>::new();
    for event in messages(raw)? {
        if !matches!(event["version"].as_str(), Some("1.0" | "v0.9" | "v0.9.1")) {
            return None;
        }
        let event = event.as_object()?;
        if event.len() != 2 {
            return None;
        }
        let (kind, body) = event.iter().find(|(key, _)| *key != "version")?;
        let id = body["surfaceId"].as_str().filter(|id| valid_id(id))?;
        let existing = surfaces.iter().position(|(surface, _)| surface == id);
        match kind.as_str() {
            "createSurface" if existing.is_none() => {
                if let Some(catalog) = body.get("catalogId") {
                    if !matches!(catalog.as_str(), Some(CATALOG_1 | CATALOG_09)) {
                        return None;
                    }
                }
                if body["sendDataModel"] == true {
                    return None;
                }
                let mut surface = Surface {
                    model: body["dataModel"].clone(),
                    ..Surface::default()
                };
                if let Some(components) = body.get("components") {
                    surface.update(components.as_array()?)?;
                }
                surfaces.push((id.to_string(), surface));
            }
            "updateComponents" => surfaces[existing?]
                .1
                .update(body["components"].as_array()?)?,
            "updateDataModel" => {
                let surface = &mut surfaces[existing?].1;
                let value = body.get("value").cloned().unwrap_or(Value::Null);
                match body["path"].as_str() {
                    None | Some("/") => surface.model = value,
                    Some(path) => update_path(&mut surface.model, path, value)?,
                }
            }
            "deleteSurface" => {
                surfaces.remove(existing?);
            }
            _ => return None,
        }
    }
    let mut blocks = Vec::new();
    for (id, surface) in surfaces {
        surface.render(
            "root",
            &format!("a2-{}-{id}", id.len()),
            &mut HashSet::new(),
            &mut blocks,
        )?;
    }
    Some(Envelope {
        schema_version: 1,
        blocks,
    })
}

fn update_path(model: &mut Value, pointer: &str, value: Value) -> Option<()> {
    if !pointer.starts_with('/') {
        return None;
    }
    let (parent, key) = pointer.rsplit_once('/')?;
    let key = key.replace("~1", "/").replace("~0", "~");
    if model.is_null() && parent.is_empty() {
        *model = Value::Object(Map::new());
    }
    let parent = model.pointer_mut(parent)?;
    match parent {
        Value::Object(object) => {
            if value.is_null() {
                object.remove(&key);
            } else {
                object.insert(key, value);
            }
        }
        Value::Array(array) => {
            let index = key.parse::<usize>().ok()?;
            if value.is_null() && index < array.len() {
                array.remove(index);
            } else if index == array.len() {
                array.push(value);
            } else {
                *array.get_mut(index)? = value;
            }
        }
        _ => return None,
    }
    Some(())
}

fn messages(raw: &str) -> Option<Vec<Value>> {
    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Array(events)) => Some(events),
        Ok(event @ Value::Object(_)) => Some(vec![event]),
        Ok(_) => None,
        Err(_) => raw
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).ok())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ag_ui_custom_payload_normalizes_only_supported_rich_versions() {
        let fixture = r#"{"type":"CUSTOM","name":"yilian.rich_content","value":{"schema_version":1,"blocks":[{"id":"title","type":"text","text":"报表"}]}}"#;
        assert!(
            matches!(ag_ui(fixture).unwrap().blocks.as_slice(), [Block::Text {text,..}] if text == "报表")
        );
        assert!(ag_ui(&fixture.replace("\"schema_version\":1", "\"schema_version\":2")).is_none());
        assert!(ag_ui(&fixture.replace("\"CUSTOM\"", "\"TOOL_CALL_START\""))
            .unwrap()
            .blocks
            .is_empty());
    }

    #[test]
    fn a2ui_applies_versioned_surface_updates_before_projection() {
        let fixture = r#"[{"version":"1.0","createSurface":{"surfaceId":"report","dataModel":{"title":"旧稿"}}},
            {"version":"1.0","updateComponents":{"surfaceId":"report","components":[
                {"id":"root","component":"Column","children":["title"]},
                {"id":"title","component":"Text","text":{"@path":"/title"}}]}},
            {"version":"1.0","updateDataModel":{"surfaceId":"report","path":"/title","value":"新稿"}}]"#;
        assert!(
            matches!(a2ui(fixture).unwrap().blocks.as_slice(), [Block::Text {text,..}] if text == "新稿")
        );
        assert!(a2ui(&fixture.replace("\"1.0\"", "\"9.0\"")).is_none());
        assert!(a2ui(&fixture.replace("\"Text\"", "\"RunShell\"")).is_none());
        assert!(
            a2ui(&fixture.replace("\"children\":[\"title\"]", "\"children\":[\"root\"]")).is_none()
        );
        assert!(
            matches!(a2ui(&fixture.replace("\"1.0\"", "\"v0.9.1\"").replace("\"@path\"", "\"path\"")).unwrap().blocks.as_slice(), [Block::Text {text,..}] if text == "新稿")
        );
    }
}
