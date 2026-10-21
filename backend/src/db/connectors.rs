use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

mod resources;
pub(crate) use resources::remove_connector_entity_on;
#[cfg(test)]
mod tests;

const SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(1021, "1021_connector_workbench", MigrationOwner::V1TaskWorld,
"CREATE TABLE connector_canvases (id TEXT PRIMARY KEY,name TEXT NOT NULL,agent_id TEXT NOT NULL REFERENCES agent_definitions(id),display_json TEXT NOT NULL,updated_at INTEGER NOT NULL);
CREATE TABLE connector_layouts (canvas_id TEXT PRIMARY KEY REFERENCES connector_canvases(id) ON DELETE CASCADE,layout_json TEXT NOT NULL);
CREATE TABLE connector_bindings (id TEXT PRIMARY KEY,agent_id TEXT NOT NULL REFERENCES agent_definitions(id),kind TEXT NOT NULL,source_id TEXT NOT NULL,target_id TEXT NOT NULL,service_kind TEXT NOT NULL DEFAULT '',model_source TEXT NOT NULL DEFAULT '',UNIQUE(agent_id,kind,source_id,target_id,service_kind));
CREATE UNIQUE INDEX connector_conversation_agent ON connector_bindings(source_id) WHERE kind='conversation_agent';
CREATE UNIQUE INDEX connector_agent_service ON connector_bindings(agent_id,service_kind) WHERE kind='agent_model';");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorBinding {
    #[serde(default)]
    pub id: String,
    pub kind: String,
    pub source_id: String,
    pub target_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_source: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorDisplay {
    pub mode: String,
    pub visible_kinds: Vec<String>,
    pub hidden_node_ids: Vec<String>,
    pub show_edges: bool,
    pub show_labels: bool,
    pub density: String,
}
impl Default for ConnectorDisplay {
    fn default() -> Self {
        Self {
            mode: "all".into(),
            visible_kinds: vec![],
            hidden_node_ids: vec![],
            show_edges: true,
            show_labels: true,
            density: "compact".into(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorPosition {
    pub id: String,
    pub x: f64,
    pub y: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorViewport {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorLayout {
    pub nodes: Vec<ConnectorPosition>,
    pub viewport: ConnectorViewport,
}
impl Default for ConnectorLayout {
    fn default() -> Self {
        Self {
            nodes: vec![],
            viewport: ConnectorViewport {
                x: 0.,
                y: 0.,
                zoom: 1.,
            },
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct ConnectorCanvas {
    pub id: String,
    pub name: String,
    pub agent_id: String,
    pub updated_at: i64,
}
#[derive(Debug, Clone, Serialize)]
pub struct ConnectorResource {
    pub id: String,
    pub kind: String,
    pub entity_id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_kind: Option<String>,
}
impl ConnectorResource {
    pub fn new(kind: &str, id: &str, label: &str) -> Self {
        Self {
            id: format!("{kind}:{id}"),
            kind: kind.into(),
            entity_id: id.into(),
            label: label.into(),
            service_kind: if kind == "model" {
                Some("chat".into())
            } else {
                None
            },
            model_source: if kind == "model" {
                Some("profile".into())
            } else {
                None
            },
        }
    }
}
/// Resolved identifiers reference the original module entities, never duplicated resources.
#[derive(Debug, Clone, Default)]
pub struct ConnectorRuntimeBindings {
    pub capabilities: Vec<(String, String)>,
    pub model_profile_id: Option<String>,
    pub model_pool_id: Option<String>,
    pub memories: Vec<super::Memory>,
    pub knowledge: Vec<super::Memory>,
    pub restrict_memory: bool,
    pub restrict_knowledge: bool,
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
fn binding_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConnectorBinding> {
    let service: String = row.get(4)?;
    let source: String = row.get(5)?;
    Ok(ConnectorBinding {
        id: row.get(0)?,
        kind: row.get(1)?,
        source_id: row.get(2)?,
        target_id: row.get(3)?,
        service_kind: if service.is_empty() {
            None
        } else {
            Some(service)
        },
        model_source: if source.is_empty() {
            None
        } else {
            Some(source)
        },
    })
}
impl Database {
    pub fn remove_connector_entity(&self, kind: &str, entity_id: &str) -> Result<(), String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(err)?;
        remove_connector_entity_on(&tx, kind, entity_id).map_err(err)?;
        tx.commit().map_err(err)
    }
    pub fn initialize_connectors_schema(&self) -> Result<(), rusqlite::Error> {
        self.apply_product_migration(&SCHEMA)
    }
    pub fn list_connector_canvases(
        &self,
        agent_id: Option<&str>,
    ) -> Result<Vec<ConnectorCanvas>, String> {
        let conn = self.conn();
        let mut stmt=conn.prepare("SELECT id,name,agent_id,updated_at FROM connector_canvases WHERE (?1 IS NULL OR agent_id=?1) ORDER BY updated_at,id").map_err(err)?;
        let rows = stmt
            .query_map([agent_id], |r| {
                Ok(ConnectorCanvas {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    agent_id: r.get(2)?,
                    updated_at: r.get(3)?,
                })
            })
            .map_err(err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }
    pub fn connector_canvas(
        &self,
        id: &str,
    ) -> Result<Option<(ConnectorCanvas, ConnectorDisplay, ConnectorLayout)>, String> {
        let conn = self.conn();
        let row=conn.query_row("SELECT c.id,c.name,c.agent_id,c.updated_at,c.display_json,l.layout_json FROM connector_canvases c LEFT JOIN connector_layouts l ON c.id=l.canvas_id WHERE c.id=?1",[id],|r|Ok((ConnectorCanvas{id:r.get(0)?,name:r.get(1)?,agent_id:r.get(2)?,updated_at:r.get(3)?},r.get::<_,String>(4)?,r.get::<_,Option<String>>(5)?))).optional().map_err(err)?;
        row.map(|(canvas, display, layout)| {
            Ok((
                canvas,
                serde_json::from_str(&display).map_err(err)?,
                match layout {
                    Some(v) => serde_json::from_str(&v).map_err(err)?,
                    None => ConnectorLayout::default(),
                },
            ))
        })
        .transpose()
    }
    pub fn create_connector_canvas(&self, name: &str, agent: &str) -> Result<String, String> {
        if name.trim().is_empty() {
            return Err("画布名称不能为空".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let conn = self.conn();
        if !conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM agent_definitions WHERE id=?1)",
                [agent],
                |r| r.get::<_, bool>(0),
            )
            .map_err(err)?
        {
            return Err("智能体不存在".into());
        }
        conn.execute(
            "INSERT INTO connector_canvases VALUES(?1,?2,?3,?4,?5)",
            params![
                id,
                name.trim(),
                agent,
                serde_json::to_string(&ConnectorDisplay::default()).map_err(err)?,
                chrono::Utc::now().timestamp_millis()
            ],
        )
        .map_err(err)?;
        Ok(id)
    }
    pub fn rename_connector_canvas(&self, id: &str, name: &str) -> Result<bool, String> {
        if name.trim().is_empty() {
            return Err("画布名称不能为空".into());
        }
        self.conn()
            .execute(
                "UPDATE connector_canvases SET name=?2,updated_at=?3 WHERE id=?1",
                params![id, name.trim(), chrono::Utc::now().timestamp_millis()],
            )
            .map(|n| n > 0)
            .map_err(err)
    }
    pub fn delete_connector_canvas(&self, id: &str) -> Result<bool, String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(err)?;
        super::settings_canvas::delete_settings_canvas_on(&tx, id).map_err(err)?;
        tx.execute("DELETE FROM connector_layouts WHERE canvas_id=?1", [id])
            .map_err(err)?;
        let n = tx
            .execute("DELETE FROM connector_canvases WHERE id=?1", [id])
            .map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(n > 0)
    }
    pub fn save_connector_layout(&self, id: &str, value: &ConnectorLayout) -> Result<(), String> {
        if !value.viewport.x.is_finite()
            || !value.viewport.y.is_finite()
            || !value.viewport.zoom.is_finite()
            || !(0.25..=2.).contains(&value.viewport.zoom)
            || value
                .nodes
                .iter()
                .any(|n| !n.x.is_finite() || !n.y.is_finite())
        {
            return Err("布局坐标或缩放无效".into());
        }
        self.conn().execute("INSERT INTO connector_layouts VALUES(?1,?2) ON CONFLICT(canvas_id) DO UPDATE SET layout_json=excluded.layout_json",params![id,serde_json::to_string(value).map_err(err)?]).map_err(err)?;
        Ok(())
    }
    pub fn save_connector_display(&self, id: &str, value: &ConnectorDisplay) -> Result<(), String> {
        if !["all", "conversations", "capabilities", "custom"].contains(&value.mode.as_str())
            || !["compact", "comfortable"].contains(&value.density.as_str())
        {
            return Err("显示设置无效".into());
        }
        self.conn()
            .execute(
                "UPDATE connector_canvases SET display_json=?2,updated_at=?3 WHERE id=?1",
                params![
                    id,
                    serde_json::to_string(value).map_err(err)?,
                    chrono::Utc::now().timestamp_millis()
                ],
            )
            .map_err(err)?;
        Ok(())
    }
    pub fn connector_runtime_bindings(
        &self,
        agent_id: &str,
    ) -> Result<ConnectorRuntimeBindings, String> {
        let mut resolved = ConnectorRuntimeBindings {
            restrict_memory: true,
            restrict_knowledge: true,
            ..Default::default()
        };
        for b in self.resolve_connector_bindings(agent_id)? {
            let (kind, id) = b.target_id.split_once(':').ok_or("连接器实体引用无效")?;
            match b.kind.as_str() {
                "agent_capability" => resolved.capabilities.push((kind.into(), id.into())),
                "agent_model" if b.service_kind.as_deref() == Some("chat") => {
                    match b.model_source.as_deref() {
                        Some("profile") => {
                            self.get_llm_model(id)?.ok_or("已绑定模型不存在")?;
                            resolved.model_profile_id = Some(id.into());
                        }
                        Some("pool") => {
                            self.get_model_pool(id)?.ok_or("已绑定模型池不存在")?;
                            resolved.model_pool_id = Some(id.into());
                        }
                        _ => return Err("模型来源无效".into()),
                    }
                }
                "agent_model" => {
                    let service = b.service_kind.as_deref().ok_or("绑定模型用途缺失")?;
                    match b.model_source.as_deref() {
                        Some("profile") => {
                            let profile = self
                                .get_basic_service_profile(id)?
                                .ok_or("绑定服务档案不存在")?;
                            if profile.service_kind != service {
                                return Err("绑定服务用途不匹配".into());
                            }
                        }
                        Some("pool") => {
                            let pool = self.get_model_pool(id)?.ok_or("绑定模型池不存在")?;
                            if !pool.enabled || pool.service_kind != service {
                                return Err("绑定模型池未启用或用途不匹配".into());
                            }
                        }
                        _ => return Err("绑定模型来源无效".into()),
                    }
                }
                "agent_memory" => {
                    resolved.restrict_memory = true;
                    resolved.memories.push(self.get_memory(id)?);
                }
                "agent_knowledge" => {
                    resolved.restrict_knowledge = true;
                    resolved.knowledge.push(self.get_memory(id)?);
                }
                _ => {}
            }
        }
        Ok(resolved)
    }
    pub fn resolve_connector_bindings(
        &self,
        agent_id: &str,
    ) -> Result<Vec<ConnectorBinding>, String> {
        let conn = self.conn();
        let mut stmt=conn.prepare("SELECT id,kind,source_id,target_id,service_kind,model_source FROM connector_bindings WHERE agent_id=?1 ORDER BY id").map_err(err)?;
        let rows = stmt.query_map([agent_id], binding_row).map_err(err)?;
        let mut bindings = rows.collect::<Result<Vec<_>, _>>().map_err(err)?;
        let pool: Option<String> = conn
            .query_row(
                "SELECT pool_id FROM model_pool_bindings WHERE subject_id=?1",
                [agent_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?;
        if let Some(id) = pool {
            bindings
                .retain(|b| b.kind != "agent_model" || b.service_kind.as_deref() != Some("chat"));
            bindings.push(ConnectorBinding {
                id: format!("pool:{agent_id}"),
                kind: "agent_model".into(),
                source_id: format!("agent:{agent_id}"),
                target_id: format!("model:{id}"),
                service_kind: Some("chat".into()),
                model_source: Some("pool".into()),
            });
        }
        let tools: Option<String> = conn
            .query_row(
                "SELECT allowed_tools FROM agent_definitions WHERE id=?1",
                [agent_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?;
        if let Some(raw) = tools {
            for tool in serde_json::from_str::<Vec<String>>(&raw).map_err(err)? {
                bindings.push(ConnectorBinding {
                    id: format!("tool:{agent_id}:{tool}"),
                    kind: "agent_capability".into(),
                    source_id: format!("agent:{agent_id}"),
                    target_id: format!("tool:{tool}"),
                    service_kind: None,
                    model_source: None,
                });
            }
        }
        Ok(bindings)
    }
    pub fn connector_agent_for_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<Option<String>, String> {
        self.conn().query_row("SELECT agent_id FROM connector_bindings WHERE kind='conversation_agent' AND source_id=?1",[format!("conversation:{conversation_id}")],|r|r.get(0)).optional().map_err(err)
    }
    pub fn save_connector_binding(
        &self,
        agent: &str,
        binding: &ConnectorBinding,
    ) -> Result<ConnectorBinding, String> {
        if binding.kind == "agent_model" && binding.model_source.as_deref() == Some("pool") {
            let id = binding
                .target_id
                .strip_prefix("model:")
                .ok_or("模型池引用无效")?;
            let pool = self.get_model_pool(id)?.ok_or("模型池不存在")?;
            if !pool.enabled || Some(pool.service_kind.as_str()) != binding.service_kind.as_deref()
            {
                return Err("模型池未启用或用途不匹配".into());
            }
        }
        if binding.kind == "agent_capability" && binding.target_id.starts_with("tool:") {
            let tool = binding.target_id.strip_prefix("tool:").unwrap();
            let mut conn = self.conn();
            let tx = conn.transaction().map_err(err)?;
            let raw: String = tx
                .query_row(
                    "SELECT allowed_tools FROM agent_definitions WHERE id=?1",
                    [agent],
                    |r| r.get(0),
                )
                .map_err(err)?;
            let mut tools: Vec<String> = serde_json::from_str(&raw).map_err(err)?;
            if !tools.iter().any(|t| t == tool) {
                tools.push(tool.into());
            }
            tx.execute(
                "UPDATE agent_definitions SET allowed_tools=?2 WHERE id=?1",
                params![agent, serde_json::to_string(&tools).map_err(err)?],
            )
            .map_err(err)?;
            tx.commit().map_err(err)?;
            let mut saved = binding.clone();
            saved.id = format!("tool:{agent}:{tool}");
            return Ok(saved);
        }
        if binding.kind == "agent_model"
            && binding.model_source.as_deref() == Some("pool")
            && binding.service_kind.as_deref() == Some("chat")
        {
            let id = binding
                .target_id
                .strip_prefix("model:")
                .ok_or("模型引用无效")?;
            let pool = self.get_model_pool(id)?.ok_or("模型池不存在")?;
            if !pool.enabled {
                return Err("模型池未启用".into());
            }
            let mut conn = self.conn();
            let tx = conn.transaction().map_err(err)?;
            tx.execute("DELETE FROM connector_bindings WHERE agent_id=?1 AND kind='agent_model' AND service_kind='chat'",[agent]).map_err(err)?;
            tx.execute("INSERT INTO model_pool_bindings(subject_id,pool_id) VALUES(?1,?2) ON CONFLICT(subject_id) DO UPDATE SET pool_id=excluded.pool_id",params![agent,id]).map_err(err)?;
            tx.commit().map_err(err)?;
            let mut saved = binding.clone();
            saved.id = format!("pool:{agent}");
            return Ok(saved);
        }
        if let Some(existing) = self
            .resolve_connector_bindings(agent)?
            .into_iter()
            .find(|b| {
                b.kind == binding.kind
                    && b.source_id == binding.source_id
                    && b.target_id == binding.target_id
                    && b.service_kind == binding.service_kind
                    && b.model_source == binding.model_source
            })
        {
            return Ok(existing);
        }
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(err)?;
        if binding.kind == "conversation_agent" {
            tx.execute(
                "DELETE FROM connector_bindings WHERE kind='conversation_agent' AND source_id=?1",
                [&binding.source_id],
            )
            .map_err(err)?;
        }
        if binding.kind == "agent_model" {
            if binding.service_kind.as_deref() == Some("chat") {
                tx.execute(
                    "DELETE FROM model_pool_bindings WHERE subject_id=?1",
                    [agent],
                )
                .map_err(err)?;
            }
            tx.execute("DELETE FROM connector_bindings WHERE kind='agent_model' AND agent_id=?1 AND service_kind=?2",params![agent,binding.service_kind.as_deref().unwrap_or("")]).map_err(err)?;
        }
        let mut saved = binding.clone();
        saved.id = uuid::Uuid::new_v4().to_string();
        tx.execute("INSERT INTO connector_bindings VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(agent_id,kind,source_id,target_id,service_kind) DO NOTHING",params![saved.id,agent,saved.kind,saved.source_id,saved.target_id,saved.service_kind.as_deref().unwrap_or(""),saved.model_source.as_deref().unwrap_or("")]).map_err(err)?;
        saved=tx.query_row("SELECT id,kind,source_id,target_id,service_kind,model_source FROM connector_bindings WHERE agent_id=?1 AND kind=?2 AND source_id=?3 AND target_id=?4 AND service_kind=?5",params![agent,saved.kind,saved.source_id,saved.target_id,saved.service_kind.as_deref().unwrap_or("")],binding_row).map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(saved)
    }
    pub fn delete_connector_binding(&self, agent: &str, id: &str) -> Result<bool, String> {
        if let Some(tool) = id.strip_prefix(&format!("tool:{agent}:")) {
            let mut conn = self.conn();
            let tx = conn.transaction().map_err(err)?;
            let raw: String = tx
                .query_row(
                    "SELECT allowed_tools FROM agent_definitions WHERE id=?1",
                    [agent],
                    |r| r.get(0),
                )
                .map_err(err)?;
            let mut tools: Vec<String> = serde_json::from_str(&raw).map_err(err)?;
            let before = tools.len();
            tools.retain(|t| t != tool);
            tx.execute(
                "UPDATE agent_definitions SET allowed_tools=?2 WHERE id=?1",
                params![agent, serde_json::to_string(&tools).map_err(err)?],
            )
            .map_err(err)?;
            tx.commit().map_err(err)?;
            return Ok(before != tools.len());
        }
        if id == format!("pool:{agent}") {
            return self
                .conn()
                .execute(
                    "DELETE FROM model_pool_bindings WHERE subject_id=?1",
                    [agent],
                )
                .map(|n| n > 0)
                .map_err(err);
        }
        self.conn()
            .execute(
                "DELETE FROM connector_bindings WHERE id=?1 AND agent_id=?2",
                params![id, agent],
            )
            .map(|n| n > 0)
            .map_err(err)
    }
}
