use super::*;
use crate::{
    integrations::secret::InMemorySecretStore,
    modules::{
        automation::{
            self,
            model::{AutomationInput, AutomationTarget, Schedule},
        },
        task::{validation::ValidationPolicy, TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind},
    },
    safety::{
        grant::{GrantEffect, GrantResource, GrantSource, SecurityGrant},
        ControlSession,
    },
    workspace::{Workspace, WorkspaceId},
};
static PREVIEW_TEST_GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
struct Fixture {
    server: Arc<AppServer>,
    root: std::path::PathBuf,
    workspace: Workspace,
    other: Workspace,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "yilian-integration-outputs-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let server = Arc::new(
            AppServer::new_with_control_session_and_store(
                &root.join("state.db"),
                root.to_str().unwrap(),
                ControlSession::new("integration-outputs-local-test-control-session-0000000")
                    .unwrap(),
                Arc::new(InMemorySecretStore::new()),
            )
            .unwrap(),
        );
        let workspace = Workspace::new(
            WorkspaceId::generate(),
            "origin".into(),
            String::new(),
            Some(root.to_string_lossy().into()),
            1,
        )
        .unwrap();
        let other = Workspace::new(
            WorkspaceId::generate(),
            "destination".into(),
            String::new(),
            Some(root.to_string_lossy().into()),
            1,
        )
        .unwrap();
        server.db.create_workspace(&workspace).unwrap();
        server.db.create_workspace(&other).unwrap();
        for grant in server.db.list_grants("local-user").unwrap() {
            if grant.permission == PermissionId::FilesystemRead {
                server.db.delete_grant(&grant.id).unwrap();
            }
        }
        Self {
            server,
            root,
            workspace,
            other,
        }
    }
    fn grant(&self, effect: GrantEffect) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.server
            .db
            .create_grant(&SecurityGrant {
                id: id.clone(),
                subject_id: "local-user".into(),
                effect,
                permission: PermissionId::FilesystemRead,
                resource: GrantResource::Filesystem {
                    root: self.root.to_string_lossy().into(),
                    recursive: true,
                },
                source: GrantSource::User,
                created_at: 0,
                expires_at: None,
            })
            .unwrap();
        id
    }
    fn chat(&self) -> (String, String) {
        let conversation = self.server.db.create_conversation("code output").unwrap();
        self.server
            .db
            .set_conversation_workspace(&conversation.id, Some(self.workspace.id.as_str()))
            .unwrap();
        self.server.db.conn().execute("INSERT INTO messages(id,conversation_id,role,content,created_at)VALUES('output-message',?1,'assistant','controlled',1)",[&conversation.id]).unwrap();
        let resource = self
            .server
            .resource_service
            .ingest("code.txt", "text/plain", b"controlled source", json!({}))
            .unwrap();
        let output = self
            .server
            .db
            .register_conversation_code_output(
                &conversation.id,
                "output-message",
                0,
                "Code",
                &resource.id,
                None,
            )
            .unwrap();
        (conversation.id, output.id)
    }
    fn personal(&self, call: &str) -> String {
        let automation = automation::save(
            &self.server,
            None,
            AutomationInput {
                expected_revision: None,
                name: "local output".into(),
                workflow_graph_id: String::new(),
                target: Some(AutomationTarget::PersonalAssistant {
                    goal: "controlled report".into(),
                    conversation_id: String::new(),
                    root_path: String::new(),
                    allow_file_writes: true,
                    allow_file_discovery: false,
                    max_runs: 5,
                }),
                workspace_id: self.workspace.id.to_string(),
                timezone: "UTC".into(),
                schedule: Schedule::Daily {
                    time: "09:00".into(),
                },
                enabled: false,
            },
        )
        .unwrap();
        let Some(AutomationTarget::PersonalAssistant {
            conversation_id, ..
        }) = automation.target
        else {
            unreachable!()
        };
        let path = self.root.join("report.txt");
        std::fs::write(&path, "local report").unwrap();
        self.server.db.conn().execute("INSERT INTO conversation_execution_records(conversation_id,tool_call_id,name,args_json,risk_level,execution_status,approval_status,verification_status,started_at,finished_at,sequence,created_at,updated_at)VALUES(?1,?2,'write_file',?3,'medium','succeeded','not_required','passed',20,21,1,20,21)",params![conversation_id,call,json!({"path":path,"content":"local report"}).to_string()]).unwrap();
        automation.id
    }
    fn artifact(&self) -> String {
        let graph = TaskGraphId::new("source-graph").unwrap();
        let node = TaskNodeId::new("source-node").unwrap();
        self.server
            .task_world
            .create_graph(
                graph.clone(),
                vec![
                    TaskNode::new(node.clone(), TaskNodeKind::Work, "local result", json!({}))
                        .unwrap(),
                ],
                vec![],
                1,
            )
            .unwrap();
        let execution = self
            .server
            .task_world
            .start_execution(&graph, &node, 1, 2)
            .unwrap();
        self.server
            .task_world
            .complete_execution(
                &graph,
                &execution.id,
                json!({"ok":true,"result":"verified local text"}),
                ValidationPolicy::StructuredResult,
                3,
            )
            .unwrap();
        // Explicit contract metadata fixture: its source space differs from the export destination.
        self.server.db.conn().execute("INSERT INTO proofloop_goal_versions(goal_id,version,workspace_id,contract_json,confirmed_at) VALUES('origin-goal',1,?1,'{}',1)",[self.workspace.id.as_str()]).unwrap();
        self.server.db.conn().execute("INSERT INTO proofloop_drafts(draft_id,revision,goal_id,goal_version,graph_id,workspace_id,user_goal,payload_json,status,updated_at) VALUES('origin-draft',1,'origin-goal',1,'source-graph',?1,'origin','{}','confirmed',1)",[self.workspace.id.as_str()]).unwrap();
        ArtifactService::new(self.server.db.clone())
            .materialize(
                &ArtifactSource::Node {
                    graph_id: graph.to_string(),
                    node_id: node.to_string(),
                    execution_id: execution.id.to_string(),
                    workspace_id: self.other.id.to_string(),
                },
                "Report",
                self.root.to_str().unwrap(),
                30,
            )
            .unwrap()
            .0
            .id
            .to_string()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn integration_outputs_catalogue_merges_three_real_sources_without_promoting_verification() {
    let f = Fixture::new();
    let (_, code) = f.chat();
    let personal = f.personal(&code);
    let artifact = f.artifact();
    let page = catalogue(&f.server.db, Some(f.workspace.id.as_str()), 50).unwrap();
    let outputs = page["outputs"].as_array().unwrap();
    assert_eq!(outputs.len(), 3);
    assert_eq!(
        outputs.iter().filter(|o| o["id"] == code).count(),
        2,
        "same id across namespaces must remain distinct"
    );
    let result = outputs.iter().find(|o| o["id"] == artifact).unwrap();
    assert_eq!(result["source_workspace_id"], f.workspace.id.as_str());
    assert_eq!(result["destination_workspace_id"], f.other.id.as_str());
    let file = outputs
        .iter()
        .find(|o| o["kind"] == "personal_file")
        .unwrap();
    assert_eq!(file["source"]["automation_id"], personal);
    assert_eq!(file["verification_status"], "passed");
    let other = catalogue(&f.server.db, Some(f.other.id.as_str()), 50).unwrap();
    assert_eq!(other["outputs"].as_array().unwrap().len(), 1);
    assert!(catalogue(&f.server.db, Some("missing-space"), 50).is_err());
    assert_eq!(catalogue(&f.server.db, None, 1).unwrap()["truncated"], true);
}
#[test]
fn integration_outputs_origin_snapshot_does_not_follow_later_contract_changes() {
    let f = Fixture::new();
    let id = f.artifact();
    f.server
        .db
        .conn()
        .execute(
            "UPDATE proofloop_goal_versions SET workspace_id=?1 WHERE goal_id='origin-goal'",
            [f.other.id.as_str()],
        )
        .unwrap();
    let output = artifact_output(&f.server.db, &id).unwrap();
    assert_eq!(
        output.source_workspace_id.as_deref(),
        Some(f.workspace.id.as_str())
    );
    assert_eq!(
        catalogue(&f.server.db, Some(f.workspace.id.as_str()), 50).unwrap()["outputs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    f.server
        .db
        .conn()
        .execute(
            "UPDATE artifact_provenance SET origin_json=NULL WHERE artifact_id=?1",
            [&id],
        )
        .unwrap();
    let legacy = artifact_output(&f.server.db, &id).unwrap();
    assert_eq!(legacy.source_workspace_id, None);
    assert_eq!(
        legacy.destination_workspace_id.as_deref(),
        Some(f.other.id.as_str())
    );
    assert_eq!(
        legacy.source.kind, "legacy_node_artifact",
        "legacy records may not borrow today's source space"
    );
}
#[test]
fn integration_outputs_corrupt_origin_keeps_valid_siblings_and_reports_issue() {
    let f = Fixture::new();
    let (_, code) = f.chat();
    let bad = f.artifact();
    f.server
        .db
        .conn()
        .execute(
            "UPDATE artifact_provenance SET origin_json='not-json' WHERE artifact_id=?1",
            [&bad],
        )
        .unwrap();
    let page = catalogue(&f.server.db, Some(f.other.id.as_str()), 50).unwrap();
    assert_eq!(page["outputs"].as_array().unwrap().len(), 0);
    assert_eq!(page["issues"][0]["id"], bad);
    let page = catalogue(&f.server.db, None, 50).unwrap();
    assert!(page["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["id"] == code));
    assert_eq!(page["issues"].as_array().unwrap().len(), 1);
}
#[tokio::test]
async fn integration_outputs_preview_preserves_source_space_and_resource_integrity() {
    let _serial = PREVIEW_TEST_GATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let f = Fixture::new();
    let (conversation, id) = f.chat();
    let query = || PreviewQuery {
        source_id: Some(conversation.clone()),
        workspace_id: Some(f.workspace.id.to_string()),
    };
    let value = preview(
        State(f.server.clone()),
        Path(("conversation".into(), id.clone())),
        Query(query()),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(value["text"], "controlled source");
    assert!(preview(
        State(f.server.clone()),
        Path(("conversation".into(), id.clone())),
        Query(PreviewQuery {
            source_id: Some(conversation.clone()),
            workspace_id: Some(f.other.id.to_string())
        })
    )
    .await
    .is_err());
    assert!(preview(
        State(f.server.clone()),
        Path(("conversation".into(), id.clone())),
        Query(PreviewQuery {
            source_id: Some("other-conversation".into()),
            workspace_id: None
        })
    )
    .await
    .is_err());
    let resource = f
        .server
        .db
        .conversation_output(&conversation, &id)
        .unwrap()
        .unwrap()
        .resource_id
        .unwrap();
    let stored = f.server.resource_service.get(&resource).unwrap().unwrap();
    std::fs::write(stored.storage_path, b"changed content").unwrap();
    assert!(preview(
        State(f.server.clone()),
        Path(("conversation".into(), id)),
        Query(query())
    )
    .await
    .is_err());
}
#[tokio::test]
async fn integration_outputs_preview_rechecks_personal_grants_and_changed_file() {
    let _serial = PREVIEW_TEST_GATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let f = Fixture::new();
    let automation = f.personal("local-call");
    let query = || PreviewQuery {
        source_id: Some(automation.clone()),
        workspace_id: Some(f.workspace.id.to_string()),
    };
    assert!(preview(
        State(f.server.clone()),
        Path(("personal_file".into(), "local-call".into())),
        Query(query())
    )
    .await
    .is_err());
    f.grant(GrantEffect::Allow);
    assert_eq!(
        preview(
            State(f.server.clone()),
            Path(("personal_file".into(), "local-call".into())),
            Query(query())
        )
        .await
        .unwrap()
        .0["text"],
        "local report"
    );
    let deny = f.grant(GrantEffect::Deny);
    assert!(preview(
        State(f.server.clone()),
        Path(("personal_file".into(), "local-call".into())),
        Query(query())
    )
    .await
    .is_err());
    f.server.db.delete_grant(&deny).unwrap();
    std::fs::write(f.root.join("report.txt"), "modified").unwrap();
    assert!(preview(
        State(f.server.clone()),
        Path(("personal_file".into(), "local-call".into())),
        Query(query())
    )
    .await
    .is_err());
}
#[tokio::test]
async fn integration_outputs_artifact_preview_requires_current_grant_and_rejects_tampering() {
    let _serial = PREVIEW_TEST_GATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let f = Fixture::new();
    let id = f.artifact();
    let query = || PreviewQuery {
        source_id: None,
        workspace_id: Some(f.workspace.id.to_string()),
    };
    assert!(preview(
        State(f.server.clone()),
        Path(("artifact".into(), id.clone())),
        Query(query())
    )
    .await
    .is_err());
    f.grant(GrantEffect::Allow);
    assert!(preview(
        State(f.server.clone()),
        Path(("artifact".into(), id.clone())),
        Query(query())
    )
    .await
    .unwrap()
    .0["text"]
        .as_str()
        .unwrap()
        .contains("verified local text"));
    let deny = f.grant(GrantEffect::Deny);
    assert_eq!(
        download(
            State(f.server.clone()),
            Path(("artifact".into(), id.clone())),
            Query(query())
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST,
        "download must also honor permissions revoked after preview"
    );
    assert!(preview(
        State(f.server.clone()),
        Path(("artifact".into(), id.clone())),
        Query(query())
    )
    .await
    .is_err());
    f.server.db.delete_grant(&deny).unwrap();
    assert_eq!(
        download(
            State(f.server.clone()),
            Path(("artifact".into(), id.clone())),
            Query(query())
        )
        .await
        .status(),
        StatusCode::OK
    );
    let artifact = f
        .server
        .db
        .get_artifact(&ArtifactId::new(id).unwrap())
        .unwrap()
        .unwrap();
    std::fs::write(artifact.path.unwrap(), "tampered").unwrap();
    assert!(ArtifactService::new(f.server.db.clone())
        .read_bytes(&artifact.id, f.root.to_str().unwrap())
        .is_err());
}
