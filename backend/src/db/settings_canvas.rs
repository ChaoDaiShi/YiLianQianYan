//! Settings canvas presentation only; actual configuration and resource bindings keep their owners.
use super::Database;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsCanvasPosition {
    pub id: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsCanvasViewport {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsCanvasState {
    pub connected_node_ids: Vec<String>,
    #[serde(default)]
    pub expanded_group_ids: Vec<String>,
    pub nodes: Vec<SettingsCanvasPosition>,
    pub viewport: SettingsCanvasViewport,
}

impl Default for SettingsCanvasState {
    fn default() -> Self {
        Self {
            expanded_group_ids: vec![],
            connected_node_ids: settings_canvas_presets()
                .expect("embedded settings catalog")
                .into_iter()
                .map(|preset| preset.id)
                .collect(),
            nodes: Vec::new(),
            viewport: SettingsCanvasViewport {
                x: 0.,
                y: 0.,
                zoom: 1.,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsCanvasPreset {
    pub id: String,
    pub label: String,
    pub description: String,
    pub group: String,
    pub side: String,
}

pub fn settings_canvas_presets() -> Result<Vec<SettingsCanvasPreset>, String> {
    serde_json::from_str(include_str!(
        "../../../frontend/src/features/settings/canvas/settingPresets.json"
    ))
    .map_err(|e| e.to_string())
}

impl SettingsCanvasState {
    pub fn validate(&self) -> Result<(), String> {
        let known: HashSet<String> = settings_canvas_presets()?
            .into_iter()
            .map(|preset| preset.id)
            .collect();
        let mut connected = HashSet::new();
        let group_ids = [
            "models",
            "voice",
            "agent",
            "security",
            "interface",
            "extensions",
        ];
        let mut expanded = HashSet::new();
        for id in &self.expanded_group_ids {
            if !group_ids.contains(&id.as_str()) || !expanded.insert(id) {
                return Err("设置分类不存在或展开重复".into());
            }
        }
        for id in &self.connected_node_ids {
            if !(known.contains(id) || plugin_node_id(id)) || !connected.insert(id) {
                return Err("设置节点不存在或连接重复".into());
            }
        }
        let mut positions = HashSet::new();
        for node in &self.nodes {
            if !(known.contains(&node.id)
                || plugin_node_id(&node.id)
                || node
                    .id
                    .strip_prefix("group:")
                    .is_some_and(|id| group_ids.contains(&id)))
                || !positions.insert(&node.id)
            {
                return Err("设置节点不存在或布局重复".into());
            }
            if !node.x.is_finite() || !node.y.is_finite() {
                return Err("设置节点坐标无效".into());
            }
        }
        if !self.viewport.x.is_finite()
            || !self.viewport.y.is_finite()
            || !self.viewport.zoom.is_finite()
            || !(0.25..=2.).contains(&self.viewport.zoom)
        {
            return Err("设置画布坐标或缩放无效".into());
        }
        Ok(())
    }
}

fn plugin_node_id(id: &str) -> bool {
    let parts: Vec<_> = id.split(':').collect();
    parts.len() == 3
        && parts[0] == "plugin"
        && crate::plugin::PluginId::new(parts[1]).is_ok()
        && crate::plugin::PluginId::new(parts[2]).is_ok()
}

fn key(canvas_id: Option<&str>) -> String {
    match canvas_id {
        Some(id) => format!("settings_canvas:canvas:{id}"),
        None => "settings_canvas:global".into(),
    }
}

fn scope_exists(conn: &Connection, canvas_id: Option<&str>) -> Result<bool, String> {
    match canvas_id {
        None => Ok(true),
        Some(id) => conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM connector_canvases WHERE id=?1)",
                [id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string()),
    }
}

/// Called inside connector canvas deletion's existing transaction.
pub(crate) fn delete_settings_canvas_on(
    conn: &Connection,
    canvas_id: &str,
) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM settings WHERE key=?1", [key(Some(canvas_id))])?;
    Ok(())
}

impl Database {
    /// Missing state returns a default without inserting anything; missing canvas returns None.
    pub fn settings_canvas_state(
        &self,
        canvas_id: Option<&str>,
    ) -> Result<Option<SettingsCanvasState>, String> {
        let conn = self.conn();
        if !scope_exists(&conn, canvas_id)? {
            return Ok(None);
        }
        let stored: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key=?1",
                [key(canvas_id)],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let state = match stored {
            Some(json) => {
                serde_json::from_str::<SettingsCanvasState>(&json).map_err(|e| e.to_string())?
            }
            None => SettingsCanvasState::default(),
        };
        state.validate()?;
        Ok(Some(state))
    }

    pub fn save_settings_canvas_state(
        &self,
        canvas_id: Option<&str>,
        state: &SettingsCanvasState,
    ) -> Result<bool, String> {
        state.validate()?;
        let json = serde_json::to_string(state).map_err(|e| e.to_string())?;
        let conn = self.conn();
        if !scope_exists(&conn, canvas_id)? {
            return Ok(false);
        }
        conn.execute(
            "INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key(canvas_id), json],
        )
        .map_err(|e| e.to_string())?;
        Ok(true)
    }
}
