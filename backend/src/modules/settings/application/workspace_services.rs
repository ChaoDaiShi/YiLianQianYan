use serde::{Deserialize, Serialize};

use crate::{
    api::chat::runtime::{resolve_scoped_model_source, ChatModelSource},
    app::state::AppServer,
    db::{
        basic_services::BasicServiceProfile,
        workspace_service_settings::{
            ChatServiceSelection, ServiceReference, ServiceSelection, ServiceSettingsError,
            WorkspaceServiceKind, WorkspaceServiceSettings,
        },
        Database,
    },
    workspace::WorkspaceId,
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ConfigurationScope {
    Global,
    Workspace,
    Agent,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ServiceConfigurationSource {
    source: String,
    id: String,
    label: String,
    scope: ConfigurationScope,
    revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    actual_profile: Option<InvokedServiceProfile>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct InvokedServiceProfile {
    id: String,
    model: String,
    revision: u64,
}

impl ServiceConfigurationSource {
    fn from_reference(
        reference: &ServiceReference,
        scope: ConfigurationScope,
        label: String,
        revision: u64,
    ) -> Self {
        let (source, id) = reference.columns();
        Self {
            source: source.into(),
            id: id.into(),
            label,
            scope,
            revision,
            actual_profile: None,
        }
    }

    pub(crate) fn set_invoked_profile(&mut self, profile: &BasicServiceProfile) {
        self.actual_profile = Some(InvokedServiceProfile {
            id: profile.id.clone(),
            model: profile.model.clone(),
            revision: profile.revision,
        });
    }

    fn chat(selection: ChatServiceSelection, scope: ConfigurationScope) -> Self {
        match selection {
            ChatServiceSelection::Profile(row) => Self::from_reference(
                &ServiceReference::Profile { id: row.id },
                scope,
                row.label,
                row.updated_at as u64,
            ),
            ChatServiceSelection::Pool(pool) => Self::from_reference(
                &ServiceReference::Pool { id: pool.id },
                scope,
                pool.label,
                pool.revision,
            ),
        }
    }
}

pub(crate) struct ResolvedService {
    pub(crate) source: ServiceConfigurationSource,
    pub(crate) selection: ServiceSelection,
}

#[derive(Serialize)]
pub(crate) struct WorkspaceServiceSettingsView {
    workspace_id: String,
    service_kind: WorkspaceServiceKind,
    #[serde(rename = "override")]
    override_value: Option<ServiceReference>,
    effective: Option<ServiceConfigurationSource>,
    inherited: Option<ServiceConfigurationSource>,
    revision: u64,
    application: &'static str,
}

fn global_reference(
    db: &Database,
    kind: WorkspaceServiceKind,
) -> Result<Option<ServiceReference>, ServiceSettingsError> {
    let kind = kind.as_str();
    if let Some(id) = db
        .get_model_pool_binding(&format!("service:{kind}"))
        .map_err(ServiceSettingsError::Storage)?
    {
        return Ok(Some(ServiceReference::Pool { id }));
    }
    Ok(db
        .get_basic_service_default(kind)
        .map_err(ServiceSettingsError::Storage)?
        .map(|id| ServiceReference::Profile { id }))
}

fn agent_reference(
    db: &Database,
    kind: WorkspaceServiceKind,
    agent_id: &str,
) -> Result<Option<ServiceReference>, ServiceSettingsError> {
    let binding = db
        .resolve_connector_bindings(agent_id)
        .map_err(ServiceSettingsError::Storage)?
        .into_iter()
        .find(|binding| {
            binding.kind == "agent_model" && binding.service_kind.as_deref() == Some(kind.as_str())
        });
    let Some(binding) = binding else {
        return Ok(None);
    };
    let source = binding
        .model_source
        .ok_or_else(|| ServiceSettingsError::Invalid("模型来源缺失".into()))?;
    let id = binding
        .target_id
        .strip_prefix("model:")
        .ok_or_else(|| ServiceSettingsError::Invalid("模型引用无效".into()))?;
    ServiceReference::from_binding(&source, id.to_owned()).map(Some)
}

fn resolve_reference(
    db: &Database,
    kind: WorkspaceServiceKind,
    reference: ServiceReference,
    scope: ConfigurationScope,
) -> Result<ResolvedService, ServiceSettingsError> {
    let selection = db.resolve_service_reference(kind, &reference)?;
    let (label, revision) = match &selection.pool {
        Some(pool) => (pool.label.clone(), pool.revision),
        None => {
            let profile = &selection.profiles[0];
            (profile.label.clone(), profile.revision)
        }
    };
    Ok(ResolvedService {
        source: ServiceConfigurationSource::from_reference(&reference, scope, label, revision),
        selection,
    })
}

pub(crate) fn resolve_conversation_service(
    db: &Database,
    kind: WorkspaceServiceKind,
    conversation_id: &str,
    agent_id: Option<&str>,
) -> Result<ResolvedService, ServiceSettingsError> {
    if let Some(agent_id) = agent_id {
        if let Some(reference) = agent_reference(db, kind, agent_id)? {
            return resolve_reference(db, kind, reference, ConfigurationScope::Agent);
        }
    }
    if !conversation_id.is_empty() {
        let workspace = db
            .conversation_workspace(conversation_id)
            .map_err(ServiceSettingsError::Storage)?;
        if let Some(workspace) = workspace {
            let workspace = WorkspaceId::new(workspace)
                .map_err(|error| ServiceSettingsError::Invalid(error.to_string()))?;
            let setting = db.get_workspace_service_settings(&workspace, kind)?;
            if let Some(reference) = setting.and_then(|setting| setting.override_value) {
                return resolve_reference(db, kind, reference, ConfigurationScope::Workspace);
            }
        }
    }
    let reference = global_reference(db, kind)?
        .ok_or_else(|| ServiceSettingsError::Invalid("请先设置该用途的默认来源".into()))?;
    resolve_reference(db, kind, reference, ConfigurationScope::Global)
}

pub(crate) fn settings_view(
    server: &AppServer,
    workspace: &WorkspaceId,
    kind: WorkspaceServiceKind,
    setting: WorkspaceServiceSettings,
) -> Result<WorkspaceServiceSettingsView, ServiceSettingsError> {
    let db = &server.db;
    let project = |reference, scope| match resolve_reference(db, kind, reference, scope) {
        Ok(resolved) => Ok(Some(resolved.source)),
        Err(ServiceSettingsError::Invalid(_)) => Ok(None),
        Err(error) => Err(error),
    };
    let inherited = match kind {
        WorkspaceServiceKind::Chat => global_chat_source(server)?,
        WorkspaceServiceKind::Vision | WorkspaceServiceKind::Image => global_reference(db, kind)?
            .map(|reference| project(reference, ConfigurationScope::Global))
            .transpose()?
            .flatten(),
    };
    let effective = match setting.override_value.clone() {
        Some(reference) if kind == WorkspaceServiceKind::Chat => {
            match db.resolve_chat_service_reference(&reference) {
                Ok(selection) => Some(ServiceConfigurationSource::chat(
                    selection,
                    ConfigurationScope::Workspace,
                )),
                Err(ServiceSettingsError::Invalid(_)) => None,
                Err(error) => return Err(error),
            }
        }
        Some(reference) => project(reference, ConfigurationScope::Workspace)?,
        None => inherited.clone(),
    };
    Ok(WorkspaceServiceSettingsView {
        workspace_id: workspace.as_str().to_owned(),
        service_kind: kind,
        override_value: setting.override_value,
        effective,
        inherited,
        revision: setting.revision,
        application: "next_call",
    })
}

fn global_chat_source(
    server: &AppServer,
) -> Result<Option<ServiceConfigurationSource>, ServiceSettingsError> {
    let config = server.config.read();
    let scope = ConfigurationScope::Global;
    let source = resolve_scoped_model_source(&server.db, &config.model, None, None)
        .map_err(ServiceSettingsError::Storage)?;
    Ok(match source {
        ChatModelSource::Fixed { row: Some(row), .. } if row.api_format == "openai" => Some(
            ServiceConfigurationSource::chat(ChatServiceSelection::Profile(row), scope),
        ),
        ChatModelSource::Pool(pool) if pool.enabled && pool.service_kind == "chat" => Some(
            ServiceConfigurationSource::chat(ChatServiceSelection::Pool(pool), scope),
        ),
        ChatModelSource::Fixed { config, row: None } => Some(ServiceConfigurationSource {
            source: "global_config".into(),
            id: "app_config".into(),
            label: config.name,
            scope,
            actual_profile: None,
            revision: server
                .db
                .get_settings_revision()
                .map_err(ServiceSettingsError::Storage)?,
        }),
        _ => None,
    })
}
