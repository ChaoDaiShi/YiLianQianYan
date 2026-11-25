use super::*;
use std::path::{Path, PathBuf};

pub(super) struct Fixture {
    pub(super) server: AppServer,
    pub(super) root: PathBuf,
}
impl Fixture {
    pub(super) fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "yilian-personal-management-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let server = AppServer::new_with_control_session_and_store(
            &root.join("state.db"),
            root.to_str().unwrap(),
            ControlSession::new("personal-management-controlled-local-token-000000000000").unwrap(),
            Arc::new(InMemorySecretStore::new()),
        )
        .unwrap();
        // Metadata-only reference. No credentials are written or resolved.
        server.config.write().model.api_key_ref =
            Some(SecretRef::new("controlled.metadata.reference"));
        Self { server, root }
    }
    pub(super) fn save(&self, runs: u32) -> Automation {
        let workspace = crate::workspace::Workspace::new(
            crate::workspace::WorkspaceId::generate(),
            "Management fixture".into(),
            String::new(),
            Some(self.root.to_string_lossy().into_owned()),
            1,
        )
        .unwrap();
        self.server.db.create_workspace(&workspace).unwrap();
        super::super::super::super::save(
            &self.server,
            None,
            AutomationInput {
                expected_revision: None,
                name: "Managed report".into(),
                workflow_graph_id: String::new(),
                target: Some(AutomationTarget::PersonalAssistant {
                    goal: "Read and report".into(),
                    conversation_id: String::new(),
                    root_path: String::new(),
                    allow_file_writes: true,
                    allow_file_discovery: false,
                    max_runs: runs,
                }),
                workspace_id: workspace.id.to_string(),
                timezone: "UTC".into(),
                schedule: Schedule::Daily {
                    time: "09:00".into(),
                },
                enabled: true,
            },
        )
        .unwrap()
    }
    pub(super) fn conversation<'a>(&self, a: &'a Automation) -> &'a str {
        match a.target.as_ref().unwrap() {
            AutomationTarget::PersonalAssistant {
                conversation_id, ..
            } => conversation_id,
            _ => unreachable!(),
        }
    }
    pub(super) fn grant(&self, path: &Path, effect: GrantEffect) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.server
            .db
            .create_grant(&SecurityGrant {
                id: id.clone(),
                subject_id: "local-user".into(),
                effect,
                permission: PermissionId::FilesystemRead,
                resource: GrantResource::Filesystem {
                    root: path.to_string_lossy().into_owned(),
                    recursive: true,
                },
                source: GrantSource::User,
                created_at: 0,
                expires_at: None,
            })
            .unwrap();
        id
    }
    pub(super) fn record(
        &self,
        a: &Automation,
        call: &str,
        path: &Path,
        content: &str,
        succeeded: bool,
    ) {
        let conversation = self.conversation(a);
        let mut event = AgentEvent::error_event(conversation, String::new());
        event.event_type = "tool_start".into();
        event.error = None;
        event.tool_call_id = Some(call.into());
        event.tool_name = Some("write_file".into());
        event.args = Some(json!({"path":path,"content":content}));
        self.server.db.apply_execution_event(&event).unwrap();
        event.event_type = "tool_end".into();
        event.status = Some(if succeeded { "success" } else { "error" }.into());
        event.result = Some("fixture result".into());
        self.server.db.apply_execution_event(&event).unwrap();
        event.event_type = "verification".into();
        event.verification_success = Some(succeeded);
        self.server.db.apply_execution_event(&event).unwrap();
    }
    pub(super) fn occurrence(&self, a: &Automation, status: &str) -> Occurrence {
        let mut value = super::super::super::super::occurrence(a, 1, true, false);
        value.created_at = 1;
        value.status = status.into();
        assert!(self
            .server
            .db
            .claim_automation_occurrence(a, a.next_run_at, &mut value, true)
            .unwrap());
        value
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
