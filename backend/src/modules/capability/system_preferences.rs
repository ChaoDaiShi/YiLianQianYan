use crate::db::Database;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
const KEY: &str = "v1.system_preferences.v1";
const NAV: &[&str] = &[
    "tasks",
    "chat",
    "workflows",
    "workspaces",
    "skills",
    "plugins",
    "agents",
    "capabilities",
    "memory",
    "knowledge",
    "system",
    "logs",
    "settings",
];
const MODULES: &[&str] = &[
    "gateway",
    "approvals",
    "recovery",
    "capabilities",
    "monitoring",
    "workflows",
];
const CARDS: &[&str] = &[
    "health",
    "processes",
    "resources",
    "approvals",
    "diagnostics",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NavigationItem {
    pub id: String,
    pub visible: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MonitorCard {
    pub id: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MonitorLayout {
    pub mode: String,
    pub cards: Vec<MonitorCard>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub schema_version: u32,
    pub revision: u64,
    pub setup_completed: bool,
    pub navigation: Vec<NavigationItem>,
    pub enabled_modules: Vec<String>,
    pub monitor: MonitorLayout,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            setup_completed: false,
            navigation: NAV
                .iter()
                .map(|id| NavigationItem {
                    id: (*id).into(),
                    visible: true,
                })
                .collect(),
            enabled_modules: MODULES.iter().map(|id| (*id).into()).collect(),
            monitor: MonitorLayout {
                mode: "grid".into(),
                cards: CARDS
                    .iter()
                    .enumerate()
                    .map(|(i, id)| MonitorCard {
                        id: (*id).into(),
                        x: (i as u32 % 2) * 2,
                        y: i as u32 / 2,
                        width: 2,
                        height: 1,
                    })
                    .collect(),
            },
        }
    }
}
impl Preferences {
    pub fn validate(&self) -> Result<(), String> {
        use std::collections::HashSet;
        if self.schema_version != 1
            || self.revision > 9_007_199_254_740_990
            || self.navigation.len() != NAV.len()
            || self
                .navigation
                .iter()
                .map(|item| &item.id)
                .collect::<HashSet<_>>()
                .len()
                != NAV.len()
            || self.navigation.iter().any(|item| {
                !NAV.contains(&item.id.as_str())
                    || (["tasks", "chat", "settings"].contains(&item.id.as_str()) && !item.visible)
            })
            || self.enabled_modules.len() > MODULES.len()
            || self.enabled_modules.iter().collect::<HashSet<_>>().len()
                != self.enabled_modules.len()
            || self
                .enabled_modules
                .iter()
                .any(|id| !MODULES.contains(&id.as_str()))
            || ["gateway", "approvals", "recovery"]
                .iter()
                .any(|id| !self.enabled_modules.iter().any(|enabled| enabled == id))
            || !["grid", "free"].contains(&self.monitor.mode.as_str())
            || self.monitor.cards.len() > CARDS.len()
            || self
                .monitor
                .cards
                .iter()
                .map(|card| &card.id)
                .collect::<HashSet<_>>()
                .len()
                != self.monitor.cards.len()
            || self.monitor.cards.iter().any(|card| {
                !CARDS.contains(&card.id.as_str())
                    || card.x > 3
                    || card.y > 30
                    || !(1..=4).contains(&card.width)
                    || !(1..=3).contains(&card.height)
                    || card.x + card.width > 4
            })
        {
            return Err("invalid_or_unsafe_preferences".into());
        }
        Ok(())
    }
}

fn read(conn: &rusqlite::Connection) -> Result<Preferences, String> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key=?1", [KEY], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|_| "preferences_unavailable")?;
    Ok(raw
        .and_then(|value| serde_json::from_str::<Preferences>(&value).ok())
        .filter(|value| value.validate().is_ok())
        .unwrap_or_default())
}
pub fn load(db: &Database) -> Result<Preferences, String> {
    read(&db.conn())
}
pub fn save(db: &Database, mut next: Preferences) -> Result<Preferences, String> {
    next.validate()?;
    let mut conn = db.conn();
    let transaction = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|_| "preferences_unavailable")?;
    let current = read(&transaction)?;
    if current.revision != next.revision {
        return Err("stale_preferences_revision".into());
    }
    next.revision = next.revision.checked_add(1).ok_or("revision_overflow")?;
    transaction.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", rusqlite::params![KEY, serde_json::to_string(&next).map_err(|_| "invalid_preferences")?]).map_err(|_| "preferences_write_failed")?;
    transaction
        .commit()
        .map_err(|_| "preferences_write_failed")?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mandatory_modules_routes_and_layout_bounds_are_validated() {
        let mut hidden = Preferences::default();
        hidden.navigation[0].visible = false;
        assert!(hidden.validate().is_err());
        let mut disabled = Preferences::default();
        disabled.enabled_modules.clear();
        assert!(disabled.validate().is_err());
        let mut invalid = Preferences::default();
        invalid.monitor.cards[0].x = 10;
        assert!(invalid.validate().is_err());
        let mut duplicate = Preferences::default();
        duplicate.navigation[1] = duplicate.navigation[0].clone();
        assert!(duplicate.validate().is_err());
    }
    #[test]
    fn preferences_cancel_defaults_stale_write_and_invalid_storage_are_safe() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        assert!(!load(&db).unwrap().setup_completed);
        let initial = load(&db).unwrap();
        let mut chosen = initial.clone();
        chosen.setup_completed = true;
        let saved = save(&db, chosen).unwrap();
        assert_eq!(saved.revision, 1);
        assert!(save(&db, initial).is_err());
        assert_eq!(load(&db).unwrap(), saved);
        db.conn()
            .execute(
                "UPDATE settings SET value=?1 WHERE key=?2",
                rusqlite::params![r#"{"schema_version":99}"#, KEY],
            )
            .unwrap();
        assert_eq!(load(&db).unwrap(), Preferences::default());
    }
}
