// ============================================================
// Memory API handlers — long-term AI memory CRUD + reserved endpoints
// ============================================================

use axum::{
    extract::{Path, Query, State},
    Json,
};
use std::sync::Arc;

use crate::db::{CreateMemoryRequest, Memory, MemoryQuery, UpdateMemoryRequest};
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
                return process_extracted_memories(&server, &conv_id, parsed.memories);
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

    process_extracted_memories(&server, &conv_id, parsed.memories)
}

fn process_extracted_memories(
    server: &AppServer,
    conv_id: &str,
    candidates: Vec<ExtractedMemory>,
) -> Json<serde_json::Value> {
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
        match server.db.create_memory(&CreateMemoryRequest {
            content,
            category: mem.category.clone(),
            source: "auto".to_string(),
            source_conversation_id: Some(conv_id.to_string()),
            metadata: None,
        }) {
            Ok(memory) => created.push(memory),
            Err(e) => {
                tracing::error!("Failed to create memory: {e}");
                skipped += 1;
            }
        }
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

// ── Reserved: Reindex embeddings ──

pub async fn reindex_handler(State(_server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "reserved",
        "message": "Reindex embeddings endpoint — reserved for future use"
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

    #[test]
    fn process_extracts_valid_preference_memory() {
        let (server, db_path, conv_id) = test_server("process-valid");

        let candidates = vec![ExtractedMemory {
            category: "preference".to_string(),
            content: "用户偏好每次只进行最小范围代码修改".to_string(),
        }];

        let json = process_extracted_memories(&server, &conv_id, candidates);
        let value = json.0;
        assert_eq!(value["ok"], true);
        assert_eq!(value["created"], 1);
        assert_eq!(value["skipped"], 0);
        assert_eq!(value["memories"].as_array().unwrap().len(), 1);
        assert_eq!(value["memories"][0]["category"], "preference");
        assert_eq!(value["memories"][0]["source"], "auto");

        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn process_skips_duplicate_memory() {
        let (server, db_path, conv_id) = test_server("process-dup");
        let candidates = vec![ExtractedMemory {
            category: "fact".to_string(),
            content: "用户居住在杭州".to_string(),
        }];

        // First extraction
        let json1 = process_extracted_memories(&server, &conv_id, candidates.clone());
        assert_eq!(json1.0["created"], 1);

        // Second extraction with identical content
        let json2 = process_extracted_memories(&server, &conv_id, candidates);
        assert_eq!(json2.0["created"], 0);
        assert_eq!(json2.0["skipped"], 1);

        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn process_skips_empty_candidates() {
        let (server, db_path, conv_id) = test_server("process-empty");
        let candidates: Vec<ExtractedMemory> = vec![];

        let json = process_extracted_memories(&server, &conv_id, candidates);
        assert_eq!(json.0["ok"], true);
        assert_eq!(json.0["created"], 0);
        assert_eq!(json.0["skipped"], 0);

        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn process_skips_invalid_category() {
        let (server, db_path, conv_id) = test_server("process-invalid-cat");
        let candidates = vec![ExtractedMemory {
            category: "random".to_string(),
            content: "some content".to_string(),
        }];

        let json = process_extracted_memories(&server, &conv_id, candidates);
        assert_eq!(json.0["created"], 0);
        assert_eq!(json.0["skipped"], 1);

        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn process_skips_empty_content() {
        let (server, db_path, conv_id) = test_server("process-empty-content");
        let candidates = vec![ExtractedMemory {
            category: "note".to_string(),
            content: "   ".to_string(),
        }];

        let json = process_extracted_memories(&server, &conv_id, candidates);
        assert_eq!(json.0["created"], 0);
        assert_eq!(json.0["skipped"], 1);

        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn process_rejects_sensitive_content() {
        let (server, db_path, conv_id) = test_server("process-sensitive");
        let candidates = vec![ExtractedMemory {
            category: "knowledge".to_string(),
            content: "API key is sk-abc123".to_string(),
        }];

        let json = process_extracted_memories(&server, &conv_id, candidates);
        assert_eq!(json.0["created"], 0);
        assert_eq!(json.0["skipped"], 1);

        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn process_handles_chinese_content() {
        let (server, db_path, conv_id) = test_server("process-chinese");
        let candidates = vec![ExtractedMemory {
            category: "preference".to_string(),
            content: "用户长期使用中文内容进行交互，偏好简洁回复".to_string(),
        }];

        let json = process_extracted_memories(&server, &conv_id, candidates);
        let value = json.0;
        assert_eq!(value["ok"], true);
        assert_eq!(value["created"], 1);
        let saved = &value["memories"][0]["content"];
        assert!(saved.as_str().unwrap().contains("中文"));

        std::fs::remove_file(&db_path).ok();
    }
}
