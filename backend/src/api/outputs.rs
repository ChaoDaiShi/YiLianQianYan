//! Unified read-only output catalogue. Each source keeps its own storage and checks.
use crate::{
    app::state::AppServer,
    db::Database,
    execution::origin::{self, OutputOrigin},
    modules::task::{
        artifact::{ArtifactService, ArtifactSource},
        ArtifactId,
    },
    safety::{
        grant::{GrantDecision, GrantEvaluator},
        PermissionId, ResourceDescriptor,
    },
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
pub struct OutputRef {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub mime_type: Option<String>,
    pub source_workspace_id: Option<String>,
    pub destination_workspace_id: Option<String>,
    pub source_href: String,
    pub source: OutputOrigin,
    pub execution_status: String,
    pub verification_status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub preview_available: bool,
    pub download_url: Option<String>,
}
impl OutputRef {
    fn in_workspace(&self, workspace: Option<&str>) -> bool {
        workspace.is_none_or(|id| {
            self.source_workspace_id.as_deref() == Some(id)
                || self.destination_workspace_id.as_deref() == Some(id)
        })
    }
}
#[derive(Default, Deserialize)]
pub struct ListQuery {
    pub workspace_id: Option<String>,
    pub limit: Option<usize>,
}
#[derive(Default, Deserialize)]
pub struct PreviewQuery {
    pub source_id: Option<String>,
    pub workspace_id: Option<String>,
}
fn error(message: String) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error":"output_unavailable","message":message})),
    )
}
fn segment(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

static OUTPUT_PREVIEWS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

fn artifact_output(db: &Database, id: &str) -> Result<OutputRef, String> {
    let artifact = db
        .get_artifact(&ArtifactId::new(id).map_err(|e| e.to_string())?)?
        .ok_or("成果不存在")?;
    let provenance = db.artifact_provenance(id)?;
    let mut source = if let Some(p) = &provenance {
        if let Some(snapshot) = &p.origin {
            snapshot.clone()
        } else {
            // Old provenance never captured the source space. The current graph
            // contract can have changed since creation, so it cannot backfill it.
            match &p.source {
                ArtifactSource::Task {
                    task_id,
                    execution_id,
                } => {
                    let mut source = OutputOrigin::new("legacy_task_artifact", task_id, None);
                    source.execution_id = Some(execution_id.clone());
                    source
                }
                ArtifactSource::Node {
                    graph_id,
                    node_id,
                    execution_id,
                    ..
                } => {
                    let mut source = OutputOrigin::new("legacy_node_artifact", graph_id, None);
                    source.graph_id = Some(graph_id.clone());
                    source.node_id = Some(node_id.clone());
                    source.execution_id = Some(execution_id.clone());
                    source
                }
            }
        }
    } else {
        // Old summary-only records retain their task identity; do not invent verification.
        origin::resolve_artifact_origin(
            db,
            &ArtifactSource::Task {
                task_id: artifact.task_id.to_string(),
                execution_id: artifact.task_execution_id.to_string(),
            },
        )
        .unwrap_or_else(|_| {
            let mut source = OutputOrigin::new("legacy_artifact", artifact.task_id.as_str(), None);
            source.execution_id = Some(artifact.task_execution_id.to_string());
            source
        })
    };
    source.destination_workspace_id = Some(artifact.workspace_id.to_string());
    let source_href = if let Some(graph) = &source.graph_id {
        format!("/task-world/{}", segment(graph))
    } else {
        format!("/tasks?task={}", segment(&source.id))
    };
    let execution_status = if let Some(graph) = &source.graph_id {
        db.conn()
            .query_row(
                "SELECT status FROM task_node_executions WHERE graph_id=?1 AND execution_id=?2",
                params![graph, source.execution_id],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| "unknown".into())
    } else {
        db.conn()
            .query_row(
                "SELECT status FROM task_executions WHERE id=?1",
                [source.execution_id.as_deref().unwrap_or("")],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| "unknown".into())
    };
    Ok(OutputRef {
        id: id.into(),
        kind: "artifact".into(),
        title: artifact.name,
        mime_type: artifact.mime_type,
        source_workspace_id: source.source_workspace_id.clone(),
        destination_workspace_id: source.destination_workspace_id.clone(),
        source_href,
        source,
        execution_status,
        verification_status: if provenance.is_some() {
            "accepted"
        } else {
            "not_recorded"
        }
        .into(),
        created_at: artifact.created_at,
        updated_at: artifact.updated_at,
        preview_available: artifact.path.is_some(),
        download_url: Some(format!("/api/outputs/artifact/{}/download", segment(id))),
    })
}

fn conversation_output(db: &Database, conversation: &str, id: &str) -> Result<OutputRef, String> {
    let output = db
        .conversation_output(conversation, id)?
        .ok_or("成果不属于此来源对话")?;
    let source = origin::resolve_conversation_origin(db, conversation)?;
    let mime = output
        .resource_id
        .as_ref()
        .map(|resource| {
            db.conn()
                .query_row(
                    "SELECT mime_type FROM resources WHERE id=?1",
                    [resource],
                    |r| r.get::<_, String>(0),
                )
                .optional()
                .map_err(|e| e.to_string())
        })
        .transpose()?
        .flatten();
    Ok(OutputRef {
        id: output.id,
        kind: "conversation".into(),
        title: output.title,
        mime_type: mime,
        source_workspace_id: source.source_workspace_id.clone(),
        destination_workspace_id: None,
        source_href: format!("/chat/{}", segment(conversation)),
        source,
        execution_status: output.status.clone(),
        verification_status: "resource_integrity_checked_on_read".into(),
        created_at: output.created_at,
        updated_at: output.updated_at,
        preview_available: output.status == "ready" && output.resource_id.is_some(),
        download_url: output.resource_id.as_ref().map(|_| {
            format!(
                "/api/outputs/conversation/{}/download?source_id={}",
                segment(id),
                segment(conversation)
            )
        }),
    })
}

fn personal_output(db: &Database, automation: &str, call: &str) -> Result<OutputRef, String> {
    let source = origin::resolve_personal_origin(db, automation, call)?;
    let (path,execution,verification,created,finished):(String,String,String,Option<i64>,Option<i64>)=db.conn().query_row("SELECT json_extract(args_json,'$.path'),execution_status,verification_status,started_at,finished_at FROM conversation_execution_records WHERE conversation_id=?1 AND tool_call_id=?2 AND name='write_file'",params![source.conversation_id,call],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(|e|e.to_string())?;
    let title = path
        .rsplit(['\\', '/'])
        .find(|part| !part.is_empty())
        .unwrap_or("生成文件")
        .to_string();
    Ok(OutputRef {
        id: call.into(),
        kind: "personal_file".into(),
        title,
        mime_type: Some("text/plain".into()),
        source_workspace_id: source.source_workspace_id.clone(),
        destination_workspace_id: None,
        source_href: format!(
            "/chat/{}",
            segment(source.conversation_id.as_deref().unwrap_or(""))
        ),
        source,
        execution_status: execution,
        verification_status: verification,
        created_at: created.unwrap_or(0),
        updated_at: finished.or(created).unwrap_or(0),
        preview_available: true,
        download_url: None,
    })
}

/// At most three bounded candidate pages are merged. No file is opened here.
pub fn catalogue(db: &Database, workspace: Option<&str>, limit: usize) -> Result<Value, String> {
    // Same idempotent product migrations used by the existing source APIs.
    db.initialize_artifact_provenance()?;
    db.initialize_proofloop_schema()
        .map_err(|e| e.to_string())?;
    let limit = limit.clamp(1, 200);
    let candidate_limit = (limit + 1) as i64;
    if let Some(id) = workspace {
        let exists: bool = db
            .conn()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id=?1)",
                [id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !exists {
            return Err("工作空间不存在".into());
        }
    }
    let artifact_ids = {
        let conn = db.conn();
        let mut query=conn.prepare("SELECT a.id FROM artifacts a LEFT JOIN artifact_provenance p ON p.artifact_id=a.id WHERE ?1 IS NULL OR a.workspace_id=?1 OR CASE WHEN json_valid(p.origin_json) THEN json_extract(p.origin_json,'$.source_workspace_id') ELSE NULL END=?1 ORDER BY a.created_at DESC,a.id DESC LIMIT ?2").map_err(|e|e.to_string())?;
        let rows = query
            .query_map(params![workspace, candidate_limit], |r| {
                r.get::<_, String>(0)
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };
    let conversations = {
        let conn = db.conn();
        let mut query=conn.prepare("SELECT o.conversation_id,o.id FROM conversation_outputs o LEFT JOIN settings s ON s.key='conversation_workspace:'||o.conversation_id WHERE ?1 IS NULL OR s.value=?1 ORDER BY o.created_at DESC,o.id DESC LIMIT ?2").map_err(|e|e.to_string())?;
        let rows = query
            .query_map(params![workspace, candidate_limit], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };
    let personal = {
        let conn = db.conn();
        let mut query=conn.prepare("SELECT a.id,r.tool_call_id FROM automations a JOIN conversation_execution_records r ON r.conversation_id=json_extract(CASE WHEN json_valid(a.payload) THEN a.payload ELSE '{}' END,'$.target.conversation_id') WHERE json_extract(CASE WHEN json_valid(a.payload) THEN a.payload ELSE '{}' END,'$.target.kind')='personal_assistant' AND r.name='write_file' AND r.execution_status='succeeded' AND (?1 IS NULL OR json_extract(CASE WHEN json_valid(a.payload) THEN a.payload ELSE '{}' END,'$.workspace_id')=?1) ORDER BY r.started_at DESC,r.tool_call_id DESC,a.id DESC LIMIT ?2").map_err(|e|e.to_string())?;
        let rows = query
            .query_map(params![workspace, candidate_limit], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };
    let mut outputs = Vec::new();
    let mut issues = Vec::new();
    let candidates_truncated =
        artifact_ids.len() > limit || conversations.len() > limit || personal.len() > limit;
    // A damaged source remains visible as a bounded diagnostic; valid siblings
    // remain usable. No failed source is promoted into a previewable output.
    for id in artifact_ids {
        match artifact_output(db, &id) {Ok(output)=>outputs.push(output),Err(error)=>issues.push(json!({"kind":"artifact","id":id,"error":crate::utils::text::truncate_chars(&error,500)}))}
    }
    for (conversation, id) in conversations {
        match conversation_output(db, &conversation, &id) {Ok(output)=>outputs.push(output),Err(error)=>issues.push(json!({"kind":"conversation","id":id,"error":crate::utils::text::truncate_chars(&error,500)}))}
    }
    for (automation, call) in personal {
        match personal_output(db, &automation, &call) {Ok(output)=>outputs.push(output),Err(error)=>issues.push(json!({"kind":"personal_file","id":call,"error":crate::utils::text::truncate_chars(&error,500)}))}
    }
    outputs.retain(|item| item.in_workspace(workspace));
    outputs.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.source.id.cmp(&b.source.id))
    });
    let truncated = outputs.len() > limit || candidates_truncated;
    outputs.truncate(limit);
    issues.truncate(limit);
    Ok(json!({"outputs":outputs,"issues":issues,"limit":limit,"truncated":truncated}))
}

pub async fn list(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    catalogue(
        &server.db,
        query.workspace_id.as_deref(),
        query.limit.unwrap_or(50),
    )
    .map(Json)
    .map_err(error)
}

fn authorize_artifact_read(server: &AppServer, path: &str) -> Result<(), String> {
    let real = std::path::Path::new(path)
        .canonicalize()
        .map_err(|_| "成果文件不可用")?;
    let grants = server
        .db
        .list_grants("local-user")
        .map_err(|e| e.to_string())?;
    if !matches!(
        GrantEvaluator::new(grants, &server.workspace_root).evaluate(
            PermissionId::FilesystemRead,
            &ResourceDescriptor::File {
                path: real.to_string_lossy().into_owned()
            },
            chrono::Utc::now().timestamp_millis()
        ),
        GrantDecision::Allow
    ) {
        return Err("当前文件读取权限不允许预览；成果记录不会授予读取权限".into());
    }
    Ok(())
}

pub async fn preview(
    State(server): State<Arc<AppServer>>,
    Path(identity): Path<(String, String)>,
    Query(query): Query<PreviewQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let permit = OUTPUT_PREVIEWS.try_acquire().map_err(|_| {
        (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error":"output_busy","message":"成果预览繁忙，请稍后重试"})),
        )
    })?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        preview_read(State(server), Path(identity), Query(query))
    })
    .await
    .map_err(|_| error("成果解析未能完成，请重试".into()))?
}

fn preview_read(
    State(server): State<Arc<AppServer>>,
    Path((kind, id)): Path<(String, String)>,
    Query(query): Query<PreviewQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let output = match kind.as_str() {
        "artifact" => artifact_output(&server.db, &id),
        "conversation" => conversation_output(
            &server.db,
            query
                .source_id
                .as_deref()
                .ok_or_else(|| error("缺少来源对话".into()))?,
            &id,
        ),
        "personal_file" => personal_output(
            &server.db,
            query
                .source_id
                .as_deref()
                .ok_or_else(|| error("缺少来源委托".into()))?,
            &id,
        ),
        _ => Err("未知成果来源类型".into()),
    }
    .map_err(error)?;
    if !output.in_workspace(query.workspace_id.as_deref()) {
        return Err(error("成果不属于所请求的工作空间".into()));
    }
    if !output.preview_available {
        return Err(error("此成果尚无可预览的实际内容".into()));
    }
    let value = match kind.as_str() {
        "personal_file" => {
            let value = crate::modules::automation::personal::management::preview(
                &server,
                output.source.automation_id.as_deref().unwrap_or(""),
                &id,
            )
            .map_err(error)?;
            json!({"output":output,"text":value["content"],"truncated":value["truncated"]})
        }
        "conversation" => {
            let stored = server
                .db
                .conversation_output(output.source.conversation_id.as_deref().unwrap_or(""), &id)
                .map_err(error)?
                .ok_or_else(|| error("来源成果已移除".into()))?;
            let resource = stored
                .resource_id
                .ok_or_else(|| error("成果资源尚不可用".into()))?;
            let preview =
                crate::modules::resource::resource_preview(&server.resource_service, &resource)
                    .map_err(error)?;
            if preview.status == "failed" {
                return Err(error(
                    preview.error.unwrap_or_else(|| "成果格式解析失败".into()),
                ));
            }
            json!({"output":output,"text":preview.text,"truncated":preview.truncated,"resource_preview":preview})
        }
        "artifact" => {
            let artifact = server
                .db
                .get_artifact(&ArtifactId::new(&id).map_err(|e| error(e.to_string()))?)
                .map_err(error)?
                .ok_or_else(|| error("成果不存在".into()))?;
            authorize_artifact_read(&server, artifact.path.as_deref().unwrap_or(""))
                .map_err(error)?;
            let (artifact, bytes) = ArtifactService::new(server.db.clone())
                .read_bytes(
                    &ArtifactId::new(&id).map_err(|e| error(e.to_string()))?,
                    &server.workspace_root,
                )
                .map_err(|e| error(e.to_string()))?;
            let preview = crate::modules::resource::extract_preview(
                // Artifact.name is a human title and need not have an extension.
                // MIME validation uses the real persisted file name, as the
                // source reader has already checked its path/hash/permissions.
                artifact
                    .path
                    .as_deref()
                    .and_then(|path| std::path::Path::new(path).file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or(&artifact.name),
                artifact
                    .mime_type
                    .as_deref()
                    .unwrap_or("application/octet-stream"),
                &bytes,
            );
            if preview.status == "failed" {
                return Err(error(
                    preview.error.unwrap_or_else(|| "成果格式解析失败".into()),
                ));
            }
            json!({"output":output,"text":preview.text,"truncated":preview.truncated,"resource_preview":preview})
        }
        _ => unreachable!(),
    };
    Ok(Json(value))
}

/// The unified surface never falls back to a less restrictive legacy download.
pub async fn download(
    State(server): State<Arc<AppServer>>,
    Path((kind, id)): Path<(String, String)>,
    Query(query): Query<PreviewQuery>,
) -> Response {
    let permit = match OUTPUT_PREVIEWS.try_acquire() {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({"message":"成果读取繁忙，请稍后重试"})),
            )
                .into_response()
        }
    };
    match tokio::task::spawn_blocking(
        move || -> Result<(String, Vec<u8>), (StatusCode, Json<Value>)> {
            let _permit = permit;
            let output = match kind.as_str() {
                "artifact" => artifact_output(&server.db, &id),
                "conversation" => conversation_output(
                    &server.db,
                    query
                        .source_id
                        .as_deref()
                        .ok_or_else(|| error("缺少来源对话".into()))?,
                    &id,
                ),
                _ => Err("此来源只支持有界文本预览".into()),
            }
            .map_err(error)?;
            if !output.in_workspace(query.workspace_id.as_deref()) {
                return Err(error("成果不属于所请求空间".into()));
            }
            let bytes = if kind == "artifact" {
                let artifact = server
                    .db
                    .get_artifact(&ArtifactId::new(&id).map_err(|e| error(e.to_string()))?)
                    .map_err(error)?
                    .ok_or_else(|| error("成果不存在".into()))?;
                authorize_artifact_read(&server, artifact.path.as_deref().unwrap_or(""))
                    .map_err(error)?;
                ArtifactService::new(server.db.clone())
                    .read_bytes(&artifact.id, &server.workspace_root)
                    .map_err(|e| error(e.to_string()))?
                    .1
            } else {
                let stored = server
                    .db
                    .conversation_output(
                        output.source.conversation_id.as_deref().unwrap_or(""),
                        &id,
                    )
                    .map_err(error)?
                    .ok_or_else(|| error("来源成果已移除".into()))?;
                let resource = stored
                    .resource_id
                    .ok_or_else(|| error("成果资源尚不可用".into()))?;
                server
                    .resource_service
                    .read_bytes(&resource)
                    .map_err(|_| error("资源完整性检查失败".into()))?
            };
            Ok((
                output
                    .mime_type
                    .unwrap_or_else(|| "application/octet-stream".into()),
                bytes,
            ))
        },
    )
    .await
    {
        Ok(Ok((mime, bytes))) => (
            [
                (axum::http::header::CONTENT_TYPE, mime),
                (axum::http::header::CONTENT_DISPOSITION, "attachment".into()),
                (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
            ],
            bytes,
        )
            .into_response(),
        Ok(Err(error)) => error.into_response(),
        Err(_) => error("成果读取未能完成".into()).into_response(),
    }
}

#[cfg(test)]
mod tests;
