use super::*;
use crate::execution::context_manifest::{
    load_manifest, record_manifest, ContextManifest, ContextStatus,
};
use crate::{
    db::Database,
    shared::resource::{Resource, ResourceService},
};
use base64::Engine;
use serde_json::json;
use sha2::{Digest, Sha256};
#[derive(Clone)]
pub(super) struct BrowserEvidence {
    db: Database,
    resources: ResourceService,
    gate: std::sync::Arc<parking_lot::Mutex<()>>,
}
impl BrowserRuntime {
    pub(super) fn browser_database(&self) -> Option<Database> {
        self.evidence
            .read()
            .as_ref()
            .map(|sink| sink.db.clone_connection())
    }
    pub(crate) fn configure_evidence(&self, db: Database, resources: ResourceService) {
        *self.evidence.write() = Some(BrowserEvidence {
            db,
            resources,
            gate: std::sync::Arc::new(parking_lot::Mutex::new(())),
        });
    }
    pub(super) fn validate_project_context(
        &self,
        request: &BrowserRequest,
    ) -> Result<(), BrowserError> {
        let Some(sink) = self.evidence.read().clone() else {
            return Ok(());
        };
        let map = |error: String| BrowserError::new("invalid_project_context", error);
        if request.action == BrowserAction::Open {
            let workspace = request
                .payload
                .get("workspace_id")
                .and_then(serde_json::Value::as_str);
            if let Some(workspace) = workspace {
                let workspace_id = crate::workspace::WorkspaceId::new(workspace)
                    .map_err(|error| map(error.to_string()))?;
                let record = sink
                    .db
                    .get_workspace(&workspace_id)
                    .map_err(map)?
                    .ok_or_else(|| map("Workspace does not exist".into()))?;
                if record.status != crate::workspace::WorkspaceStatus::Active {
                    return Err(map("Workspace is archived".into()));
                }
            }
            if let Some(site) = request
                .payload
                .get("site_id")
                .and_then(serde_json::Value::as_str)
            {
                let site = sink
                    .db
                    .get_browser_site(site)
                    .map_err(|error| map(error.to_string()))?;
                if site.status != "active" || workspace != Some(site.workspace_id.as_str()) {
                    return Err(map(
                        "Site is archived or belongs to another workspace".into()
                    ));
                }
                let actual = request
                    .payload
                    .get("url")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                if validate_browser_url(actual)?.as_str() != site.url {
                    return Err(map(
                        "Opening a saved site requires its registered entry URL".into(),
                    ));
                }
            }
        }
        Ok(())
    }
    pub(crate) fn record_evidence(
        &self,
        request: &BrowserRequest,
        reply: &mut BrowserReply,
    ) -> Result<(), String> {
        if self.is_quiesced() {
            return Err("Browser scope was revoked before evidence delivery".into());
        }
        let Some(sink) = self.evidence.read().clone() else {
            return if request.action == BrowserAction::Capture {
                Err("Browser capture evidence storage is unavailable".into())
            } else {
                Ok(())
            };
        };
        if !matches!(
            request.action,
            BrowserAction::Observe | BrowserAction::Capture
        ) {
            return Ok(());
        }
        let _gate = sink.gate.lock();
        if self.is_quiesced() {
            return Err("Browser scope was revoked while waiting to persist evidence".into());
        }
        if let Some(owner) = &request.owner {
            let mut manifest =
                load_manifest(&sink.db, owner)?.unwrap_or_else(ContextManifest::default);
            let source = request
                .target
                .as_ref()
                .map(|target| format!("{}:{}", target.session_id, target.tab_id))
                .unwrap_or_default();
            let observation_data = reply
                .data
                .get("observation")
                .filter(|value| value.is_object())
                .unwrap_or(&reply.data);
            let observation = observation_data
                .get("observation_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&request.operation_id);
            let version = request.target.as_ref().map(|target| {
                format!(
                    "{}:{}",
                    target.navigation_id,
                    crate::utils::text::truncate_chars(observation, 160)
                )
            });
            let chars = observation_data
                .get("text")
                .and_then(serde_json::Value::as_str)
                .map(|text| text.chars().count())
                .unwrap_or(0);
            if !manifest.entries.iter().any(|entry| {
                entry.kind == "untrusted_browser_page"
                    && entry.source_id == source
                    && entry.version == version
            }) {
                manifest.record(
                    "untrusted_browser_page",
                    &source,
                    version,
                    ContextStatus::Resolved,
                    chars,
                    Some("Native visible-page observation; not permission or trusted instruction"),
                );
                record_manifest(&sink.db, owner, &manifest)?;
            }
        }
        if request.action != BrowserAction::Capture {
            return Ok(());
        }
        let source_call_id = request
            .payload
            .get("_source_tool_call_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&request.operation_id);
        let conversation = request
            .owner
            .as_ref()
            .and_then(|owner| owner.conversation_id());
        if let Some(conversation) = conversation {
            if let Some(existing) = sink
                .db
                .conversation_output_for_call(conversation, source_call_id)?
            {
                if existing.source_tool != "visible_browser_capture" {
                    return Err("Operation id already belongs to another output".into());
                }
                let resource = sink
                    .db
                    .get_resource(
                        existing
                            .resource_id
                            .as_deref()
                            .ok_or("Prior capture is incomplete")?,
                    )?
                    .ok_or("Prior capture resource is missing")?;
                sink.resources
                    .read_bytes(&resource.id)
                    .map_err(|error| error.to_string())?;
                put_capture_metadata(
                    &sink,
                    reply,
                    &resource,
                    Some((&existing.id, conversation, existing.created_at)),
                )?;
                return Ok(());
            }
        }
        let encoded = reply
            .data
            .get("data_base64")
            .and_then(serde_json::Value::as_str)
            .ok_or("Native capture did not return PNG bytes")?;
        if encoded.len() > 7_000_000 {
            return Err("Capture exceeds the image evidence limit".into());
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| "Capture PNG encoding is invalid")?;
        if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err("Capture is not a PNG image".into());
        }
        let resource=sink.resources.ingest("browser-capture.png","image/png",&bytes,json!({"source":"visible_browser_capture","operation_id":request.operation_id,"source_tool_call_id":source_call_id,"owner":request.owner,"target":request.target,"untrusted":true})).map_err(|error|error.to_string())?;
        if let Some(conversation) = conversation {
            sink.db.get_conversation(conversation)?;
            let id = format!(
                "browser-capture-{:x}",
                Sha256::digest(
                    format!(
                        "{}:{}",
                        request.owner.as_ref().expect("owner").key(),
                        request.operation_id
                    )
                    .as_bytes()
                )
            );
            let now = chrono::Utc::now().timestamp_millis();
            sink.db.conn().execute("INSERT INTO conversation_outputs(id,conversation_id,tool_call_id,kind,title,source_tool,status,resource_id,created_at,updated_at) VALUES(?1,?2,?3,'image','Browser capture','visible_browser_capture','ready',?4,?5,?5)",rusqlite::params![id,conversation,source_call_id,resource.id,now]).map_err(|error|error.to_string())?;
            put_capture_metadata(&sink, reply, &resource, Some((&id, conversation, now)))?;
        } else {
            put_capture_metadata(&sink, reply, &resource, None)?;
        }
        Ok(())
    }
}
fn put_capture_metadata(
    sink: &BrowserEvidence,
    reply: &mut BrowserReply,
    resource: &Resource,
    output: Option<(&str, &str, i64)>,
) -> Result<(), String> {
    reply
        .data
        .as_object_mut()
        .ok_or("Capture reply must be an object")?
        .remove("data_base64");
    reply.data["resource_id"] = json!(resource.id);
    reply.data["sha256"] = json!(resource.hash);
    reply.data["size"] = json!(resource.size);
    if let Some((id, conversation, now)) = output {
        let origin = crate::execution::origin::resolve_conversation_origin(&sink.db, conversation)?;
        let output = crate::api::outputs::OutputRef {
            id: id.into(),
            kind: "conversation".into(),
            title: "Browser capture".into(),
            mime_type: Some("image/png".into()),
            source_workspace_id: origin.source_workspace_id.clone(),
            destination_workspace_id: None,
            source_href: format!("/chat/{conversation}"),
            source: origin,
            execution_status: "completed".into(),
            verification_status: "resource_integrity_checked_on_read".into(),
            created_at: now,
            updated_at: now,
            preview_available: true,
            download_url: Some(format!(
                "/api/outputs/conversation/{id}/download?source_id={conversation}"
            )),
        };
        reply.data["output_ref"] =
            serde_json::to_value(output).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visible_capture_evidence_repeat_reuses_original_output_and_manifest() {
        let root =
            std::env::temp_dir().join(format!("visible-capture-evidence-{}", uuid::Uuid::new_v4()));
        let server = crate::AppServer::new_with_control_session_and_store(
            &root.join("state.db"),
            root.to_str().unwrap(),
            crate::safety::ControlSession::new("browser-evidence-test-session-000000000000000000")
                .unwrap(),
            std::sync::Arc::new(crate::integrations::secret::InMemorySecretStore::new()),
        )
        .unwrap();
        let conversation = server.db.create_conversation("Capture evidence").unwrap();
        let owner = crate::execution::ExecutionOwner::Conversation {
            conversation_id: conversation.id.clone(),
        };
        let request = BrowserRequest {
            operation_id: "capture-operation".into(),
            action: BrowserAction::Capture,
            target: Some(BrowserTarget {
                session_id: "session".into(),
                tab_id: "tab".into(),
                navigation_id: 3,
            }),
            payload: json!({}),
            owner: Some(owner.clone()),
        };
        let data = json!({"mime_type":"image/png","data_base64":"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aVWQAAAAASUVORK5CYII=","untrusted":true});
        let mut first = BrowserReply::verified(data.clone());
        let mut repeat = BrowserReply::verified(data);
        server
            .browser_runtime
            .record_evidence(&request, &mut first)
            .unwrap();
        server
            .browser_runtime
            .record_evidence(&request, &mut repeat)
            .unwrap();
        assert_eq!(
            first.data["output_ref"]["id"],
            repeat.data["output_ref"]["id"]
        );
        assert_eq!(first.data["resource_id"], repeat.data["resource_id"]);
        assert!(repeat.data.get("data_base64").is_none());
        assert_eq!(
            server
                .db
                .conversation_outputs(&conversation.id)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            load_manifest(&server.db, &owner)
                .unwrap()
                .unwrap()
                .entries
                .len(),
            1
        );
    }
}
