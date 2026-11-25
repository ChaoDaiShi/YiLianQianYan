use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

use crate::workspace::WorkspaceId;

use super::{
    basic_services::{get_basic_service_profile_on, BasicServiceProfile},
    llm_models::{get_llm_model_on, LlmModelRow},
    model_pools::{get_model_pool_on, ModelPool},
    Database,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceServiceKind {
    Vision,
    Image,
    Chat,
}

impl WorkspaceServiceKind {
    pub(crate) fn parse(value: &str) -> Result<Self, ServiceSettingsError> {
        match value {
            "vision" => Ok(Self::Vision),
            "image" => Ok(Self::Image),
            "chat" => Ok(Self::Chat),
            _ => Err(ServiceSettingsError::Invalid(
                "该服务用途不支持工作空间覆盖".into(),
            )),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Vision => "vision",
            Self::Image => "image",
            Self::Chat => "chat",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub(crate) enum ServiceReference {
    Profile { id: String },
    Pool { id: String },
}

impl ServiceReference {
    pub(crate) fn columns(&self) -> (&'static str, &str) {
        match self {
            Self::Profile { id } => ("profile", id),
            Self::Pool { id } => ("pool", id),
        }
    }

    pub(crate) fn from_binding(source: &str, id: String) -> Result<Self, ServiceSettingsError> {
        match source {
            "profile" => Ok(Self::Profile { id }),
            "pool" => Ok(Self::Pool { id }),
            _ => Err(ServiceSettingsError::Invalid("服务来源无效".into())),
        }
    }

    fn resolve_on(
        &self,
        conn: &Connection,
        kind: WorkspaceServiceKind,
    ) -> Result<ServiceSelection, ServiceSettingsError> {
        let profile = |id| {
            let profile = get_basic_service_profile_on(conn, id)
                .map_err(ServiceSettingsError::Storage)?
                .ok_or_else(|| ServiceSettingsError::Invalid("服务档案已删除".into()))?;
            if profile.service_kind != kind.as_str() {
                return Err(ServiceSettingsError::Invalid("服务档案用途不匹配".into()));
            }
            Ok(profile)
        };
        match self {
            Self::Profile { id } => Ok(ServiceSelection {
                pool: None,
                profiles: vec![profile(id)?],
            }),
            Self::Pool { id } => {
                let pool = resolve_pool_on(conn, id, kind)?;
                let profiles = pool
                    .tiers
                    .iter()
                    .flat_map(|tier| &tier.profile_ids)
                    .map(|id| profile(id))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(ServiceSelection {
                    pool: Some(pool),
                    profiles,
                })
            }
        }
    }

    fn resolve_chat_on(
        &self,
        conn: &Connection,
    ) -> Result<ChatServiceSelection, ServiceSettingsError> {
        match self {
            Self::Profile { id } => {
                let profile = chat_profile_on(conn, id)?;
                Ok(ChatServiceSelection::Profile(profile))
            }
            Self::Pool { id } => {
                let pool = resolve_pool_on(conn, id, WorkspaceServiceKind::Chat)?;
                for id in pool.tiers.iter().flat_map(|tier| &tier.profile_ids) {
                    chat_profile_on(conn, id)?;
                }
                Ok(ChatServiceSelection::Pool(pool))
            }
        }
    }
}

fn resolve_pool_on(
    conn: &Connection,
    id: &str,
    kind: WorkspaceServiceKind,
) -> Result<ModelPool, ServiceSettingsError> {
    let pool = get_model_pool_on(conn, id)
        .map_err(ServiceSettingsError::Storage)?
        .ok_or_else(|| ServiceSettingsError::Invalid("模型池已删除".into()))?;
    if !pool.enabled || pool.service_kind != kind.as_str() {
        return Err(ServiceSettingsError::Invalid(
            "模型池未启用或用途不匹配".into(),
        ));
    }
    Ok(pool)
}

fn chat_profile_on(conn: &Connection, id: &str) -> Result<LlmModelRow, ServiceSettingsError> {
    let profile = get_llm_model_on(conn, id)?
        .ok_or_else(|| ServiceSettingsError::Invalid("对话模型档案已删除".into()))?;
    if profile.api_format != "openai" {
        return Err(ServiceSettingsError::Invalid(
            "模型档案不支持对话协议".into(),
        ));
    }
    Ok(profile)
}

pub(crate) enum ChatServiceSelection {
    Profile(LlmModelRow),
    Pool(ModelPool),
}

pub(crate) struct ServiceSelection {
    pub(crate) pool: Option<ModelPool>,
    pub(crate) profiles: Vec<BasicServiceProfile>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ServiceSettingsError {
    #[error("工作空间不存在")]
    WorkspaceMissing,
    #[error("工作空间配置已有更新，请重新载入后保存")]
    RevisionConflict,
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Storage(String),
}

impl From<rusqlite::Error> for ServiceSettingsError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Storage(error.to_string())
    }
}

pub(crate) struct WorkspaceServiceSettings {
    pub(crate) revision: u64,
    pub(crate) override_value: Option<ServiceReference>,
}

#[derive(Serialize)]
pub(crate) struct WorkspaceServiceOverrideSummary {
    workspace_id: String,
    name: String,
    kinds: Vec<WorkspaceServiceKind>,
}

impl Database {
    pub(super) fn initialize_workspace_service_settings(&self) -> rusqlite::Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS workspace_service_revisions (
                workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id),
                revision INTEGER NOT NULL
             );",
        )?;
        let existing: Option<String> = tx.query_row(
            "SELECT sql FROM sqlite_schema WHERE type='table' AND name='workspace_service_overrides'",
            [],
            |row| row.get(0),
        ).optional()?;
        let migrate = existing.is_some_and(|sql| !sql.contains("'chat'"));
        if migrate {
            tx.execute_batch(
                "ALTER TABLE workspace_service_overrides RENAME TO workspace_service_overrides_legacy;",
            )?;
        }
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS workspace_service_overrides (
                workspace_id TEXT NOT NULL REFERENCES workspaces(id),
                service_kind TEXT NOT NULL CHECK(service_kind IN ('vision', 'image', 'chat')),
                source TEXT NOT NULL CHECK(source IN ('profile', 'pool')),
                target_id TEXT NOT NULL,
                PRIMARY KEY (workspace_id, service_kind)
             );",
        )?;
        if migrate {
            tx.execute_batch(
                "INSERT INTO workspace_service_overrides
                   SELECT workspace_id, service_kind, source, target_id
                   FROM workspace_service_overrides_legacy;
                 DROP TABLE workspace_service_overrides_legacy;",
            )?;
        }
        tx.commit()
    }

    pub(crate) fn resolve_service_reference(
        &self,
        kind: WorkspaceServiceKind,
        reference: &ServiceReference,
    ) -> Result<ServiceSelection, ServiceSettingsError> {
        reference.resolve_on(&self.conn(), kind)
    }

    pub(crate) fn resolve_chat_service_reference(
        &self,
        reference: &ServiceReference,
    ) -> Result<ChatServiceSelection, ServiceSettingsError> {
        reference.resolve_chat_on(&self.conn())
    }

    pub(crate) fn get_workspace_service_settings(
        &self,
        workspace: &WorkspaceId,
        kind: WorkspaceServiceKind,
    ) -> Result<Option<WorkspaceServiceSettings>, ServiceSettingsError> {
        workspace_service_settings_on(&self.conn(), workspace, kind)
    }

    pub(crate) fn set_workspace_service_override(
        &self,
        workspace: &WorkspaceId,
        kind: WorkspaceServiceKind,
        expected_revision: u64,
        override_value: Option<ServiceReference>,
    ) -> Result<WorkspaceServiceSettings, ServiceSettingsError> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = workspace_service_settings_on(&tx, workspace, kind)?
            .ok_or(ServiceSettingsError::WorkspaceMissing)?;
        if current.revision != expected_revision {
            return Err(ServiceSettingsError::RevisionConflict);
        }
        if let Some(reference) = &override_value {
            match kind {
                WorkspaceServiceKind::Chat => {
                    reference.resolve_chat_on(&tx)?;
                }
                WorkspaceServiceKind::Vision | WorkspaceServiceKind::Image => {
                    reference.resolve_on(&tx, kind)?;
                }
            }
        }
        if current.override_value == override_value {
            return Ok(current);
        }

        if let Some(reference) = &override_value {
            let (source, id) = reference.columns();
            tx.execute(
                "INSERT INTO workspace_service_overrides
                 (workspace_id, service_kind, source, target_id) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(workspace_id, service_kind)
                 DO UPDATE SET source=excluded.source, target_id=excluded.target_id",
                params![workspace.as_str(), kind.as_str(), source, id],
            )?;
        } else {
            tx.execute(
                "DELETE FROM workspace_service_overrides WHERE workspace_id=?1 AND service_kind=?2",
                params![workspace.as_str(), kind.as_str()],
            )?;
        }
        let revision = current.revision + 1;
        tx.execute(
            "INSERT INTO workspace_service_revisions (workspace_id, revision) VALUES (?1, ?2)
             ON CONFLICT(workspace_id) DO UPDATE SET revision=excluded.revision",
            params![workspace.as_str(), revision],
        )?;
        tx.commit()?;
        Ok(WorkspaceServiceSettings {
            revision,
            override_value,
        })
    }

    pub(crate) fn list_workspace_service_overrides(
        &self,
    ) -> Result<Vec<WorkspaceServiceOverrideSummary>, ServiceSettingsError> {
        let conn = self.conn();
        let mut statement = conn.prepare(
            "SELECT w.id, w.name, MAX(o.service_kind='vision'), MAX(o.service_kind='image'),
                    MAX(o.service_kind='chat')
             FROM workspaces w JOIN workspace_service_overrides o ON o.workspace_id=w.id
             WHERE w.status!='deleted'
             GROUP BY w.id
             ORDER BY w.updated_at DESC, w.id DESC",
        )?;
        let rows = statement.query_map([], |row| {
            let mut kinds = Vec::new();
            if row.get(2)? {
                kinds.push(WorkspaceServiceKind::Vision);
            }
            if row.get(3)? {
                kinds.push(WorkspaceServiceKind::Image);
            }
            if row.get(4)? {
                kinds.push(WorkspaceServiceKind::Chat);
            }
            Ok(WorkspaceServiceOverrideSummary {
                workspace_id: row.get(0)?,
                name: row.get(1)?,
                kinds,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

fn workspace_service_settings_on(
    conn: &Connection,
    workspace: &WorkspaceId,
    kind: WorkspaceServiceKind,
) -> Result<Option<WorkspaceServiceSettings>, ServiceSettingsError> {
    let row = conn
        .query_row(
            "SELECT COALESCE(r.revision, 0), o.source, o.target_id
             FROM workspaces w
             LEFT JOIN workspace_service_revisions r ON r.workspace_id=w.id
             LEFT JOIN workspace_service_overrides o ON o.workspace_id=w.id AND o.service_kind=?2
             WHERE w.id=?1 AND w.status!='deleted'",
            params![workspace.as_str(), kind.as_str()],
            |row| Ok((row.get(0)?, row.get::<_, Option<String>>(1)?, row.get(2)?)),
        )
        .optional()?;
    row.map(|(revision, source, id)| {
        Ok(WorkspaceServiceSettings {
            revision,
            override_value: source
                .zip(id)
                .map(|(source, id)| ServiceReference::from_binding(&source, id))
                .transpose()?,
        })
    })
    .transpose()
}

pub(super) fn workspace_service_reference_exists(
    conn: &Connection,
    source: &str,
    id: &str,
    chat: Option<bool>,
) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM workspace_service_overrides o
            JOIN workspaces w ON w.id=o.workspace_id
            WHERE o.source=?1 AND o.target_id=?2 AND w.status!='deleted'
              AND (?3 IS NULL OR (o.service_kind='chat')=?3)
         )",
        params![source, id, chat],
        |row| row.get(0),
    )
}
