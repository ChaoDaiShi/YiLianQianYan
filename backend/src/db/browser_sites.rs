//! Project bookmarks and bounded, redacted visit metadata. Never grants access.
mod policy;
use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
pub use policy::{normalize_site_url, redacted_visit_url, NormalizedSiteUrl};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

const SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(1083, "1083_project_browser_sites", MigrationOwner::V1TaskWorld,
    "CREATE TABLE browser_project_sites(id TEXT PRIMARY KEY,workspace_id TEXT NOT NULL REFERENCES workspaces(id),
       title TEXT NOT NULL,purpose TEXT NOT NULL,url TEXT NOT NULL,origin TEXT NOT NULL,port INTEGER NOT NULL,
       startup_hint TEXT NOT NULL,bookmarked INTEGER NOT NULL DEFAULT 1,status TEXT NOT NULL DEFAULT 'active',
       revision INTEGER NOT NULL DEFAULT 1,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,last_visited_at INTEGER,
       CHECK(status IN ('active','archived')),CHECK(revision>0),CHECK(port BETWEEN 1 AND 65535));
     CREATE INDEX browser_sites_workspace ON browser_project_sites(workspace_id,status,updated_at,id);
     CREATE TABLE browser_visits(id TEXT PRIMARY KEY,workspace_id TEXT REFERENCES workspaces(id),site_id TEXT REFERENCES browser_project_sites(id),
       session_id TEXT NOT NULL,tab_id TEXT NOT NULL,navigation_id TEXT NOT NULL,redacted_url TEXT NOT NULL,title TEXT NOT NULL,visited_at INTEGER NOT NULL,
       UNIQUE(session_id,tab_id,navigation_id));
     CREATE INDEX browser_visits_recent ON browser_visits(visited_at DESC,id DESC);
     CREATE INDEX browser_visits_workspace ON browser_visits(workspace_id,visited_at DESC,id DESC);");

#[derive(Debug, thiserror::Error)]
pub enum BrowserSiteError {
    #[error("{0}")]
    Invalid(String),
    #[error("站点或工作空间不存在")]
    NotFound,
    #[error("站点已更新，请刷新后重试")]
    Conflict,
    #[error("工作空间或站点已归档")]
    Archived,
    #[error("浏览器站点数据存储失败")]
    Storage(#[from] rusqlite::Error),
}
#[derive(Clone, Debug, Serialize)]
pub struct ProjectSite {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    pub purpose: String,
    pub url: String,
    pub origin: String,
    pub port: u16,
    pub startup_hint: String,
    pub bookmarked: bool,
    pub status: String,
    pub revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_visited_at: Option<i64>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBrowserSite {
    pub workspace_id: String,
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub startup_hint: String,
    #[serde(default = "yes")]
    pub bookmarked: bool,
}
fn yes() -> bool {
    true
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateBrowserSite {
    pub title: Option<String>,
    pub purpose: Option<String>,
    pub url: Option<String>,
    pub startup_hint: Option<String>,
    pub bookmarked: Option<bool>,
    pub expected_revision: Option<i64>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserVisitInput {
    pub workspace_id: Option<String>,
    pub site_id: Option<String>,
    pub session_id: String,
    pub tab_id: String,
    pub navigation_id: String,
    pub url: String,
    #[serde(default)]
    pub title: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct BrowserVisit {
    pub id: String,
    pub workspace_id: Option<String>,
    pub site_id: Option<String>,
    pub session_id: String,
    pub tab_id: String,
    pub navigation_id: String,
    pub redacted_url: String,
    pub title: String,
    pub visited_at: i64,
}

const SITE_COLUMNS: &str = "id,workspace_id,title,purpose,url,origin,port,startup_hint,bookmarked,status,revision,created_at,updated_at,last_visited_at";
const VISIT_COLUMNS: &str =
    "id,workspace_id,site_id,session_id,tab_id,navigation_id,redacted_url,title,visited_at";
fn site_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectSite> {
    Ok(ProjectSite {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        title: r.get(2)?,
        purpose: r.get(3)?,
        url: r.get(4)?,
        origin: r.get(5)?,
        port: r.get(6)?,
        startup_hint: r.get(7)?,
        bookmarked: r.get(8)?,
        status: r.get(9)?,
        revision: r.get(10)?,
        created_at: r.get(11)?,
        updated_at: r.get(12)?,
        last_visited_at: r.get(13)?,
    })
}
fn visit_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<BrowserVisit> {
    Ok(BrowserVisit {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        site_id: r.get(2)?,
        session_id: r.get(3)?,
        tab_id: r.get(4)?,
        navigation_id: r.get(5)?,
        redacted_url: r.get(6)?,
        title: r.get(7)?,
        visited_at: r.get(8)?,
    })
}
fn find_site(conn: &Connection, id: &str) -> Result<ProjectSite, BrowserSiteError> {
    conn.query_row(
        &format!("SELECT {SITE_COLUMNS} FROM browser_project_sites WHERE id=?1"),
        [id],
        site_row,
    )
    .optional()?
    .ok_or(BrowserSiteError::NotFound)
}
fn active_workspace(conn: &Connection, id: &str) -> Result<(), BrowserSiteError> {
    match conn
        .query_row("SELECT status FROM workspaces WHERE id=?1", [id], |r| {
            r.get::<_, String>(0)
        })
        .optional()?
        .as_deref()
    {
        Some("active") => Ok(()),
        Some(_) => Err(BrowserSiteError::Archived),
        None => Err(BrowserSiteError::NotFound),
    }
}
fn checked_text(
    raw: &str,
    name: &str,
    max: usize,
    required: bool,
) -> Result<String, BrowserSiteError> {
    let value = raw.trim();
    if (required && value.is_empty())
        || value.chars().count() > max
        || value
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(BrowserSiteError::Invalid(format!(
            "{name}不能为空、包含控制字符或超过{max}字符"
        )));
    }
    Ok(value.into())
}

impl Database {
    pub fn initialize_browser_sites(&self) -> rusqlite::Result<()> {
        self.apply_product_migration(&SCHEMA)
    }
    pub fn get_browser_site(&self, id: &str) -> Result<ProjectSite, BrowserSiteError> {
        find_site(&self.conn(), id)
    }
    pub fn list_browser_sites(
        &self,
        workspace: Option<&str>,
        include_archived: bool,
        limit: usize,
    ) -> Result<Vec<ProjectSite>, BrowserSiteError> {
        let conn = self.conn();
        let mut stmt=conn.prepare(&format!("SELECT {SITE_COLUMNS} FROM browser_project_sites WHERE (?1 IS NULL OR workspace_id=?1) AND (?2 OR status='active') ORDER BY COALESCE(last_visited_at,0) DESC,updated_at DESC,id DESC LIMIT ?3"))?;
        let rows = stmt.query_map(
            params![workspace, include_archived, limit.clamp(1, 201) as i64],
            site_row,
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
    pub fn create_browser_site(
        &self,
        input: &CreateBrowserSite,
        now: i64,
    ) -> Result<ProjectSite, BrowserSiteError> {
        let normalized = normalize_site_url(&input.url).map_err(BrowserSiteError::Invalid)?;
        let title = checked_text(&input.title, "标题", 120, true)?;
        let purpose = checked_text(&input.purpose, "用途", 2000, false)?;
        let hint = checked_text(&input.startup_hint, "启动提示", 4000, false)?;
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        active_workspace(&tx, &input.workspace_id)?;
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute("INSERT INTO browser_project_sites(id,workspace_id,title,purpose,url,origin,port,startup_hint,bookmarked,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)",params![id,input.workspace_id,title,purpose,normalized.url,normalized.origin,normalized.port,hint,input.bookmarked,now])?;
        let site = find_site(&tx, &id)?;
        tx.commit()?;
        Ok(site)
    }
    pub fn update_browser_site(
        &self,
        id: &str,
        input: &UpdateBrowserSite,
        now: i64,
    ) -> Result<ProjectSite, BrowserSiteError> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut site = find_site(&tx, id)?;
        active_workspace(&tx, &site.workspace_id)?;
        if site.status != "active" {
            return Err(BrowserSiteError::Archived);
        }
        if input.expected_revision.is_some_and(|r| r != site.revision) {
            return Err(BrowserSiteError::Conflict);
        }
        if let Some(v) = &input.title {
            site.title = checked_text(v, "标题", 120, true)?
        }
        if let Some(v) = &input.purpose {
            site.purpose = checked_text(v, "用途", 2000, false)?
        }
        if let Some(v) = &input.startup_hint {
            site.startup_hint = checked_text(v, "启动提示", 4000, false)?
        }
        if let Some(v) = input.bookmarked {
            site.bookmarked = v
        }
        if let Some(v) = &input.url {
            let n = normalize_site_url(v).map_err(BrowserSiteError::Invalid)?;
            site.url = n.url;
            site.origin = n.origin;
            site.port = n.port;
        }
        tx.execute("UPDATE browser_project_sites SET title=?2,purpose=?3,url=?4,origin=?5,port=?6,startup_hint=?7,bookmarked=?8,revision=revision+1,updated_at=MAX(updated_at,?9) WHERE id=?1",params![id,site.title,site.purpose,site.url,site.origin,site.port,site.startup_hint,site.bookmarked,now])?;
        let result = find_site(&tx, id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn set_browser_site_archived(
        &self,
        id: &str,
        archived: bool,
        expected: Option<i64>,
        now: i64,
    ) -> Result<ProjectSite, BrowserSiteError> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let site = find_site(&tx, id)?;
        if !archived {
            active_workspace(&tx, &site.workspace_id)?;
        }
        if expected.is_some_and(|r| r != site.revision) {
            return Err(BrowserSiteError::Conflict);
        }
        let status = if archived { "archived" } else { "active" };
        if site.status != status {
            tx.execute("UPDATE browser_project_sites SET status=?2,revision=revision+1,updated_at=MAX(updated_at,?3) WHERE id=?1",params![id,status,now])?;
        }
        let result = find_site(&tx, id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn note_browser_visit(
        &self,
        input: &BrowserVisitInput,
        now: i64,
    ) -> Result<BrowserVisit, BrowserSiteError> {
        let redacted = redacted_visit_url(&input.url).map_err(BrowserSiteError::Invalid)?;
        for value in [&input.session_id, &input.tab_id, &input.navigation_id] {
            checked_text(value, "会话标识", 160, true)?;
        }
        let title = input
            .title
            .chars()
            .filter(|c| !c.is_control())
            .take(240)
            .collect::<String>();
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing=tx.query_row(&format!("SELECT {VISIT_COLUMNS} FROM browser_visits WHERE session_id=?1 AND tab_id=?2 AND navigation_id=?3"),params![input.session_id,input.tab_id,input.navigation_id],visit_row).optional()?;
        let mut workspace = input.workspace_id.clone();
        if let Some(id) = &input.site_id {
            let site = find_site(&tx, id)?;
            if site.status != "active" {
                return Err(BrowserSiteError::Archived);
            }
            if workspace.as_ref().is_some_and(|w| w != &site.workspace_id) {
                return Err(BrowserSiteError::Invalid("站点与项目不匹配".into()));
            }
            workspace = Some(site.workspace_id);
        }
        if let Some(id) = &workspace {
            active_workspace(&tx, id)?;
        }
        if let Some(old) = existing {
            if old.workspace_id != workspace
                || old.site_id != input.site_id
                || old.redacted_url != redacted
            {
                return Err(BrowserSiteError::Conflict);
            }
            return Ok(old);
        }
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute("INSERT INTO browser_visits(id,workspace_id,site_id,session_id,tab_id,navigation_id,redacted_url,title,visited_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![id,workspace,input.site_id,input.session_id,input.tab_id,input.navigation_id,redacted,title,now])?;
        if let Some(site) = &input.site_id {
            tx.execute("UPDATE browser_project_sites SET last_visited_at=MAX(COALESCE(last_visited_at,0),?2) WHERE id=?1",params![site,now])?;
        }
        tx.execute("DELETE FROM browser_visits WHERE id IN(SELECT id FROM browser_visits ORDER BY visited_at DESC,id DESC LIMIT -1 OFFSET 10000)",[])?;
        let result = tx.query_row(
            &format!("SELECT {VISIT_COLUMNS} FROM browser_visits WHERE id=?1"),
            [id],
            visit_row,
        )?;
        tx.commit()?;
        Ok(result)
    }
    pub fn list_browser_visits(
        &self,
        workspace: Option<&str>,
        site: Option<&str>,
        limit: usize,
    ) -> Result<Vec<BrowserVisit>, BrowserSiteError> {
        let conn = self.conn();
        let mut stmt=conn.prepare(&format!("SELECT {VISIT_COLUMNS} FROM browser_visits WHERE (?1 IS NULL OR workspace_id=?1) AND (?2 IS NULL OR site_id=?2) ORDER BY visited_at DESC,id DESC LIMIT ?3"))?;
        let rows = stmt.query_map(
            params![workspace, site, limit.clamp(1, 201) as i64],
            visit_row,
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

#[cfg(test)]
mod tests;
