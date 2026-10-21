//! Settings declared by installed, enabled plugins; no package scripts are executed.
use crate::{app::state::AppServer, db::Database, plugin::settings::PluginSetting};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};

type Error = (StatusCode, Json<Value>);
fn error(status: StatusCode, message: impl Into<String>) -> Error {
    (status, Json(json!({"error": message.into()})))
}
#[derive(Clone, Serialize)]
pub struct Entry {
    pub node_id: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub setting: PluginSetting,
}
pub fn entries(db: &Database) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    for record in crate::modules::capability::import_store::list(db)?
        .into_iter()
        .filter(|record| record.installed && record.enabled && record.package.kind == "plugin")
    {
        let manifest = crate::plugin::parse_manifest(
            record
                .package
                .files
                .get("plugin.json")
                .ok_or("插件清单不存在")?,
        )
        .map_err(|e| e.to_string())?;
        for setting in manifest.contributes.settings {
            entries.push(Entry {
                node_id: format!("plugin:{}:{}", record.package.id, setting.id),
                plugin_id: record.package.id.clone(),
                plugin_name: record.package.name.clone(),
                setting,
            });
        }
    }
    Ok(entries)
}
pub fn presets(
    db: &Database,
) -> Result<Vec<crate::db::settings_canvas::SettingsCanvasPreset>, String> {
    Ok(entries(db)?
        .into_iter()
        .map(|entry| crate::db::settings_canvas::SettingsCanvasPreset {
            id: entry.node_id,
            label: format!("{} · {}", entry.plugin_name, entry.setting.title),
            description: entry.setting.description,
            group: "plugins".into(),
            side: "right".into(),
        })
        .collect())
}
fn find(db: &Database, plugin: &str, id: &str) -> Result<Entry, Error> {
    entries(db)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .into_iter()
        .find(|entry| entry.plugin_id == plugin && entry.setting.id == id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "插件设置不存在或插件未启用"))
}
fn key(plugin: &str, id: &str) -> String {
    format!("plugin_config:{plugin}:{id}")
}
pub fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route("/api/plugins/settings", get(list))
        .route("/api/plugins/:plugin/settings/:id", get(load).put(save))
}
async fn list(State(server): State<Arc<AppServer>>) -> Result<Json<Value>, Error> {
    Ok(Json(
        json!({"settings": entries(&server.db).map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?}),
    ))
}
async fn load(
    State(server): State<Arc<AppServer>>,
    Path((plugin, id)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    let entry = find(&server.db, &plugin, &id)?;
    let stored: Option<String> = server
        .db
        .conn()
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [key(&plugin, &id)],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let values = match stored {
        Some(raw) => serde_json::from_str::<BTreeMap<String, Value>>(&raw)
            .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
        None => entry.setting.defaults(),
    };
    entry
        .setting
        .validate_values(&values)
        .map_err(|e| error(StatusCode::CONFLICT, e))?;
    Ok(Json(json!({"entry":entry,"values":values})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    values: BTreeMap<String, Value>,
}
async fn save(
    State(server): State<Arc<AppServer>>,
    Path((plugin, id)): Path<(String, String)>,
    Json(update): Json<Update>,
) -> Result<Json<Value>, Error> {
    let entry = find(&server.db, &plugin, &id)?;
    entry
        .setting
        .validate_values(&update.values)
        .map_err(|e| error(StatusCode::BAD_REQUEST, e))?;
    server.db.conn().execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        rusqlite::params![key(&plugin,&id),serde_json::to_string(&update.values).map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR,e.to_string()))?])
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR,e.to_string()))?;
    Ok(Json(json!({"entry":entry,"values":update.values})))
}

#[cfg(test)]
mod tests;
