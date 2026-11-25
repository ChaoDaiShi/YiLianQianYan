//! User-selected runtime composition. The execution gateway still authorizes each action.
use crate::{
    app::state::AppServer,
    db::Database,
    execution::context_manifest::{ContextManifest, ContextStatus},
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

const KEY: &str = "harness_profiles";
#[cfg(test)]
mod tests;
pub const DEFAULT_ID: &str = "default";
const SYSTEM_GROUPS: &[(&str, &str)] = &[
    ("system-all", "全部工具"),
    ("system-desktop", "操作电脑"),
    ("system-browser", "浏览网页"),
    ("system-files", "管理文件"),
    ("system-memory", "管理记忆"),
    ("system-planning", "任务与技能"),
];

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    System,
    User,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolGroup {
    pub id: String,
    pub name: String,
    pub level: Level,
    pub tools: BTreeSet<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub level: Level,
    pub tool_group_ids: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_ids: Option<BTreeSet<String>>,
    pub workflow_ids: BTreeSet<String>,
    pub plugin_ids: BTreeSet<String>,
    pub agent_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub revision: u64,
    pub active_id: String,
    pub groups: Vec<ToolGroup>,
    pub profiles: Vec<Profile>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            revision: 0,
            active_id: DEFAULT_ID.into(),
            groups: vec![],
            profiles: vec![],
        }
    }
}

fn read(conn: &rusqlite::Connection, key: &str) -> Result<Option<Settings>, String> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key=?1", [key], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|e| e.to_string())?;
    raw.map(|value| serde_json::from_str(&value).map_err(|e| e.to_string()))
        .transpose()
}

pub fn settings(db: &Database) -> Result<Settings, String> {
    Ok(read(&db.conn(), KEY)?.unwrap_or_default())
}

pub fn save(db: &Database, mut next: Settings) -> Result<Settings, String> {
    let mut conn = db.conn();
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let current = read(&tx, KEY)?.unwrap_or_default();
    if current.revision != next.revision {
        return Err("Harness 配置已变化，请刷新后重试".into());
    }
    next.revision += 1;
    let body = serde_json::to_string(&next).map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT OR REPLACE INTO settings(key,value) VALUES (?1,?2)",
        params![KEY, body],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO settings(key,value) VALUES (?1,?2)",
        params![format!("{KEY}:{}", next.revision), body],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(next)
}

fn system_matches(group: &str, name: &str) -> bool {
    match group {
        "system-all" => true,
        "system-desktop" => {
            [
                "screenshot",
                "mouse",
                "keyboard",
                "process",
                "bash",
                "open_application",
            ]
            .contains(&name)
                || name.starts_with("software_")
        }
        "system-browser" => {
            name == "open_url" || name.starts_with("visible_browser_") || name == "http_request"
        }
        "system-files" => ["read_file", "write_file", "edit_file", "glob", "grep"].contains(&name),
        "system-memory" => name.ends_with("_memory") || name.ends_with("_memories"),
        "system-planning" => ["ask_user", "write_todos", "load_skill"].contains(&name),
        _ => false,
    }
}

pub fn system_groups(names: &BTreeSet<String>) -> Vec<ToolGroup> {
    SYSTEM_GROUPS
        .iter()
        .map(|&(id, name)| ToolGroup {
            id: id.into(),
            name: name.into(),
            level: Level::System,
            tools: names
                .iter()
                .filter(|tool| system_matches(id, tool))
                .cloned()
                .collect(),
        })
        .collect()
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn validate(
    value: &Settings,
    tools: &BTreeSet<String>,
    workflows: &HashSet<String>,
    plugins: &HashSet<String>,
    agents: &HashSet<String>,
) -> Result<(), String> {
    if value.groups.len() > 128 || value.profiles.len() > 64 {
        return Err("Harness 或工具组数量超限".into());
    }
    let mut groups: std::collections::HashMap<_, _> = SYSTEM_GROUPS
        .iter()
        .map(|(id, _)| (*id, Level::System))
        .collect();
    for group in &value.groups {
        if !valid_id(&group.id)
            || group.name.trim().is_empty()
            || group.name.chars().count() > 100
            || groups.insert(&group.id, group.level).is_some()
            || !group.tools.is_subset(tools)
        {
            return Err(format!("工具组配置无效或引用了不可用工具：{}", group.name));
        }
    }
    let mut ids = HashSet::from([DEFAULT_ID]);
    for profile in &value.profiles {
        if !valid_id(&profile.id)
            || profile.name.trim().is_empty()
            || profile.name.chars().count() > 100
            || profile.description.chars().count() > 1000
            || !ids.insert(&profile.id)
        {
            return Err("Harness 名称或标识无效".into());
        }
        if profile
            .tool_group_ids
            .iter()
            .any(|id| match groups.get(id.as_str()) {
                None => true,
                Some(level) => profile.level == Level::System && *level == Level::User,
            })
        {
            return Err(format!(
                "Harness 引用了不存在的组或超出级别的组：{}",
                profile.name
            ));
        }
        if profile
            .workflow_ids
            .iter()
            .any(|id| !workflows.contains(id))
            || profile.plugin_ids.iter().any(|id| !plugins.contains(id))
            || profile
                .agent_id
                .as_ref()
                .is_some_and(|id| !agents.contains(id))
        {
            return Err(format!("Harness 引用了不可用资源：{}", profile.name));
        }
    }
    if !ids.contains(value.active_id.as_str()) {
        return Err("当前 Harness 不存在".into());
    }
    Ok(())
}

#[derive(Clone)]
pub struct Selection {
    pub revision: u64,
    pub profile: Profile,
    custom_tools: BTreeSet<String>,
}

fn select(value: Settings, id: &str) -> Result<Option<Selection>, String> {
    if id == DEFAULT_ID {
        return Ok(None);
    }
    let profile = value
        .profiles
        .into_iter()
        .find(|item| item.id == id)
        .ok_or("Harness 配置已不存在")?;
    let custom_tools = value
        .groups
        .into_iter()
        .filter(|group| profile.tool_group_ids.contains(&group.id))
        .flat_map(|group| group.tools)
        .collect();
    Ok(Some(Selection {
        revision: value.revision,
        profile,
        custom_tools,
    }))
}

pub fn active(db: &Database) -> Result<Option<Selection>, String> {
    let value = settings(db)?;
    let id = value.active_id.clone();
    select(value, &id)
}

pub fn from_manifest(
    db: &Database,
    manifest: Option<&ContextManifest>,
) -> Result<Option<Selection>, String> {
    let Some(entry) = manifest.and_then(|m| m.entries.iter().find(|e| e.kind == "harness")) else {
        return Ok(None);
    };
    if entry.source_id == DEFAULT_ID {
        return Ok(None);
    }
    let revision = entry
        .version
        .as_deref()
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or("Harness 运行版本无效")?;
    let value =
        read(&db.conn(), &format!("{KEY}:{revision}"))?.ok_or("Harness 运行版本已不存在")?;
    select(value, &entry.source_id)
}

pub fn record(manifest: &mut ContextManifest, selection: Option<&Selection>) {
    let (id, revision, name) = selection.map_or((DEFAULT_ID, None, "系统默认"), |s| {
        (
            s.profile.id.as_str(),
            Some(s.revision.to_string()),
            s.profile.name.as_str(),
        )
    });
    manifest.record(
        "harness",
        id,
        revision,
        ContextStatus::Resolved,
        0,
        Some(name),
    );
}

impl Selection {
    pub fn allows_tool(&self, name: &str) -> bool {
        self.custom_tools.contains(name)
            || self
                .profile
                .tool_group_ids
                .iter()
                .any(|id| system_matches(id, name))
    }

    pub fn allows_workflow(&self, db: &Database, id: &str) -> Result<bool, String> {
        Ok(self.profile.workflow_ids.contains(id) && self.allows_resource(db, id)?)
    }

    fn allows_skill(&self, skill: &AvailableSkill) -> bool {
        self.profile
            .skill_ids
            .as_ref()
            .is_none_or(|ids| ids.contains(&skill.id))
            && skill
                .plugin_id
                .as_ref()
                .is_none_or(|id| self.profile.plugin_ids.contains(id))
    }

    pub fn allows_resource(&self, db: &Database, id: &str) -> Result<bool, String> {
        crate::modules::capability::import_store::require_active_plugin_resource(db, id)?;
        let resources =
            crate::api::capabilities::plugin_contributions::enabled_plugin_resources(db)?;
        Ok(resources
            .iter()
            .filter(|row| row["id"].as_str() == Some(id))
            .all(|row| {
                row["plugin_id"]
                    .as_str()
                    .is_some_and(|owner| self.profile.plugin_ids.contains(owner))
            }))
    }
}

#[derive(Serialize)]
pub(crate) struct AvailableSkill {
    pub id: String,
    pub name: String,
    pub plugin_id: Option<String>,
}

fn discovered_skills(server: &AppServer, ready_only: bool) -> Result<Vec<AvailableSkill>, String> {
    let inactive =
        crate::modules::capability::import_store::inactive_plugin_resource_ids(&server.db)?;
    let owners: HashMap<_, _> =
        crate::api::capabilities::plugin_contributions::enabled_plugin_resources(&server.db)?
            .into_iter()
            .filter(|row| row["kind"].as_str() == Some("skill"))
            .filter_map(|row| {
                Some((
                    row["id"].as_str()?.to_owned(),
                    row["plugin_id"].as_str()?.to_owned(),
                ))
            })
            .collect();
    Ok(server
        .skill_discovery
        .read()
        .all()
        .into_iter()
        .filter(|skill| {
            !inactive.contains(&skill.name) && (!ready_only || skill.compatibility.runtime_ready)
        })
        .map(|skill| AvailableSkill {
            id: skill.name.clone(),
            name: skill.display_name.clone(),
            plugin_id: owners.get(&skill.name).cloned(),
        })
        .collect())
}

pub(crate) fn available_skills(server: &AppServer) -> Result<Vec<AvailableSkill>, String> {
    let mut skills = discovered_skills(server, true)?;
    skills.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    Ok(skills)
}

pub struct ToolScope {
    pub selection: Selection,
    pub skill_names: HashSet<String>,
    pub agent_tools: Option<HashSet<String>>,
}

impl ToolScope {
    pub fn allows(&self, name: &str) -> bool {
        self.selection.allows_tool(name)
            && self
                .agent_tools
                .as_ref()
                .is_none_or(|tools| tools.contains(name))
    }
}

pub fn tool_scope(
    server: &AppServer,
    selection: &Selection,
    agent_tools: Option<&[String]>,
) -> Result<ToolScope, String> {
    let mut scope = ToolScope {
        selection: selection.clone(),
        skill_names: HashSet::new(),
        agent_tools: agent_tools.map(|tools| tools.iter().cloned().collect()),
    };
    if scope.allows("load_skill") {
        // Tool registry assembly refreshes compatibility after this scope is selected.
        scope.skill_names = discovered_skills(server, false)?
            .into_iter()
            .filter(|skill| selection.allows_skill(skill))
            .map(|skill| skill.id)
            .collect();
    }
    Ok(scope)
}
