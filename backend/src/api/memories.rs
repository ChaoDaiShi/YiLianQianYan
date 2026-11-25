// ============================================================
// Memory API handlers — long-term AI memory CRUD + reserved endpoints
// ============================================================

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::app::state::AppServer;
use crate::db::{
    CreateMemoryRequest, Memory, MemoryQuery, MemoryStats, RetrieveQuery, UpdateMemoryRequest,
};
use crate::integrations::llm::client::LlmClient;

pub(crate) mod indexing;

pub type StorageApiError = (StatusCode, Json<serde_json::Value>);

fn governance_error(error: String) -> StorageApiError {
    let status = match error.as_str() {
        "memory_revision_conflict"
        | "stale_embedding_result"
        | "memory_version_requires_reviewed_correction"
        | "knowledge_document_owned_chunk" => StatusCode::CONFLICT,
        "feedback_source_unavailable" | "memory_version_unavailable" => StatusCode::NOT_FOUND,
        "invalid_memory_correction"
        | "invalid_memory_scope"
        | "invalid_memory_reason"
        | "sensitive_memory_content"
        | "invalid_memory_content"
        | "invalid_memory_category"
        | "invalid_memory_source" => StatusCode::BAD_REQUEST,
        _ => return storage_error(error),
    };
    (
        status,
        Json(serde_json::json!({"code":error,"error":error})),
    )
}

pub async fn governance_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<crate::db::memory_governance::MemoryGovernance>, StorageApiError> {
    if server.db.find_memory(&id).map_err(storage_error)?.is_none() {
        return Err(not_found_error("记忆"));
    }
    server
        .db
        .memory_governance(&id)
        .map(Json)
        .map_err(storage_error)
}

pub async fn history_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<crate::db::memory_governance::MemoryVersion>>, StorageApiError> {
    if server.db.find_memory(&id).map_err(storage_error)?.is_none() {
        return Err(not_found_error("记忆"));
    }
    server
        .db
        .memory_history(&id)
        .map(Json)
        .map_err(storage_error)
}

pub async fn correct_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(req): Json<crate::db::memory_governance::CorrectMemoryRequest>,
) -> Result<Json<MemoryWithSource>, StorageApiError> {
    let memory = server
        .db
        .correct_memory(&id, &req)
        .map_err(governance_error)?;
    embed_saved_memory(&server, &memory).await;
    Ok(Json(single_with_source_status(&server.db, memory)?))
}

pub async fn withdraw_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(req): Json<crate::db::memory_governance::WithdrawMemoryRequest>,
) -> Result<Json<crate::db::memory_governance::MemoryGovernance>, StorageApiError> {
    server
        .db
        .withdraw_memory(&id, &req)
        .map(Json)
        .map_err(governance_error)
}

pub async fn restore_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(req): Json<crate::db::memory_governance::RestoreMemoryVersion>,
) -> Result<Json<MemoryWithSource>, StorageApiError> {
    let memory = server
        .db
        .restore_memory_version(&id, &req)
        .map_err(governance_error)?;
    embed_saved_memory(&server, &memory).await;
    Ok(Json(single_with_source_status(&server.db, memory)?))
}

pub async fn withdrawn_handler(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<Vec<MemoryWithSource>>, StorageApiError> {
    let entries = server.db.withdrawn_memories().map_err(storage_error)?;
    Ok(Json(with_source_status(&server.db, entries)?))
}

#[derive(Debug, serde::Serialize)]
pub struct MemoryWithSource {
    #[serde(flatten)]
    memory: Memory,
    source_removed: bool,
}

fn with_source_status(
    db: &crate::db::Database,
    mut entries: Vec<Memory>,
) -> Result<Vec<MemoryWithSource>, StorageApiError> {
    let statuses = db
        .memory_source_status(&mut entries)
        .map_err(|error| storage_error(error.to_string()))?;
    Ok(entries
        .into_iter()
        .zip(statuses)
        .map(|(memory, source_removed)| MemoryWithSource {
            memory,
            source_removed,
        })
        .collect())
}

fn single_with_source_status(
    db: &crate::db::Database,
    mut memory: Memory,
) -> Result<MemoryWithSource, StorageApiError> {
    let statuses = db
        .memory_source_status(std::slice::from_mut(&mut memory))
        .map_err(|error| storage_error(error.to_string()))?;
    Ok(MemoryWithSource {
        memory,
        source_removed: statuses[0],
    })
}

fn storage_error(_: String) -> StorageApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "code": "local_storage_unavailable",
            "error": "本地数据暂时不可用，请重试"
        })),
    )
}

fn not_found_error(entity: &str) -> StorageApiError {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "code": "not_found",
            "error": format!("{entity}不存在")
        })),
    )
}

fn document_owned_chunk() -> StorageApiError {
    (
        StatusCode::CONFLICT,
        Json(serde_json::json!({
            "code": "knowledge_document_owned_chunk",
            "error": "该片段由知识文档管理，请在知识库中重新导入或移除源文档"
        })),
    )
}

// ── List / Search ──

pub async fn list_handler(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<MemoryQuery>,
) -> Result<Json<Vec<MemoryWithSource>>, StorageApiError> {
    // Log query params for debugging
    if query.q.is_some() || query.category.is_some() || query.source.is_some() {
        tracing::debug!(
            "Memory query: q={:?} cat={:?} src={:?}",
            query.q,
            query.category,
            query.source
        );
    }
    let entries = server.db.list_memories(&query).map_err(storage_error)?;
    Ok(Json(with_source_status(&server.db, entries)?))
}

pub async fn library_handler(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<MemoryQuery>,
) -> Result<Json<Vec<MemoryWithSource>>, StorageApiError> {
    let entries = server
        .db
        .list_memory_library(&query)
        .map_err(storage_error)?;
    Ok(Json(with_source_status(&server.db, entries)?))
}

#[derive(serde::Deserialize)]
pub struct ActivityQuery {
    #[serde(default)]
    recent: bool,
    offset: Option<usize>,
    q: Option<String>,
}

pub async fn activity_handler(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<ActivityQuery>,
) -> Result<Json<serde_json::Value>, StorageApiError> {
    const PAGE_SIZE: usize = 40;
    let offset = query.offset.unwrap_or(0);
    let mut entries = server
        .db
        .search_memory_activity(
            query.recent,
            query
                .q
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
            PAGE_SIZE + 1,
            offset,
        )
        .map_err(|error| storage_error(error.to_string()))?;
    let next_offset = (entries.len() > PAGE_SIZE).then(|| offset.saturating_add(PAGE_SIZE));
    entries.truncate(PAGE_SIZE);
    Ok(Json(
        serde_json::json!({"entries": entries, "next_offset": next_offset}),
    ))
}

pub async fn remove_activity_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StorageApiError> {
    let removed = server
        .db
        .remove_memory_activity(&id)
        .map_err(|error| storage_error(error.to_string()))?;
    if !removed {
        return Err(not_found_error("记忆日志"));
    }
    Ok(Json(serde_json::json!({"status":"deleted"})))
}

// ── Get by ID ──

pub async fn get_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<MemoryWithSource>, StorageApiError> {
    match server.db.find_memory(&id) {
        Ok(Some(mem)) => Ok(Json(single_with_source_status(&server.db, mem)?)),
        Ok(None) => Err(not_found_error("记忆")),
        Err(e) => Err(storage_error(e)),
    }
}

// ── Create ──

pub async fn create_handler(
    State(server): State<Arc<AppServer>>,
    Json(req): Json<CreateMemoryRequest>,
) -> Result<Json<Memory>, StorageApiError> {
    let mem = server.db.create_memory(&req).map_err(governance_error)?;
    embed_saved_memory(&server, &mem).await;
    Ok(Json(mem))
}

async fn embed_saved_memory(server: &AppServer, mem: &Memory) {
    let model_cfg = server.config.read().model.clone();
    if model_cfg.has_local_embedding() {
        let _ = server.memory_indexer.start(server);
        return;
    }
    if !crate::modules::onboarding::allowed(&server.db, "external_model") {
        return;
    }
    if model_cfg.has_embedding() {
        let llm = LlmClient::new(&model_cfg, Arc::clone(&server.secret_resolver))
            .with_profile_permissions(server.db.clone_connection());
        let ticket = match server.db.begin_memory_embedding(
            mem,
            &crate::db::memory_governance::embedding_identity(&model_cfg),
        ) {
            Ok(ticket) => ticket,
            Err(error) => {
                tracing::warn!(memory_id = %mem.id, %error, "memory persisted without a new embedding ticket");
                return;
            }
        };
        match llm.embed(&mem.content).await {
            Ok(vec) => {
                if let Err(e) = server.db.complete_memory_embedding(&ticket, &vec) {
                    tracing::warn!(memory_id = %mem.id, error = %e, "failed to persist embedding");
                }
            }
            Err(e) => {
                tracing::warn!(memory_id = %mem.id, error = %e, "embedding generation failed, memory persisted without embedding");
            }
        }
    }
}

// ── Update ──

#[derive(serde::Deserialize)]
pub struct GuardedMemoryUpdate {
    expected_revision: Option<i64>,
    #[serde(flatten)]
    change: UpdateMemoryRequest,
}

pub async fn update_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(req): Json<GuardedMemoryUpdate>,
) -> Result<Json<MemoryWithSource>, StorageApiError> {
    if server
        .db
        .knowledge_chunk_document(&id)
        .map_err(storage_error)?
        .is_some()
    {
        return Err(document_owned_chunk());
    }
    match server.db.find_memory(&id) {
        Ok(Some(_)) => {
            let memory = server
                .db
                .update_memory_at_revision(&id, &req.change, req.expected_revision)
                .map_err(governance_error)?;
            embed_saved_memory(&server, &memory).await;
            Ok(Json(single_with_source_status(&server.db, memory)?))
        }
        Ok(None) => Err(not_found_error("记忆")),
        Err(e) => Err(storage_error(e)),
    }
}

// ── Delete ──

pub async fn delete_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StorageApiError> {
    if server
        .db
        .knowledge_chunk_document(&id)
        .map_err(storage_error)?
        .is_some()
    {
        return Err(document_owned_chunk());
    }
    server
        .db
        .delete_memory(&id)
        .map(|_| Json(serde_json::json!({"status": "deleted"})))
        .map_err(storage_error)
}

// ── Stats ──

pub async fn stats_handler(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<MemoryStats>, StorageApiError> {
    server
        .db
        .get_memory_stats()
        .map(Json)
        .map_err(storage_error)
}

// ── Retrieve (ranked keyword retrieval, hybrid when embedding is available) ──

pub async fn retrieve_handler(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<RetrieveQuery>,
) -> Result<Json<serde_json::Value>, StorageApiError> {
    let q = query.q.trim().to_string();
    if q.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "code": "invalid_query",
                "error": "query parameter 'q' is required and must not be empty"
            })),
        ));
    }

    let model_cfg = server.config.read().model.clone();
    let mut mode = "lexical";
    let mut query_embedding: Option<Vec<f32>> = None;

    if model_cfg.has_embedding()
        && (model_cfg.has_local_embedding()
            || crate::modules::onboarding::allowed(&server.db, "external_model"))
    {
        let llm = LlmClient::new(&model_cfg, Arc::clone(&server.secret_resolver))
            .with_profile_permissions(server.db.clone_connection());
        match llm.embed_query(&q).await {
            Ok(vec) => {
                query_embedding = Some(vec);
                mode = "hybrid";
            }
            Err(e) => {
                tracing::warn!(error = %e, "query embedding failed, falling back to lexical");
                mode = "lexical_fallback";
            }
        }
    }

    match server.db.retrieve_scoped_memories(
        &query,
        query_embedding.as_deref(),
        Some(&crate::db::memory_governance::embedding_identity(
            &model_cfg,
        )),
        None,
        true,
    ) {
        Ok(scored) => Ok(Json(serde_json::json!({
            "query": q,
            "count": scored.len(),
            "mode": mode,
            "memories": scored,
        }))),
        Err(e) => Err(storage_error(e)),
    }
}

pub(crate) mod candidates;
pub(crate) mod graph;

// ── Reserved: Batch-import ──

pub async fn batch_import_handler(
    State(_server): State<Arc<AppServer>>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StorageApiError> {
    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "code": "not_implemented",
            "error": "Batch import endpoint — reserved for future use"
        })),
    ))
}

// ── Reserved: Batch-delete ──

pub async fn batch_delete_handler(
    State(_server): State<Arc<AppServer>>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StorageApiError> {
    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "code": "not_implemented",
            "error": "Batch delete endpoint — reserved for future use"
        })),
    ))
}

// ── Reserved: Export ──

pub async fn export_handler(
    State(_server): State<Arc<AppServer>>,
) -> Result<Json<serde_json::Value>, StorageApiError> {
    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "code": "not_implemented",
            "error": "Export endpoint — reserved for future use"
        })),
    ))
}

// ── Reserved: Merge duplicates ──

pub async fn merge_handler(
    State(_server): State<Arc<AppServer>>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StorageApiError> {
    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "code": "not_implemented",
            "error": "Merge duplicates endpoint — reserved for future use"
        })),
    ))
}

// ── Reindex embeddings (historical backfill) ──

const DEFAULT_REINDEX_LIMIT: usize = 50;
const MAX_REINDEX_LIMIT: usize = 200;

#[derive(serde::Deserialize)]
pub struct ReindexRequest {
    #[serde(default = "default_reindex_limit")]
    pub limit: usize,
}

fn default_reindex_limit() -> usize {
    DEFAULT_REINDEX_LIMIT
}

pub async fn reindex_handler(
    State(server): State<Arc<AppServer>>,
    Json(req): Json<ReindexRequest>,
) -> Result<Json<serde_json::Value>, StorageApiError> {
    // Clamp limit: 1..=MAX_REINDEX_LIMIT
    let requested = req.limit.clamp(1, MAX_REINDEX_LIMIT);

    // Reject when embedding provider is not configured
    let config = server.config.read().clone();
    if !config.model.has_embedding() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "code": "embedding_not_configured",
                "error": "embedding provider is not configured"
            })),
        ));
    }
    if !config.model.has_local_embedding()
        && !crate::modules::onboarding::allowed(&server.db, "external_model")
    {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                serde_json::json!({"code":"external_model_disabled","error":"当前资料档案已关闭外部模型调用"}),
            ),
        ));
    }

    // Load memories missing embedding
    let missing = match server.db.memories_needing_embedding(
        &crate::db::memory_governance::embedding_identity(&config.model),
        requested,
    ) {
        Ok(memories) => memories,
        Err(e) => {
            tracing::error!("reindex: failed to load missing-embedding memories: {e}");
            return Err(storage_error(e));
        }
    };

    let processed = missing.len();
    let mut succeeded = 0usize;
    let mut failed = 0usize;

    let llm_client = LlmClient::new(&config.model, Arc::clone(&server.secret_resolver))
        .with_profile_permissions(server.db.clone_connection());

    for memory in &missing {
        let ticket = match server.db.begin_memory_embedding(
            memory,
            &crate::db::memory_governance::embedding_identity(&config.model),
        ) {
            Ok(ticket) => ticket,
            Err(_) => {
                failed += 1;
                continue;
            }
        };
        match llm_client.embed(&memory.content).await {
            Ok(vector) => match server.db.complete_memory_embedding(&ticket, &vector) {
                Ok(()) => {
                    succeeded += 1;
                    tracing::debug!(
                        memory_id = %memory.id,
                        dim = vector.len(),
                        "reindex: embedded memory"
                    );
                }
                Err(e) => {
                    failed += 1;
                    tracing::warn!(
                        memory_id = %memory.id,
                        error = %e,
                        "reindex: failed to persist embedding"
                    );
                }
            },
            Err(e) => {
                failed += 1;
                tracing::warn!(
                    memory_id = %memory.id,
                    error = %e,
                    "reindex: embedding request failed"
                );
            }
        }
    }

    let remaining = match server.db.memories_needing_embedding(
        &crate::db::memory_governance::embedding_identity(&config.model),
        i64::MAX as usize,
    ) {
        Ok(memories) => memories.len(),
        Err(e) => {
            tracing::error!("reindex: failed to count remaining memories: {e}");
            return Err(storage_error(e));
        }
    };

    Ok(Json(serde_json::json!({
        "ok": true,
        "requested": requested,
        "processed": processed,
        "succeeded": succeeded,
        "failed": failed,
        "remaining": remaining,
    })))
}

#[cfg(test)]
mod companion_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::AppServer;
    use crate::db::MessageRow;
    use crate::safety::ControlSession;

    fn test_server(label: &str) -> (Arc<AppServer>, std::path::PathBuf, String) {
        let (db_path, server) = {
            let path = std::env::temp_dir().join(format!(
                "yilian-memory-server-{label}-{}.db",
                uuid::Uuid::new_v4()
            ));
            let s = AppServer::new_with_control_session(&path, ".", ControlSession::generate())
                .unwrap();
            (path, Arc::new(s))
        };
        // Create a conversation so foreign keys on source_conversation_id work
        let conv = server.db.create_conversation("test conversation").unwrap();
        (server, db_path, conv.id)
    }

    #[tokio::test]
    async fn imported_knowledge_chunks_reject_memory_mutation_while_manual_memories_remain_editable(
    ) {
        let (server, _db_path, _) = test_server("document-chunk-ownership");
        let (document, _) = server
            .db
            .import_knowledge(
                "test-resource",
                "source.txt",
                "text/plain",
                "document-guard-test-hash",
                22,
                "canonical original document",
            )
            .unwrap();
        let chunks = server
            .db
            .knowledge_chunks(Some(&document.id), None)
            .unwrap();
        let chunk = &chunks[0];
        let update: GuardedMemoryUpdate =
            serde_json::from_value(serde_json::json!({"content":"must not overwrite"})).unwrap();
        let rejected = update_handler(
            State(server.clone()),
            Path(chunk.memory_id.clone()),
            Json(update),
        )
        .await
        .unwrap_err();
        assert_eq!(rejected.0, StatusCode::CONFLICT);
        assert_eq!(rejected.1 .0["code"], "knowledge_document_owned_chunk");
        let rejected = delete_handler(State(server.clone()), Path(chunk.memory_id.clone()))
            .await
            .unwrap_err();
        assert_eq!(rejected.0, StatusCode::CONFLICT);
        assert_eq!(
            server
                .db
                .find_memory(&chunk.memory_id)
                .unwrap()
                .unwrap()
                .content,
            "canonical original document"
        );
        assert_eq!(
            server
                .db
                .knowledge_chunks(Some(&document.id), None)
                .unwrap()
                .len(),
            chunks.len()
        );

        let manual = server
            .db
            .create_memory(&CreateMemoryRequest {
                content: "manual knowledge".into(),
                category: "knowledge".into(),
                source: "manual".into(),
                source_conversation_id: None,
                metadata: None,
            })
            .unwrap();
        let update: GuardedMemoryUpdate =
            serde_json::from_value(serde_json::json!({"content":"updated manual knowledge"}))
                .unwrap();
        let updated = update_handler(State(server.clone()), Path(manual.id.clone()), Json(update))
            .await
            .unwrap();
        assert_eq!(updated.0.memory.content, "updated manual knowledge");
        let stale = serde_json::from_value(
            serde_json::json!({"content":"stale update","expected_revision":1}),
        )
        .unwrap();
        let rejected = update_handler(State(server.clone()), Path(manual.id.clone()), Json(stale))
            .await
            .unwrap_err();
        assert_eq!(rejected.0, StatusCode::CONFLICT);
        assert_eq!(
            server.db.get_memory(&manual.id).unwrap().content,
            "updated manual knowledge"
        );
        delete_handler(State(server.clone()), Path(manual.id.clone()))
            .await
            .unwrap();
        assert!(server.db.find_memory(&manual.id).unwrap().is_none());
    }

    // ── Unit tests: validation helpers ──

    // ── Retrieval tests ──

    use crate::db::RetrieveQuery;

    fn seed_memory(
        db: &crate::db::Database,
        conv_id: &str,
        category: &str,
        content: &str,
        days_ago: i64,
    ) -> String {
        let now = chrono::Utc::now().timestamp_millis();
        let past = now - days_ago * 86_400_000;
        let id = uuid::Uuid::new_v4().to_string();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO memories (id, content, category, source, source_conversation_id, embedding, metadata, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'auto', ?4, NULL, NULL, ?5, ?6)",
            rusqlite::params![id, content, category, conv_id, past, past],
        )
        .unwrap();
        id
    }

    #[test]
    fn retrieve_ranks_strong_keyword_match_first() {
        let (server, db_path, conv_id) = test_server("retrieve-rank");
        seed_memory(
            &server.db,
            &conv_id,
            "knowledge",
            "今天学习 Rust 基础知识",
            1,
        );
        seed_memory(
            &server.db,
            &conv_id,
            "knowledge",
            "Trusted Execution 安全网关",
            1,
        );
        seed_memory(&server.db, &conv_id, "note", "用户今天喝了咖啡", 1);

        let results = server
            .db
            .retrieve_memories(&RetrieveQuery {
                q: "Trusted Execution".into(),
                top_k: 10,
                category: None,
            })
            .unwrap();

        assert!(!results.is_empty());
        assert!(results[0].memory.content.contains("Trusted Execution"));
        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn retrieve_excludes_completely_unrelated() {
        let (server, db_path, conv_id) = test_server("retrieve-exclude");
        seed_memory(&server.db, &conv_id, "note", "用户喜欢学习概率论", 1);

        let results = server
            .db
            .retrieve_memories(&RetrieveQuery {
                q: "Cloudflare".into(),
                top_k: 10,
                category: None,
            })
            .unwrap();

        assert!(results.is_empty());
        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn retrieve_newer_memory_ranks_higher_equal_text_score() {
        let (server, db_path, conv_id) = test_server("retrieve-time");
        seed_memory(&server.db, &conv_id, "fact", "Rust 项目开发", 30);
        seed_memory(&server.db, &conv_id, "fact", "Rust 项目进展", 1);

        let results = server
            .db
            .retrieve_memories(&RetrieveQuery {
                q: "Rust 项目".into(),
                top_k: 10,
                category: None,
            })
            .unwrap();

        assert_eq!(results.len(), 2);
        assert!(results[0].memory.content.contains("进展"));
        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn retrieve_text_relevance_beats_time() {
        let (server, db_path, conv_id) = test_server("retrieve-text-beats-time");
        seed_memory(&server.db, &conv_id, "knowledge", "Rust 安全执行框架", 60);
        seed_memory(&server.db, &conv_id, "knowledge", "今天天气不错", 0);

        let results = server
            .db
            .retrieve_memories(&RetrieveQuery {
                q: "Rust 安全执行".into(),
                top_k: 10,
                category: None,
            })
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].memory.content.contains("Rust 安全执行"));
        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn retrieve_respects_top_k() {
        let (server, db_path, conv_id) = test_server("retrieve-topk");
        for i in 0..15 {
            seed_memory(
                &server.db,
                &conv_id,
                "knowledge",
                &format!("Rust 安全测试 #{i}"),
                1,
            );
        }

        let results = server
            .db
            .retrieve_memories(&RetrieveQuery {
                q: "Rust 安全".into(),
                top_k: 5,
                category: None,
            })
            .unwrap();

        assert_eq!(results.len(), 5);
        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn retrieve_filters_by_category() {
        let (server, db_path, conv_id) = test_server("retrieve-cat-filter");
        seed_memory(&server.db, &conv_id, "preference", "Rust 偏好", 1);
        seed_memory(&server.db, &conv_id, "fact", "Rust 事实", 1);
        seed_memory(&server.db, &conv_id, "knowledge", "Rust 知识", 1);

        let results = server
            .db
            .retrieve_memories(&RetrieveQuery {
                q: "Rust".into(),
                top_k: 10,
                category: Some("knowledge".into()),
            })
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].memory.category, "knowledge");
        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn retrieve_handles_chinese_query() {
        let (server, db_path, conv_id) = test_server("retrieve-chinese");
        seed_memory(
            &server.db,
            &conv_id,
            "preference",
            "用户偏好每次只进行最小范围代码修改",
            1,
        );
        seed_memory(&server.db, &conv_id, "note", "今天天气不错", 1);

        let results = server
            .db
            .retrieve_memories(&RetrieveQuery {
                q: "最小修改".into(),
                top_k: 10,
                category: None,
            })
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].memory.content.contains("最小范围代码修改"));
        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn retrieve_utf8_no_panic() {
        let (server, db_path, conv_id) = test_server("retrieve-utf8");
        seed_memory(&server.db, &conv_id, "note", "😀🎉 测试 emoji", 1);

        let results = server
            .db
            .retrieve_memories(&RetrieveQuery {
                q: "😀".into(),
                top_k: 10,
                category: None,
            })
            .unwrap();

        assert!(!results.is_empty());
        std::fs::remove_file(&db_path).ok();
    }

    // ── Cosine similarity tests ──

    use crate::db::cosine_similarity;

    #[test]
    fn cosine_identical_vectors() {
        let a = vec![1.0_f32, 0.0, 0.0];
        let b = vec![1.0_f32, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b).unwrap();
        assert!((sim - 1.0).abs() < 0.001);
    }

    #[test]
    fn cosine_orthogonal_vectors() {
        let a = vec![1.0_f32, 0.0];
        let b = vec![0.0_f32, 1.0];
        let sim = cosine_similarity(&a, &b).unwrap();
        assert!((sim - 0.0).abs() < 0.001);
    }

    #[test]
    fn cosine_dimension_mismatch_is_none() {
        let sim = cosine_similarity(&[1.0, 0.0], &[1.0, 0.0, 0.0]);
        assert!(sim.is_none());
    }

    #[test]
    fn cosine_zero_vector_is_none() {
        let sim = cosine_similarity(&[0.0, 0.0], &[1.0, 0.0]);
        assert!(sim.is_none());
    }

    // ── Embedding serialization tests ──

    use crate::db::{parse_embedding, serialize_embedding};

    #[test]
    fn embedding_round_trip() {
        let vec = vec![0.1_f32, -0.442, 0.031];
        let json = serialize_embedding(&vec).unwrap();
        let parsed = parse_embedding(&json).unwrap();
        assert_eq!(parsed.len(), 3);
        assert!((parsed[0] - 0.1).abs() < 0.001);
        assert!((parsed[1] - (-0.442)).abs() < 0.001);
        assert!((parsed[2] - 0.031).abs() < 0.001);
    }

    #[test]
    fn parse_embedding_rejects_invalid_json() {
        assert!(parse_embedding("not-json").is_none());
    }

    #[test]
    fn parse_embedding_rejects_empty_array() {
        assert!(parse_embedding("[]").is_none());
    }

    // ── Hybrid retrieval tests ──

    #[test]
    fn hybrid_retrieval_without_embedding_is_lexical_fallback() {
        let (server, db_path, conv_id) = test_server("hybrid-no-embed");
        seed_memory(
            &server.db,
            &conv_id,
            "knowledge",
            "Trusted Execution 安全网关",
            1,
        );
        seed_memory(&server.db, &conv_id, "note", "无关内容", 1);

        // No query_embedding → lexical-only behavior
        let results = server
            .db
            .retrieve_memories_hybrid(
                &RetrieveQuery {
                    q: "Trusted Execution".into(),
                    top_k: 10,
                    category: None,
                },
                None,
            )
            .unwrap();

        assert!(!results.is_empty());
        assert!(results[0].memory.content.contains("Trusted Execution"));
        // lexical_mode = no vector score
        assert_eq!(results[0].vector_score, 0.0);
        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn hybrid_retrieval_with_embedding_boosts_vector_match() {
        let (server, db_path, conv_id) = test_server("hybrid-vector");
        let q_embed = vec![1.0_f32, 0.0, 0.0];

        // Memory A: high vector match, low lexical
        let id_a = seed_memory(&server.db, &conv_id, "knowledge", "完全不同的话题", 1);
        server
            .db
            .update_memory_embedding(&id_a, &[1.0, 0.1, 0.0])
            .unwrap();

        // Memory B: low vector match, OK lexical
        seed_memory(&server.db, &conv_id, "knowledge", "vector 测试", 1);

        let results = server
            .db
            .retrieve_memories_hybrid(
                &RetrieveQuery {
                    q: "vector".into(),
                    top_k: 10,
                    category: None,
                },
                Some(&[1.0, 0.0, 0.0]),
            )
            .unwrap();

        // Memory A should rank high due to strong vector match
        assert!(!results.is_empty());
        // The high-vector-match memory should be present
        assert!(results
            .iter()
            .any(|s| s.memory.content.contains("完全不同")));
        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn old_memory_without_embedding_still_returns_on_lexical_match() {
        let (server, db_path, conv_id) = test_server("hybrid-old-mem");
        seed_memory(
            &server.db,
            &conv_id,
            "knowledge",
            "Trusted Execution 安全执行框架",
            1,
        );
        // No embedding set → embedding is NULL

        let results = server
            .db
            .retrieve_memories_hybrid(
                &RetrieveQuery {
                    q: "Trusted Execution".into(),
                    top_k: 10,
                    category: None,
                },
                Some(&[1.0, 0.0]),
            )
            .unwrap();

        assert!(!results.is_empty());
        assert!(results[0].memory.content.contains("安全执行"));
        // No embedding → vector_score = 0
        assert_eq!(results[0].vector_score, 0.0);
        std::fs::remove_file(&db_path).ok();
    }

    // ── Embedding visibility tests ──

    use crate::db::{Memory, ScoredMemory};

    fn sample_memory(embedding: Option<String>) -> Memory {
        Memory {
            id: "mem-1".to_string(),
            content: "测试记忆".to_string(),
            category: "fact".to_string(),
            source: "auto".to_string(),
            source_conversation_id: Some("conv-1".to_string()),
            embedding,
            metadata: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn memory_serialization_does_not_include_embedding() {
        let mem = sample_memory(Some("[0.1,0.2,0.3]".to_string()));
        let json = serde_json::to_value(&mem).unwrap();
        assert!(json.get("embedding").is_none());
        // content/category still present
        assert_eq!(json["content"], "测试记忆");
    }

    #[test]
    fn memory_serialization_omits_none_embedding() {
        let mem = sample_memory(None);
        let json = serde_json::to_value(&mem).unwrap();
        assert!(json.get("embedding").is_none());
        assert!(json.get("embedding").is_none());
    }

    #[test]
    fn scored_memory_does_not_leak_embedding() {
        let scored = ScoredMemory {
            memory: sample_memory(Some("[0.1,0.2,0.3]".to_string())),
            score: 0.82,
            lexical_score: 0.61,
            vector_score: 0.91,
            revision: 1,
            scope: crate::db::MemoryScope::Profile,
        };
        let json = serde_json::to_value(&scored).unwrap();
        assert!(json.get("embedding").is_none());
        // score fields must be present
        assert_eq!(json["score"], 0.82);
        assert_eq!(json["lexical_score"], 0.61);
        assert_eq!(json["vector_score"], 0.91);
    }

    #[test]
    fn db_internal_read_still_returns_embedding() {
        let (server, db_path, conv_id) = test_server("embed-visibility-db");
        let id = seed_memory(&server.db, &conv_id, "fact", "测试", 1);
        server
            .db
            .update_memory_embedding(&id, &[0.1, 0.2, 0.3])
            .unwrap();

        let mem = server.db.get_memory(&id).unwrap();
        // Internal Rust object still has the embedding
        assert!(mem.embedding.is_some());

        // But serialization hides it
        let json = serde_json::to_value(&mem).unwrap();
        assert!(json.get("embedding").is_none());

        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn hybrid_retrieval_still_uses_stored_embedding() {
        let (server, db_path, conv_id) = test_server("embed-visibility-hybrid");
        let id = seed_memory(&server.db, &conv_id, "fact", "完全不同的话题", 1);
        server
            .db
            .update_memory_embedding(&id, &[1.0, 0.0, 0.0])
            .unwrap();

        let results = server
            .db
            .retrieve_memories_hybrid(
                &RetrieveQuery {
                    q: "无关词".into(),
                    top_k: 10,
                    category: None,
                },
                Some(&[1.0, 0.0, 0.0]),
            )
            .unwrap();

        assert!(!results.is_empty());
        // vector_score > 0 proves stored embedding was used internally
        assert!(results[0].vector_score > 0.0);

        // But serialization still hides raw embedding
        let json = serde_json::to_value(&results[0]).unwrap();
        assert!(json.get("embedding").is_none());
        assert!(json.get("vector_score").is_some());

        std::fs::remove_file(&db_path).ok();
    }

    // ── Backfill / reindex tests ──

    fn seed_memory_with_embedding(
        db: &crate::db::Database,
        conv_id: &str,
        category: &str,
        content: &str,
        embedding: &str,
    ) -> String {
        let now = chrono::Utc::now().timestamp_millis();
        let id = uuid::Uuid::new_v4().to_string();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO memories (id, content, category, source, source_conversation_id, embedding, metadata, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'auto', ?4, ?5, NULL, ?6, ?6)",
            rusqlite::params![id, content, category, conv_id, embedding, now],
        )
        .unwrap();
        id
    }

    #[test]
    fn list_without_embedding_only_returns_missing() {
        let (server, db_path, conv_id) = test_server("backfill-query");
        seed_memory(&server.db, &conv_id, "fact", "A 无 embedding", 1);
        seed_memory_with_embedding(&server.db, &conv_id, "fact", "B 有 embedding", "[0.1,0.2]");
        seed_memory(&server.db, &conv_id, "fact", "C 无 embedding", 1);

        let missing = server.db.list_memories_without_embedding(10).unwrap();
        let contents: Vec<&str> = missing.iter().map(|m| m.content.as_str()).collect();
        assert_eq!(missing.len(), 2);
        assert!(contents.contains(&"A 无 embedding"));
        assert!(contents.contains(&"C 无 embedding"));
        assert!(!contents.contains(&"B 有 embedding"));

        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn list_without_embedding_respects_limit() {
        let (server, db_path, conv_id) = test_server("backfill-limit");
        for i in 0..5 {
            seed_memory(&server.db, &conv_id, "fact", &format!("M{i}"), 1);
        }
        let missing = server.db.list_memories_without_embedding(2).unwrap();
        assert_eq!(missing.len(), 2);
        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn count_without_embedding_is_accurate() {
        let (server, db_path, conv_id) = test_server("backfill-count");
        seed_memory(&server.db, &conv_id, "fact", "A", 1);
        seed_memory(&server.db, &conv_id, "fact", "B", 1);
        seed_memory(&server.db, &conv_id, "fact", "C", 1);
        seed_memory_with_embedding(&server.db, &conv_id, "fact", "D", "[0.1]");
        seed_memory_with_embedding(&server.db, &conv_id, "fact", "E", "[0.2]");

        assert_eq!(server.db.count_memories_without_embedding().unwrap(), 3);
        std::fs::remove_file(&db_path).ok();
    }

    #[tokio::test]
    async fn reindex_rejects_when_provider_not_configured() {
        let (server, db_path, conv_id) = test_server("reindex-unconfigured");
        seed_memory(&server.db, &conv_id, "fact", "A", 1);

        // Default config has empty embedding model → not configured
        let err = reindex_handler(
            State(Arc::clone(&server)),
            Json(ReindexRequest { limit: 10 }),
        )
        .await
        .expect_err("should fail when provider not configured");

        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        let value = err.1 .0;
        assert_eq!(value["code"], "embedding_not_configured");
        assert!(value["error"].as_str().unwrap().contains("not configured"));
        // No embedding written
        assert_eq!(server.db.count_memories_without_embedding().unwrap(), 1);
        std::fs::remove_file(&db_path).ok();
    }

    // Mock embedding provider for backfill tests
    use axum::{extract::State as AxumState, routing::post, Router as AxumRouter};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone)]
    struct BackfillMockState {
        counter: Arc<AtomicUsize>,
        fail_on: Arc<std::sync::Mutex<Option<usize>>>, // fail the Nth request (0-based)
    }

    async fn start_backfill_mock(
        fail_on: Option<usize>,
    ) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
        let counter = Arc::new(AtomicUsize::new(0));
        let state = BackfillMockState {
            counter: Arc::clone(&counter),
            fail_on: Arc::new(std::sync::Mutex::new(fail_on)),
        };
        let s = state.clone();
        let app = AxumRouter::new()
            .route("/embeddings", post(backfill_embed_handler))
            .with_state(s);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (addr, counter, handle)
    }

    async fn backfill_embed_handler(
        AxumState(state): AxumState<BackfillMockState>,
        Json(body): Json<serde_json::Value>,
    ) -> (axum::http::StatusCode, Json<serde_json::Value>) {
        let idx = state.counter.fetch_add(1, Ordering::SeqCst);
        let should_fail = state
            .fail_on
            .lock()
            .unwrap()
            .map(|f| f == idx)
            .unwrap_or(false);
        if should_fail {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "mock failure"})),
            );
        }
        let _ = body;
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({
                "data": [{"embedding": [0.5, 0.25, 0.125]}]
            })),
        )
    }

    fn configure_embedding(server: &AppServer, base_url: &str) {
        let mut config = server.config.write();
        config.model.embedding_model = "test-embed-model".to_string();
        config.model.embedding_base_url = base_url.to_string();
        config.model.embedding_api_key = "test-key".to_string();
    }

    #[tokio::test]
    async fn reindex_backfills_all_missing() {
        let (server, db_path, conv_id) = test_server("reindex-backfill");
        let a = seed_memory(&server.db, &conv_id, "fact", "A", 1);
        let b = seed_memory(&server.db, &conv_id, "fact", "B", 1);

        let (addr, _counter, _handle) = start_backfill_mock(None).await;
        configure_embedding(&server, &addr);

        let json = reindex_handler(
            State(Arc::clone(&server)),
            Json(ReindexRequest { limit: 10 }),
        )
        .await
        .unwrap();
        let value = json.0;
        assert_eq!(value["ok"], true);
        assert_eq!(value["processed"], 2);
        assert_eq!(value["succeeded"], 2);
        assert_eq!(value["failed"], 0);

        // Both now have embeddings
        assert!(server.db.get_memory(&a).unwrap().embedding.is_some());
        assert!(server.db.get_memory(&b).unwrap().embedding.is_some());
        assert_eq!(server.db.count_memories_without_embedding().unwrap(), 0);

        std::fs::remove_file(&db_path).ok();
    }

    #[tokio::test]
    async fn reindex_single_failure_does_not_abort() {
        let (server, db_path, conv_id) = test_server("reindex-failure");
        seed_memory(&server.db, &conv_id, "fact", "A", 1); // will succeed
        seed_memory(&server.db, &conv_id, "fact", "B", 1); // will fail (2nd request)
        seed_memory(&server.db, &conv_id, "fact", "C", 1); // will succeed

        // Fail the 2nd request (0-based index 1)
        let (addr, _counter, _handle) = start_backfill_mock(Some(1)).await;
        configure_embedding(&server, &addr);

        let json = reindex_handler(
            State(Arc::clone(&server)),
            Json(ReindexRequest { limit: 10 }),
        )
        .await
        .unwrap();
        let value = json.0;
        assert_eq!(value["processed"], 3);
        assert_eq!(value["succeeded"], 2);
        assert_eq!(value["failed"], 1);
        // 1 memory still missing embedding
        assert_eq!(server.db.count_memories_without_embedding().unwrap(), 1);

        std::fs::remove_file(&db_path).ok();
    }

    #[tokio::test]
    async fn reindex_is_idempotent() {
        let (server, db_path, conv_id) = test_server("reindex-idempotent");
        seed_memory(&server.db, &conv_id, "fact", "A", 1);
        seed_memory(&server.db, &conv_id, "fact", "B", 1);

        let (addr, counter, _handle) = start_backfill_mock(None).await;
        configure_embedding(&server, &addr);

        // First run
        let json1 = reindex_handler(
            State(Arc::clone(&server)),
            Json(ReindexRequest { limit: 10 }),
        )
        .await
        .unwrap();
        assert_eq!(json1.0["succeeded"], 2);
        let calls_after_first = counter.load(Ordering::SeqCst);

        // Second run — nothing to process
        let json2 = reindex_handler(
            State(Arc::clone(&server)),
            Json(ReindexRequest { limit: 10 }),
        )
        .await
        .unwrap();
        assert_eq!(json2.0["processed"], 0);
        assert_eq!(json2.0["succeeded"], 0);
        // No additional provider calls
        assert_eq!(counter.load(Ordering::SeqCst), calls_after_first);

        std::fs::remove_file(&db_path).ok();
    }

    #[tokio::test]
    async fn reindex_rebuilds_legacy_embeddings_without_model_identity() {
        let (server, db_path, conv_id) = test_server("reindex-no-overwrite");
        let existing = seed_memory_with_embedding(
            &server.db,
            &conv_id,
            "fact",
            "已有 embedding",
            "[0.11,0.22]",
        );
        seed_memory(&server.db, &conv_id, "fact", "新 memory", 1);

        let (addr, counter, _handle) = start_backfill_mock(None).await;
        configure_embedding(&server, &addr);

        let json = reindex_handler(
            State(Arc::clone(&server)),
            Json(ReindexRequest { limit: 10 }),
        )
        .await
        .unwrap();
        // Legacy vectors have no provider/model identity and must be rebuilt.
        assert_eq!(json.0["processed"], 2);
        assert_eq!(json.0["succeeded"], 2);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
        let mem = server.db.get_memory(&existing).unwrap();
        assert_eq!(mem.embedding.as_deref(), Some("[0.5,0.25,0.125]"));
        assert!(server
            .db
            .memory_governance(&existing)
            .unwrap()
            .embedding_model
            .is_some());

        std::fs::remove_file(&db_path).ok();
    }
}
