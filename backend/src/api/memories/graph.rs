use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{governance_error, storage_error, MemoryWithSource, StorageApiError};
use crate::{
    app::state::AppServer,
    db::{
        memory_candidates::ExtractedMemoryRelationship,
        memory_governance::MemoryVersion,
        memory_graph::{
            ChangeMemoryLink, CreateMemoryLink, GraphScope, MemoryChangeEvent, MemoryGraphSemantic,
            MemoryLink, MemoryLinkVersion, StageLinkCorrection,
        },
        MemoryQuery,
    },
};

pub(crate) fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route("/api/memories/graph", get(graph))
        .route("/api/memories/relations", post(create))
        .route("/api/memories/relations/:id", patch(change))
        .route(
            "/api/memories/relations/:id/correction-candidate",
            post(stage_correction),
        )
}

#[derive(Deserialize)]
struct GraphQuery {
    category: Option<String>,
    source: Option<String>,
    q: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    workspace_id: Option<String>,
    conversation_id: Option<String>,
    #[serde(default)]
    withdrawn: bool,
}

#[derive(Serialize)]
struct GraphResponse {
    memories: Vec<MemoryWithSource>,
    histories: HashMap<String, Vec<MemoryVersion>>,
    relations: Vec<MemoryLink>,
    relation_versions: Vec<MemoryLinkVersion>,
    next_offset: Option<usize>,
    semantic: MemoryGraphSemantic,
    extracted_relations: Vec<ExtractedMemoryRelationship>,
    current_version_ids: HashMap<String, String>,
    change_events: Vec<MemoryChangeEvent>,
}

async fn graph(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<GraphQuery>,
) -> Result<Json<GraphResponse>, StorageApiError> {
    let embedding_model = {
        let config = server.config.read();
        config
            .model
            .has_embedding()
            .then(|| crate::db::memory_governance::embedding_identity(&config.model))
    };
    let snapshot = server
        .db
        .scoped_memory_graph(
            MemoryQuery {
                category: query.category,
                source: query.source,
                q: query.q,
                limit: query.limit,
                offset: query.offset,
            },
            query.withdrawn,
            embedding_model.as_deref(),
            &GraphScope {
                workspace_id: query.workspace_id,
                conversation_id: query.conversation_id,
            },
        )
        .map_err(link_error)?;
    Ok(Json(GraphResponse {
        memories: snapshot
            .memories
            .into_iter()
            .zip(snapshot.source_removed)
            .map(|(memory, source_removed)| MemoryWithSource {
                memory,
                source_removed,
            })
            .collect(),
        histories: snapshot.histories,
        relations: snapshot.relations,
        relation_versions: snapshot.relation_versions,
        next_offset: snapshot.next_offset,
        semantic: snapshot.semantic,
        extracted_relations: snapshot.extracted_relations,
        current_version_ids: snapshot.current_version_ids,
        change_events: snapshot.change_events,
    }))
}

async fn create(
    State(server): State<Arc<AppServer>>,
    Json(input): Json<CreateMemoryLink>,
) -> Result<Json<MemoryLink>, StorageApiError> {
    server
        .db
        .create_memory_link(input)
        .map(Json)
        .map_err(link_error)
}

async fn change(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<ChangeMemoryLink>,
) -> Result<Json<MemoryLink>, StorageApiError> {
    server
        .db
        .change_memory_link(&id, input)
        .map(Json)
        .map_err(link_error)
}

async fn stage_correction(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<StageLinkCorrection>,
) -> Result<Json<Value>, StorageApiError> {
    let candidate = server
        .db
        .stage_link_correction(&id, input)
        .map_err(link_error)?;
    Ok(Json(json!({"candidate_id":candidate})))
}

fn link_error(code: String) -> StorageApiError {
    let (status, message) = match code.as_str() {
        "memory_link_not_found" => (StatusCode::NOT_FOUND, "这条对应关系不存在"),
        "memory_graph_scope_unavailable" => (
            StatusCode::FORBIDDEN,
            "此工作空间或对话不在当前档案的可用范围内",
        ),
        "invalid_memory_link" => (
            StatusCode::BAD_REQUEST,
            "请选择不同的记忆，并填写有效的关系名称",
        ),
        "memory_link_inactive_endpoint" => (StatusCode::CONFLICT, "对应关系的两条记忆必须仍然有效"),
        "memory_link_exists" => (StatusCode::CONFLICT, "两条记忆已经建立对应关系"),
        "memory_link_not_confirmed" => {
            (StatusCode::CONFLICT, "只有已确认的对应关系才能提交关联纠正")
        }
        "memory_link_source_version_unavailable" => {
            (StatusCode::CONFLICT, "所选来源版本已失效或不在同一范围")
        }
        "memory_link_revision_conflict" => (StatusCode::CONFLICT, "对应关系已变化，请重新查看"),
        _ => return governance_error(code),
    };
    (status, Json(json!({"code":code,"error":message})))
}
