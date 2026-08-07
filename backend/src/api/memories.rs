// ============================================================
// Memory API handlers — long-term AI memory CRUD + reserved endpoints
// ============================================================

use axum::{
    extract::{Path, Query, State},
    Json,
};
use std::sync::Arc;

use crate::db::{CreateMemoryRequest, Memory, MemoryQuery, UpdateMemoryRequest};
use crate::server::AppServer;

// ── List / Search ──

pub async fn list_handler(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<MemoryQuery>,
) -> Json<Vec<Memory>> {
    // Log query params for debugging
    if query.q.is_some() || query.category.is_some() || query.source.is_some() {
        tracing::debug!(
            "Memory query: q={:?} cat={:?} src={:?}",
            query.q,
            query.category,
            query.source
        );
    }
    Json(server.db.list_memories(&query).unwrap_or_default())
}

// ── Get by ID ──

pub async fn get_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match server.db.get_memory(&id) {
        Ok(mem) => Json(serde_json::to_value(mem).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

// ── Create ──

pub async fn create_handler(
    State(server): State<Arc<AppServer>>,
    Json(req): Json<CreateMemoryRequest>,
) -> Json<serde_json::Value> {
    match server.db.create_memory(&req) {
        Ok(mem) => Json(serde_json::to_value(mem).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

// ── Update ──

pub async fn update_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateMemoryRequest>,
) -> Json<serde_json::Value> {
    match server.db.update_memory(&id, &req) {
        Ok(mem) => Json(serde_json::to_value(mem).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

// ── Delete ──

pub async fn delete_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match server.db.delete_memory(&id) {
        Ok(_) => Json(serde_json::json!({"status": "deleted"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

// ── Stats ──

pub async fn stats_handler(State(server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    match server.db.get_memory_stats() {
        Ok(stats) => Json(serde_json::to_value(stats).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

// ── Extract (placeholder — triggers LLM-based memory extraction) ──

#[derive(serde::Deserialize)]
pub struct ExtractRequest {
    pub conversation_id: Option<String>,
}

pub async fn extract_handler(
    State(_server): State<Arc<AppServer>>,
    Json(req): Json<ExtractRequest>,
) -> Json<serde_json::Value> {
    // TODO: Implement actual LLM-based memory extraction
    // Will analyze the conversation and extract key facts
    let conv_id = req.conversation_id.unwrap_or_default();
    Json(serde_json::json!({
        "status": "not_implemented",
        "message": "Memory extraction will be implemented in a future update",
        "conversation_id": conv_id,
    }))
}

// ── Reserved: Batch-import ──

pub async fn batch_import_handler(
    State(_server): State<Arc<AppServer>>,
    Json(_body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "reserved",
        "message": "Batch import endpoint — reserved for future use"
    }))
}

// ── Reserved: Batch-delete ──

pub async fn batch_delete_handler(
    State(_server): State<Arc<AppServer>>,
    Json(_body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "reserved",
        "message": "Batch delete endpoint — reserved for future use"
    }))
}

// ── Reserved: Export ──

pub async fn export_handler(State(_server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "reserved",
        "message": "Export endpoint — reserved for future use"
    }))
}

// ── Reserved: Merge duplicates ──

pub async fn merge_handler(
    State(_server): State<Arc<AppServer>>,
    Json(_body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "reserved",
        "message": "Merge duplicates endpoint — reserved for future use"
    }))
}

// ── Reserved: Reindex embeddings ──

pub async fn reindex_handler(State(_server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "reserved",
        "message": "Reindex embeddings endpoint — reserved for future use"
    }))
}
