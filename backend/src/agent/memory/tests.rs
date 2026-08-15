// ============================================================
// Agent Memory Runtime tests.
// ============================================================

use super::context::*;
use super::*;
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

// ============================================================
// Phase 2 — learning loop tests.
// ============================================================

use crate::task::model::TaskId;

fn candidate(content: &str, category: MemoryCategory, confidence: f32) -> MemoryCandidate {
    MemoryCandidate::new(
        category,
        content,
        Some(TaskId::generate()),
        "researcher",
        confidence,
    )
}

#[test]
fn category_serde_and_parse_roundtrip() {
    for (variant, text) in [
        (MemoryCategory::Fact, "fact"),
        (MemoryCategory::Preference, "preference"),
        (MemoryCategory::Knowledge, "knowledge"),
        (MemoryCategory::Note, "note"),
    ] {
        assert_eq!(variant.as_str(), text);
        assert_eq!(text.parse::<MemoryCategory>().unwrap(), variant);
        assert_eq!(
            serde_json::to_value(variant).unwrap(),
            serde_json::json!(text)
        );
    }
    assert!("unknown".parse::<MemoryCategory>().is_err());
}

#[test]
fn validate_candidate_rejects_invalid_content() {
    let policy = MemoryWritePolicy::default();
    let empty: Vec<Memory> = Vec::new();

    // Empty.
    assert_eq!(
        validate_candidate(
            &candidate("   ", MemoryCategory::Fact, 0.9),
            &policy,
            &empty
        ),
        Err(ValidationError::EmptyContent)
    );
    // Too short.
    assert_eq!(
        validate_candidate(&candidate("hi", MemoryCategory::Fact, 0.9), &policy, &empty),
        Err(ValidationError::ContentTooShort)
    );
    // Too long.
    let long = "x".repeat(DEFAULT_MAX_CONTENT_CHARS + 1);
    assert_eq!(
        validate_candidate(
            &candidate(&long, MemoryCategory::Note, 0.9),
            &policy,
            &empty
        ),
        Err(ValidationError::ContentTooLong(DEFAULT_MAX_CONTENT_CHARS))
    );
    // Low confidence.
    assert_eq!(
        validate_candidate(
            &candidate("valid content", MemoryCategory::Fact, 0.1),
            &policy,
            &empty
        ),
        Err(ValidationError::LowConfidence)
    );
    // Secret marker.
    assert_eq!(
        validate_candidate(
            &candidate("my api_key is abc123", MemoryCategory::Note, 0.9),
            &policy,
            &empty
        ),
        Err(ValidationError::ContainsSecret)
    );
    // Valid.
    assert_eq!(
        validate_candidate(
            &candidate("用户偏好简洁回答", MemoryCategory::Preference, 0.8),
            &policy,
            &empty
        ),
        Ok(())
    );
}

#[test]
fn validate_candidate_rejects_duplicates() {
    let policy = MemoryWritePolicy::default();
    let existing = vec![Memory {
        id: "m1".to_string(),
        content: "用户偏好简洁回答".to_string(),
        category: "preference".to_string(),
        source: "manual".to_string(),
        source_conversation_id: None,
        embedding: None,
        metadata: None,
        created_at: 1,
        updated_at: 1,
    }];
    assert_eq!(
        validate_candidate(
            &candidate("用户偏好简洁回答", MemoryCategory::Preference, 0.8),
            &policy,
            &existing
        ),
        Err(ValidationError::Duplicate)
    );
}

#[tokio::test]
async fn writer_stores_valid_candidates_and_rejects_invalid() {
    let (path, db) = temp_db("writer");
    let writer = MemoryWriter::new(
        db.clone_connection(),
        &ModelConfig::default(),
        MemoryWritePolicy::default(),
    );

    let report = writer
        .write(vec![
            candidate("用户偏好简洁回答", MemoryCategory::Preference, 0.8),
            candidate("my password is hunter2", MemoryCategory::Note, 0.9), // secret → rejected
            candidate("hi", MemoryCategory::Fact, 0.9),                     // too short → rejected
        ])
        .await;

    assert_eq!(report.stored, 1);
    assert_eq!(report.rejected, 2);
    assert_eq!(report.failed, 0);

    // The stored memory is persisted with agent provenance metadata.
    let stored = db
        .list_memories(&crate::db::MemoryQuery {
            category: None,
            source: None,
            q: None,
            limit: Some(10),
            offset: None,
        })
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].category, "preference");
    assert_eq!(stored[0].source, "auto");
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn writer_skips_duplicate_across_runs() {
    let (path, db) = temp_db("writer-dup");
    let writer = MemoryWriter::new(
        db.clone_connection(),
        &ModelConfig::default(),
        MemoryWritePolicy::default(),
    );

    let first = writer
        .write(vec![candidate(
            "复用经验 A",
            MemoryCategory::Knowledge,
            0.7,
        )])
        .await;
    assert_eq!(first.stored, 1);

    let second = writer
        .write(vec![candidate(
            "复用经验 A",
            MemoryCategory::Knowledge,
            0.7,
        )])
        .await;
    assert_eq!(second.stored, 0);
    assert_eq!(second.rejected, 1);
    let _ = std::fs::remove_file(&path);
}

// ============================================================
// Phase 2 Closure — deterministic reflection + dedup + secret + retrieval.
// ============================================================

use crate::task::model::TaskStatus;
use tokio_util::sync::CancellationToken;

fn reflection_input(status: TaskStatus) -> ReflectionInput {
    ReflectionInput {
        task_id: TaskId::generate(),
        task_title: "实现 MCP Runtime".to_string(),
        task_description: "为 MCP 增加 stdio transport".to_string(),
        task_status: status,
        agent_name: "researcher".to_string(),
        agent_result_summaries: vec!["完成了 stdio transport 设计".to_string()],
        artifact_summaries: vec![],
        error_summary: None,
    }
}

async fn reflect(input: ReflectionInput) -> Vec<MemoryCandidate> {
    let cancel = CancellationToken::new();
    DeterministicMemoryReflector::new()
        .reflect(input, &cancel)
        .await
        .unwrap()
}

#[test]
fn deterministic_reflector_does_not_require_llm() {
    // No ModelConfig / LlmClient is constructed anywhere in the deterministic
    // reflector — it is a pure function of the input.
    let input = reflection_input(TaskStatus::Completed);
    let candidates = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(reflect(input));
    assert!(!candidates.is_empty());
    // All candidates map to knowledge (never preference).
    assert!(candidates
        .iter()
        .all(|c| c.category == MemoryCategory::Knowledge));
}

#[tokio::test]
async fn completed_task_generates_memory() {
    let candidates = reflect(reflection_input(TaskStatus::Completed)).await;
    assert!(!candidates.is_empty());
    assert!(candidates[0].content.contains("实现 MCP Runtime"));
}

#[tokio::test]
async fn failed_task_generates_failure_memory() {
    let mut input = reflection_input(TaskStatus::Failed);
    input.agent_result_summaries.clear();
    input.error_summary = Some("连接 stdio server 失败".to_string());
    let candidates = reflect(input).await;
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].category, MemoryCategory::Note);
    assert!(candidates[0].content.contains("失败"));
}

#[tokio::test]
async fn non_terminal_and_cancelled_generate_no_memory() {
    for status in [
        TaskStatus::Cancelled,
        TaskStatus::Blocked,
        TaskStatus::WaitingApproval,
        TaskStatus::WaitingUser,
        TaskStatus::Running,
    ] {
        let candidates = reflect(reflection_input(status)).await;
        assert!(
            candidates.is_empty(),
            "status {status} should produce nothing"
        );
    }
}

#[tokio::test]
async fn artifact_summary_generates_knowledge_and_empty_does_not() {
    let mut input = reflection_input(TaskStatus::Completed);
    input.agent_result_summaries.clear();
    input.task_description.clear();
    input.artifact_summaries = vec!["设计文档".to_string()];
    let candidates = reflect(input).await;
    assert!(!candidates.is_empty());
    assert!(candidates.iter().all(|c| c.content.contains("产物")));

    // Empty artifact summary produces nothing.
    let mut empty = reflection_input(TaskStatus::Completed);
    empty.agent_result_summaries.clear();
    empty.task_description.clear();
    empty.artifact_summaries = vec!["  ".to_string()];
    assert!(reflect(empty).await.is_empty());
}

#[tokio::test]
async fn candidate_count_is_bounded_to_five() {
    let mut input = reflection_input(TaskStatus::Completed);
    input.agent_result_summaries.clear();
    input.artifact_summaries = (0..10).map(|i| format!("产物{i}")).collect();
    let candidates = reflect(input).await;
    assert!(candidates.len() <= MAX_MEMORY_CANDIDATES_PER_EXECUTION);
}

#[test]
fn secret_filter_rejects_all_markers() {
    for secret in [
        "my api_key is abc",
        "the api key is abc",
        "token is deadbeef",
        "Authorization: Bearer xxx",
        "password: hunter2",
        "secret value",
        "cookie session=abc",
        "sk-1234567890",
    ] {
        assert!(
            crate::safety::contains_sensitive_content(secret),
            "should reject: {secret}"
        );
    }
    assert!(!crate::safety::contains_sensitive_content(
        "用户偏好简洁回答"
    ));
}

#[test]
fn sanitize_failure_summary_bounds_and_rejects_secret() {
    // Normal message is bounded and preserved.
    let summary = sanitize_failure_summary("连接失败").unwrap();
    assert!(summary.contains("连接失败"));

    // Overlong message is bounded.
    let long = "x".repeat(2000);
    let bounded = sanitize_failure_summary(&long).unwrap();
    assert!(bounded.chars().count() <= 1003); // 1000 + "..." marker

    // Secret-bearing message is rejected entirely.
    assert_eq!(sanitize_failure_summary("error: token=abc123"), None);
}

#[tokio::test]
async fn same_batch_duplicate_is_stored_once() {
    let (path, db) = temp_db("same-batch-dup");
    let writer = MemoryWriter::new(
        db.clone_connection(),
        &ModelConfig::default(),
        MemoryWritePolicy::default(),
    );
    let report = writer
        .write(vec![
            candidate("同批重复经验", MemoryCategory::Knowledge, 0.8),
            candidate("同批重复经验", MemoryCategory::Knowledge, 0.8),
        ])
        .await;
    assert_eq!(report.stored, 1);
    assert_eq!(report.rejected, 1);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn learned_memory_is_retrievable_by_future_agent() {
    let (path, db) = temp_db("closure");

    // Execution #1: completed task → deterministic reflection → write.
    let writer = MemoryWriter::new(
        db.clone_connection(),
        &ModelConfig::default(),
        MemoryWritePolicy::default(),
    );
    let candidates = reflect(reflection_input(TaskStatus::Completed)).await;
    let report = writer.write(candidates).await;
    assert!(report.stored >= 1);

    // Execution #2: a related future task retrieves the learned memory.
    let builder = MemoryContextBuilder::new(db.clone_connection(), &ModelConfig::default());
    let context = builder
        .build("实现 MCP Runtime", "stdio transport", "设计")
        .await
        .unwrap();
    // Lexical retrieval must hit the newly-learned memory content.
    let hit = context
        .memories
        .iter()
        .any(|m| m.memory.content.contains("实现 MCP Runtime"));
    assert!(hit, "future retrieval must hit the learned memory");
    let _ = std::fs::remove_file(&path);
}
