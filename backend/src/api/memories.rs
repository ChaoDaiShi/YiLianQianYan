// ============================================================
// Memory API handlers — long-term AI memory CRUD + reserved endpoints
// ============================================================

use axum::{
    extract::{Path, Query, State},
    Json,
};
use std::sync::Arc;

use crate::db::{CreateMemoryRequest, Memory, MemoryQuery, RetrieveQuery, UpdateMemoryRequest};
use crate::llm::client::LlmClient;
use crate::llm::types::ChatMessage;
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
        Ok(mem) => {
            // Auto-embed: best-effort, never fails the create
            let model_cfg = server.config.read().model.clone();
            if model_cfg.has_embedding() {
                let llm = LlmClient::new(&model_cfg);
                match llm.embed(&req.content).await {
                    Ok(vec) => {
                        if let Err(e) = server.db.update_memory_embedding(&mem.id, &vec) {
                            tracing::warn!(memory_id = %mem.id, error = %e, "failed to persist embedding");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(memory_id = %mem.id, error = %e, "embedding generation failed, memory persisted without embedding");
                    }
                }
            }
            Json(serde_json::to_value(mem).unwrap_or_default())
        }
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

// ── Retrieve (ranked keyword retrieval, hybrid when embedding is available) ──

pub async fn retrieve_handler(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<RetrieveQuery>,
) -> Json<serde_json::Value> {
    let q = query.q.trim().to_string();
    if q.is_empty() {
        return Json(serde_json::json!({
            "ok": false,
            "error": "query parameter 'q' is required and must not be empty"
        }));
    }

    let model_cfg = server.config.read().model.clone();
    let mut mode = "lexical";
    let mut query_embedding: Option<Vec<f32>> = None;

    if model_cfg.has_embedding() {
        let llm = LlmClient::new(&model_cfg);
        match llm.embed(&q).await {
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

    match server
        .db
        .retrieve_memories_hybrid(&query, query_embedding.as_deref())
    {
        Ok(scored) => Json(serde_json::json!({
            "query": q,
            "count": scored.len(),
            "mode": mode,
            "memories": scored,
        })),
        Err(e) => Json(serde_json::json!({
            "ok": false,
            "error": e
        })),
    }
}

// ── Extract (LLM-based memory extraction) ──

#[derive(serde::Deserialize)]
pub struct ExtractRequest {
    pub conversation_id: Option<String>,
}

/// A single candidate memory returned by the LLM extraction tool.
#[derive(Debug, Clone, serde::Deserialize)]
struct ExtractedMemory {
    category: String,
    content: String,
}

/// The expected JSON payload inside the tool-call arguments.
#[derive(Debug, serde::Deserialize)]
struct ExtractMemoriesArgs {
    memories: Vec<ExtractedMemory>,
}

const VALID_CATEGORIES: &[&str] = &["fact", "preference", "knowledge", "note"];
const MAX_MEMORY_LENGTH: usize = 2000;
const EXTRACTION_TOOL_NAME: &str = "extract_memories";

/// Sensitive-keyword patterns — any extracted memory whose lowercased content
/// contains one of these substrings is rejected.
const SENSITIVE_PATTERNS: &[&str] = &[
    "api_key",
    "apikey",
    "api key",
    "token",
    "password",
    "secret",
    "authorization",
    "cookie",
    "private_key",
    "private key",
];

fn is_valid_category(category: &str) -> bool {
    VALID_CATEGORIES.contains(&category)
}

fn is_sensitive(content: &str) -> bool {
    let lower = content.to_lowercase();
    SENSITIVE_PATTERNS
        .iter()
        .any(|pattern| lower.contains(pattern))
}

fn memory_exists(db: &crate::db::Database, category: &str, content: &str) -> bool {
    let trimmed = content.trim();
    db.list_memories(&MemoryQuery {
        category: Some(category.to_string()),
        q: None,
        source: None,
        limit: Some(500),
        offset: None,
    })
    .unwrap_or_default()
    .into_iter()
    .any(|m| m.content.trim() == trimmed)
}

/// Build the tool definition for structured memory extraction.
fn extraction_tool_definition() -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": EXTRACTION_TOOL_NAME,
            "description": "Extract long-term memories from a conversation.\n\
                Only save information that is valuable across conversations.\n\
                DO NOT save: one-off chat, temporary questions, model responses, tool logs, \
                API keys, tokens, passwords, cookies, full conversation summaries.",
            "parameters": {
                "type": "object",
                "properties": {
                    "memories": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "category": {
                                    "type": "string",
                                    "enum": ["fact", "preference", "knowledge", "note"]
                                },
                                "content": {
                                    "type": "string",
                                    "description": "The memory content in the user's language"
                                }
                            },
                            "required": ["category", "content"]
                        }
                    }
                },
                "required": ["memories"]
            }
        }
    })
}

fn build_extraction_prompt(conversation_text: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage {
            role: "system".to_string(),
            content: Some(
                "你是忆涟千言的记忆提取模块。你的任务是从对话中提取可跨对话保留的长期记忆。\n\n\
                提取规则：\n\
                - 只提取对后续对话仍有价值的长期信息\n\
                - 可以提取：用户长期偏好、稳定个人事实、长期目标、持续项目背景、用户明确要求记住的信息\n\
                - 不要提取：一次性闲聊、临时问题、模型自己的回答、工具调用日志、完整对话摘要\n\
                - category 必须是: fact（事实）, preference（偏好）, knowledge（知识）, note（备注）之一\n\
                - content 使用用户原始语言\n\
                - 不要提取任何 API Key、Token、Password、Secret、Cookie 等凭据信息\n\
                - 如果没有值得长期保留的信息，返回空数组"
                    .to_string(),
            ),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
        ChatMessage {
            role: "user".to_string(),
            content: Some(format!("请从以下对话中提取长期记忆：\n\n{conversation_text}")),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
    ]
}

fn build_conversation_text(messages: &[crate::db::MessageRow]) -> String {
    let mut text = String::new();
    for msg in messages {
        let role_label = match msg.role.as_str() {
            "user" => "用户",
            "assistant" => "助手",
            "tool" => continue, // skip tool messages
            _ => continue,
        };
        // Skip tool-call JSON blobs in assistant messages
        let content = if msg.role == "assistant" {
            // Only include text content, not tool call JSON
            if msg.tool_calls.is_some() && msg.content.is_empty() {
                continue;
            }
            crate::utils::text::truncate_chars(&msg.content, 500)
        } else {
            crate::utils::text::truncate_chars(&msg.content, 1000)
        };
        if content.trim().is_empty() {
            continue;
        }
        text.push_str(&format!("{}: {}\n", role_label, content));
    }
    text
}

pub async fn extract_handler(
    State(server): State<Arc<AppServer>>,
    Json(req): Json<ExtractRequest>,
) -> Json<serde_json::Value> {
    let conv_id = match req.conversation_id {
        Some(ref id) if !id.is_empty() => id.clone(),
        _ => {
            return Json(serde_json::json!({
                "ok": false,
                "error": "conversation_id is required"
            }))
        }
    };

    // 1. Read conversation messages
    let conversation = match server.db.get_conversation(&conv_id) {
        Ok(conv) => conv,
        Err(e) => {
            return Json(serde_json::json!({
                "ok": false,
                "error": format!("conversation not found: {e}")
            }))
        }
    };

    let messages = conversation.messages;
    if messages.is_empty() {
        return Json(serde_json::json!({
            "ok": true,
            "created": 0,
            "skipped": 0,
            "memories": []
        }));
    }

    let conversation_text = build_conversation_text(&messages);
    if conversation_text.trim().is_empty() {
        return Json(serde_json::json!({
            "ok": true,
            "created": 0,
            "skipped": 0,
            "memories": []
        }));
    }

    // 2. Call LLM
    let config = server.config.read().clone();
    let llm_client = LlmClient::new(&config.model);
    let prompt = build_extraction_prompt(&conversation_text);
    let tools = vec![extraction_tool_definition()];

    let llm_response = match llm_client.invoke(&prompt, &tools).await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::error!("Memory extraction LLM call failed: {e}");
            return Json(serde_json::json!({
                "ok": false,
                "error": format!("LLM call failed: {e}")
            }));
        }
    };

    // 3. Parse structured output from tool call
    let tool_calls = match llm_response
        .choices
        .first()
        .and_then(|c| c.message.tool_calls.as_ref())
    {
        Some(tcs) if !tcs.is_empty() => tcs,
        _ => {
            // No tool call — maybe the model returned empty content
            // Try parsing the raw content as JSON (fallback for models
            // that output JSON directly)
            let raw_content = llm_response
                .choices
                .first()
                .and_then(|c| c.message.content.as_deref())
                .unwrap_or("");
            if let Ok(parsed) = serde_json::from_str::<ExtractMemoriesArgs>(raw_content.trim()) {
                return process_extracted_memories(&server, &conv_id, parsed.memories).await;
            }
            return Json(serde_json::json!({
                "ok": false,
                "error": "LLM did not return structured memory output"
            }));
        }
    };

    let extraction_call = tool_calls
        .iter()
        .find(|tc| tc.function.name == EXTRACTION_TOOL_NAME);

    let args_str = match extraction_call {
        Some(tc) => &tc.function.arguments,
        None => {
            return Json(serde_json::json!({
                "ok": false,
                "error": "LLM did not call the extraction tool"
            }))
        }
    };

    let parsed: ExtractMemoriesArgs = match serde_json::from_str(args_str) {
        Ok(args) => args,
        Err(e) => {
            tracing::error!("Failed to parse extraction tool arguments: {e}");
            return Json(serde_json::json!({
                "ok": false,
                "error": format!("failed to parse LLM output: {e}")
            }));
        }
    };

    process_extracted_memories(&server, &conv_id, parsed.memories).await
}

async fn process_extracted_memories(
    server: &AppServer,
    conv_id: &str,
    candidates: Vec<ExtractedMemory>,
) -> Json<serde_json::Value> {
    let model_cfg = server.config.read().model.clone();
    let has_embedding = model_cfg.has_embedding();
    let mut created = Vec::new();
    let mut skipped = 0usize;

    for mem in candidates {
        // Validate category
        if !is_valid_category(&mem.category) {
            tracing::debug!("Skipping memory with invalid category: {}", mem.category);
            skipped += 1;
            continue;
        }

        // Validate content
        let content = mem.content.trim().to_string();
        if content.is_empty() {
            skipped += 1;
            continue;
        }
        if content.chars().count() > MAX_MEMORY_LENGTH {
            tracing::debug!(
                "Skipping overly long memory ({} chars)",
                content.chars().count()
            );
            skipped += 1;
            continue;
        }

        // Reject sensitive content
        if is_sensitive(&content) {
            tracing::warn!("Rejected memory containing sensitive pattern");
            skipped += 1;
            continue;
        }

        // Dedup
        if memory_exists(&server.db, &mem.category, &content) {
            skipped += 1;
            continue;
        }

        // Write
        let memory = match server.db.create_memory(&CreateMemoryRequest {
            content: content.clone(),
            category: mem.category.clone(),
            source: "auto".to_string(),
            source_conversation_id: Some(conv_id.to_string()),
            metadata: None,
        }) {
            Ok(memory) => memory,
            Err(e) => {
                tracing::error!("Failed to create memory: {e}");
                skipped += 1;
                continue;
            }
        };

        // Auto-embed: best-effort, never fails memory creation
        if has_embedding {
            let llm = LlmClient::new(&model_cfg);
            match llm.embed(&content).await {
                Ok(vec) => {
                    if let Err(e) = server.db.update_memory_embedding(&memory.id, &vec) {
                        tracing::warn!(memory_id = %memory.id, error = %e, "failed to persist embedding");
                    }
                }
                Err(e) => {
                    tracing::warn!(memory_id = %memory.id, error = %e, "embedding generation failed, memory persisted without embedding");
                }
            }
        }

        created.push(memory);
    }

    Json(serde_json::json!({
        "ok": true,
        "created": created.len(),
        "skipped": skipped,
        "memories": created
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
) -> Json<serde_json::Value> {
    // Clamp limit: 1..=MAX_REINDEX_LIMIT
    let requested = req.limit.clamp(1, MAX_REINDEX_LIMIT);

    // Reject when embedding provider is not configured
    let config = server.config.read().clone();
    if !config.model.has_embedding() {
        return Json(serde_json::json!({
            "ok": false,
            "error": "embedding provider is not configured"
        }));
    }

    // Load memories missing embedding
    let missing = match server.db.list_memories_without_embedding(requested) {
        Ok(memories) => memories,
        Err(e) => {
            tracing::error!("reindex: failed to load missing-embedding memories: {e}");
            return Json(serde_json::json!({
                "ok": false,
                "error": format!("failed to load memories: {e}")
            }));
        }
    };

    let processed = missing.len();
    let mut succeeded = 0usize;
    let mut failed = 0usize;

    let llm_client = LlmClient::new(&config.model);

    for memory in &missing {
        match llm_client.embed(&memory.content).await {
            Ok(vector) => match server.db.update_memory_embedding(&memory.id, &vector) {
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

    let remaining = server.db.count_memories_without_embedding().unwrap_or(0);

    Json(serde_json::json!({
        "ok": true,
        "requested": requested,
        "processed": processed,
        "succeeded": succeeded,
        "failed": failed,
        "remaining": remaining,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::MessageRow;
    use crate::safety::ControlSession;
    use crate::server::AppServer;

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

    // ── Unit tests: validation helpers ──

    #[test]
    fn valid_categories_are_accepted() {
        assert!(is_valid_category("fact"));
        assert!(is_valid_category("preference"));
        assert!(is_valid_category("knowledge"));
        assert!(is_valid_category("note"));
    }

    #[test]
    fn invalid_categories_are_rejected() {
        assert!(!is_valid_category("random"));
        assert!(!is_valid_category(""));
        assert!(!is_valid_category("administrator"));
        assert!(!is_valid_category("FACT")); // case-sensitive
    }

    #[test]
    fn sensitive_patterns_are_detected() {
        assert!(is_sensitive("my api_key is sk-12345"));
        assert!(is_sensitive("the token is deadbeef"));
        assert!(is_sensitive("password: hunter2"));
        assert!(is_sensitive("Authorization: Bearer xxx"));
        assert!(is_sensitive("cookie session=abc"));
        assert!(is_sensitive("这里包含apiKey"));
    }

    #[test]
    fn clean_content_is_not_sensitive() {
        assert!(!is_sensitive("用户偏好每次只进行最小范围代码修改"));
        assert!(!is_sensitive("project uses Rust for backend"));
        assert!(!is_sensitive(""));
    }

    #[test]
    fn build_conversation_text_filters_tool_messages() {
        let msgs = vec![
            MessageRow {
                id: "1".into(),
                conversation_id: "c1".into(),
                role: "user".into(),
                content: "你好".into(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_result: None,
                created_at: 0,
            },
            MessageRow {
                id: "2".into(),
                conversation_id: "c1".into(),
                role: "assistant".into(),
                content: "你好！有什么可以帮助你的？".into(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_result: None,
                created_at: 1,
            },
            MessageRow {
                id: "3".into(),
                conversation_id: "c1".into(),
                role: "tool".into(),
                content: "screenshot data...".into(),
                tool_calls: None,
                tool_call_id: Some("tc1".into()),
                tool_name: Some("screenshot".into()),
                tool_result: None,
                created_at: 2,
            },
            MessageRow {
                id: "4".into(),
                conversation_id: "c1".into(),
                role: "user".into(),
                content: "我以后写代码希望每次只改最少几个文件".into(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_result: None,
                created_at: 3,
            },
        ];
        let text = build_conversation_text(&msgs);
        assert!(text.contains("用户: 你好"));
        assert!(text.contains("用户: 我以后写代码希望每次只改最少几个文件"));
        // tool messages are skipped
        assert!(!text.contains("screenshot"));
    }

    #[test]
    fn build_extraction_prompt_contains_system_and_user() {
        let prompt = build_extraction_prompt("测试对话");
        assert_eq!(prompt.len(), 2);
        assert_eq!(prompt[0].role, "system");
        assert_eq!(prompt[1].role, "user");
        assert!(prompt[1].content.as_ref().unwrap().contains("测试对话"));
    }

    // ── Integration test: process_extracted_memories ──

    #[tokio::test]
    async fn process_extracts_valid_preference_memory() {
        let (server, db_path, conv_id) = test_server("process-valid");

        let candidates = vec![ExtractedMemory {
            category: "preference".to_string(),
            content: "用户偏好每次只进行最小范围代码修改".to_string(),
        }];

        let json = process_extracted_memories(&server, &conv_id, candidates).await;
        let value = json.0;
        assert_eq!(value["ok"], true);
        assert_eq!(value["created"], 1);
        assert_eq!(value["skipped"], 0);
        assert_eq!(value["memories"].as_array().unwrap().len(), 1);
        assert_eq!(value["memories"][0]["category"], "preference");
        assert_eq!(value["memories"][0]["source"], "auto");

        std::fs::remove_file(&db_path).ok();
    }

    #[tokio::test]
    async fn process_skips_duplicate_memory() {
        let (server, db_path, conv_id) = test_server("process-dup");
        let candidates = vec![ExtractedMemory {
            category: "fact".to_string(),
            content: "用户居住在杭州".to_string(),
        }];

        // First extraction
        let json1 = process_extracted_memories(&server, &conv_id, candidates.clone()).await;
        assert_eq!(json1.0["created"], 1);

        // Second extraction with identical content
        let json2 = process_extracted_memories(&server, &conv_id, candidates).await;
        assert_eq!(json2.0["created"], 0);
        assert_eq!(json2.0["skipped"], 1);

        std::fs::remove_file(&db_path).ok();
    }

    #[tokio::test]
    async fn process_skips_empty_candidates() {
        let (server, db_path, conv_id) = test_server("process-empty");
        let candidates: Vec<ExtractedMemory> = vec![];

        let json = process_extracted_memories(&server, &conv_id, candidates).await;
        assert_eq!(json.0["ok"], true);
        assert_eq!(json.0["created"], 0);
        assert_eq!(json.0["skipped"], 0);

        std::fs::remove_file(&db_path).ok();
    }

    #[tokio::test]
    async fn process_skips_invalid_category() {
        let (server, db_path, conv_id) = test_server("process-invalid-cat");
        let candidates = vec![ExtractedMemory {
            category: "random".to_string(),
            content: "some content".to_string(),
        }];

        let json = process_extracted_memories(&server, &conv_id, candidates).await;
        assert_eq!(json.0["created"], 0);
        assert_eq!(json.0["skipped"], 1);

        std::fs::remove_file(&db_path).ok();
    }

    #[tokio::test]
    async fn process_skips_empty_content() {
        let (server, db_path, conv_id) = test_server("process-empty-content");
        let candidates = vec![ExtractedMemory {
            category: "note".to_string(),
            content: "   ".to_string(),
        }];

        let json = process_extracted_memories(&server, &conv_id, candidates).await;
        assert_eq!(json.0["created"], 0);
        assert_eq!(json.0["skipped"], 1);

        std::fs::remove_file(&db_path).ok();
    }

    #[tokio::test]
    async fn process_rejects_sensitive_content() {
        let (server, db_path, conv_id) = test_server("process-sensitive");
        let candidates = vec![ExtractedMemory {
            category: "knowledge".to_string(),
            content: "API key is sk-abc123".to_string(),
        }];

        let json = process_extracted_memories(&server, &conv_id, candidates).await;
        assert_eq!(json.0["created"], 0);
        assert_eq!(json.0["skipped"], 1);

        std::fs::remove_file(&db_path).ok();
    }

    #[tokio::test]
    async fn process_handles_chinese_content() {
        let (server, db_path, conv_id) = test_server("process-chinese");
        let candidates = vec![ExtractedMemory {
            category: "preference".to_string(),
            content: "用户长期使用中文内容进行交互，偏好简洁回复".to_string(),
        }];

        let json = process_extracted_memories(&server, &conv_id, candidates).await;
        let value = json.0;
        assert_eq!(value["ok"], true);
        assert_eq!(value["created"], 1);
        let saved = &value["memories"][0]["content"];
        assert!(saved.as_str().unwrap().contains("中文"));

        std::fs::remove_file(&db_path).ok();
    }

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
        let json = reindex_handler(
            State(Arc::clone(&server)),
            Json(ReindexRequest { limit: 10 }),
        )
        .await;

        let value = json.0;
        assert_eq!(value["ok"], false);
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
        .await;
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
        .await;
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
        .await;
        assert_eq!(json1.0["succeeded"], 2);
        let calls_after_first = counter.load(Ordering::SeqCst);

        // Second run — nothing to process
        let json2 = reindex_handler(
            State(Arc::clone(&server)),
            Json(ReindexRequest { limit: 10 }),
        )
        .await;
        assert_eq!(json2.0["processed"], 0);
        assert_eq!(json2.0["succeeded"], 0);
        // No additional provider calls
        assert_eq!(counter.load(Ordering::SeqCst), calls_after_first);

        std::fs::remove_file(&db_path).ok();
    }

    #[tokio::test]
    async fn reindex_does_not_overwrite_existing_embedding() {
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
        .await;
        // Only 1 memory was missing (the "新 memory")
        assert_eq!(json.0["processed"], 1);
        assert_eq!(json.0["succeeded"], 1);
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        // Existing embedding unchanged
        let mem = server.db.get_memory(&existing).unwrap();
        assert_eq!(mem.embedding.as_deref(), Some("[0.11,0.22]"));

        std::fs::remove_file(&db_path).ok();
    }
}
