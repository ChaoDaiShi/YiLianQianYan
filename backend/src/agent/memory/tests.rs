// ============================================================
// Agent Memory Runtime tests.
// ============================================================

use super::context::*;
use crate::config::types::ModelConfig;
use crate::db::{CreateMemoryRequest, Database, Memory, ScoredMemory};

fn temp_db(label: &str) -> (std::path::PathBuf, Database) {
    let path = std::env::temp_dir().join(format!(
        "yilian-agent-memory-{label}-{}.db",
        uuid::Uuid::new_v4()
    ));
    let db = Database::new(&path).unwrap();
    (path, db)
}

fn scored(content: &str, category: &str) -> ScoredMemory {
    ScoredMemory {
        memory: Memory {
            id: uuid::Uuid::new_v4().to_string(),
            content: content.to_string(),
            category: category.to_string(),
            source: "manual".to_string(),
            source_conversation_id: None,
            embedding: None,
            metadata: None,
            created_at: 1,
            updated_at: 1,
        },
        score: 0.9,
        lexical_score: 0.9,
        vector_score: 0.0,
    }
}

#[test]
fn build_memory_query_joins_and_truncates() {
    assert_eq!(build_memory_query("标题", "描述", "指令"), "标题 描述 指令");
    // Empty parts are skipped.
    assert_eq!(build_memory_query("", "", "仅指令"), "仅指令");
    // Overlong query is char-safe truncated (with a "..." suffix marker).
    let long = "x".repeat(MAX_MEMORY_QUERY_CHARS + 100);
    let q = build_memory_query(&long, "", "");
    assert!(q.chars().count() <= MAX_MEMORY_QUERY_CHARS + 3);
    assert!(q.ends_with("..."));
}

#[test]
fn build_context_text_renders_and_truncates() {
    let empty: Vec<ScoredMemory> = Vec::new();
    assert_eq!(build_context_text(&empty), "");

    let text = build_context_text(&[scored("用户偏好简洁回答", "preference")]);
    assert_eq!(text, "- [preference] 用户偏好简洁回答");

    // Overlong content is char-safe truncated (not byte-split), with a "..."
    // suffix marker appended by `truncate_chars`.
    let long_content = "忆".repeat(MAX_MEMORY_CONTEXT_CHARS + 50);
    let text = build_context_text(&[scored(&long_content, "fact")]);
    assert!(text.chars().count() <= MAX_MEMORY_CONTEXT_CHARS + 3);
    assert!(text.ends_with("..."));
}

#[tokio::test]
async fn build_retrieves_lexical_without_embedding() {
    let (path, db) = temp_db("lexical");
    db.create_memory(&CreateMemoryRequest {
        content: "用户要求代码审查要关注安全漏洞".to_string(),
        category: "preference".to_string(),
        source: "manual".to_string(),
        source_conversation_id: None,
        metadata: None,
    })
    .unwrap();
    db.create_memory(&CreateMemoryRequest {
        content: "与当前任务无关的随机内容".to_string(),
        category: "note".to_string(),
        source: "manual".to_string(),
        source_conversation_id: None,
        metadata: None,
    })
    .unwrap();

    // No embedding configured → lexical mode, no network.
    let builder = MemoryContextBuilder::new(db, &ModelConfig::default());
    let context = builder
        .build("代码审查", "关注安全漏洞", "审查")
        .await
        .unwrap();

    assert_eq!(context.mode, MemoryRetrievalMode::Lexical);
    assert!(!context.memories.is_empty());
    assert!(context.injected_text.contains("安全漏洞"));
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn build_returns_empty_context_when_no_memories() {
    let (path, db) = temp_db("empty");
    let builder = MemoryContextBuilder::new(db, &ModelConfig::default());
    let context = builder.build("任意任务", "", "").await.unwrap();
    assert!(context.memories.is_empty());
    assert_eq!(context.injected_text, "");
    let _ = std::fs::remove_file(&path);
}
