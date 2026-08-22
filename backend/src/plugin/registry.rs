// ============================================================
// Plugin registry — scan, parse, validate, and expose declarative plugins.
//
// `refresh` only scans/parses/validates plugin.json files. It never executes
// any code, spawns any binary, or auto-connects any MCP template.
// ============================================================

use std::collections::HashMap;
use std::path::Path;

use parking_lot::RwLock;
use serde::Serialize;

use super::manifest::{parse_manifest, PluginId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginStatus {
    Enabled,
    Disabled,
    Misconfigured,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginRecord {
    pub id: PluginId,
    pub name: String,
    pub version: String,
    pub description: String,
    pub publisher: Option<String>,
    pub permissions: Vec<String>,
    pub contributes: crate::plugin::manifest::PluginContributions,
    pub status: PluginStatus,
    /// Plugin-relative display root (never an absolute sensitive path).
    pub root_display: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PluginRefreshReport {
    pub discovered: usize,
    pub invalid: usize,
    pub duplicates: usize,
}

pub struct PluginRegistry {
    records: RwLock<HashMap<PluginId, PluginRecord>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
        }
    }

    /// Scan `root` for `<plugin_id>/plugin.json` directories, parse + validate
    /// each, and atomically replace the registry. A duplicate id fails closed.
    pub fn refresh(&self, root: &Path) -> Result<PluginRefreshReport, String> {
        let mut next: HashMap<PluginId, PluginRecord> = HashMap::new();
        let mut report = PluginRefreshReport::default();

        let entries = std::fs::read_dir(root).map_err(|e| e.to_string())?;
        for entry in entries.flatten() {
            let plugin_dir = entry.path();
            if !plugin_dir.is_dir() {
                continue;
            }
            let manifest_path = plugin_dir.join("plugin.json");
            let Ok(raw) = std::fs::read_to_string(&manifest_path) else {
                continue;
            };
            report.discovered += 1;
            match parse_manifest(&raw) {
                Ok(manifest) => {
                    let id = match PluginId::new(manifest.id.clone()) {
                        Ok(id) => id,
                        Err(_) => {
                            report.invalid += 1;
                            continue;
                        }
                    };
                    if next.contains_key(&id) {
                        report.duplicates += 1;
                        continue;
                    }
                    next.insert(
                        id.clone(),
                        PluginRecord {
                            id: id.clone(),
                            name: manifest.name.clone(),
                            version: manifest.version.clone(),
                            description: manifest.description.clone(),
                            publisher: manifest.publisher.clone(),
                            permissions: manifest.permissions.clone(),
                            contributes: manifest.contributes.clone(),
                            status: PluginStatus::Disabled,
                            root_display: entry.file_name().to_string_lossy().to_string(),
                        },
                    );
                }
                Err(_) => {
                    report.invalid += 1;
                }
            }
        }

        *self.records.write() = next;
        Ok(report)
    }

    pub fn list(&self) -> Vec<PluginRecord> {
        let mut records: Vec<PluginRecord> = self.records.read().values().cloned().collect();
        records.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        records
    }

    pub fn get(&self, id: &PluginId) -> Option<PluginRecord> {
        self.records.read().get(id).cloned()
    }

    pub fn set_enabled(&self, id: &PluginId, enabled: bool) -> bool {
        let mut records = self.records.write();
        match records.get_mut(id) {
            Some(record) => {
                record.status = if enabled {
                    PluginStatus::Enabled
                } else {
                    PluginStatus::Disabled
                };
                true
            }
            None => false,
        }
    }
}
