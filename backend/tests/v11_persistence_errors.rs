use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::header::CONTENT_TYPE;
use axum::http::{Method, Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router as AxumRouter};
use tower::ServiceExt;

use yilian_backend::api::build_router;
use yilian_backend::app::state::AppServer;
use yilian_backend::safety::{ControlSession, CONTROL_SESSION_HEADER};

struct TestEnv {
    server: Arc<AppServer>,
    token: String,
    db_path: PathBuf,
}

impl TestEnv {
    fn new(label: &str) -> Self {
        let db_path = std::env::temp_dir().join(format!(
            "yilian-persistence-test-{label}-{}.db",
            uuid::Uuid::new_v4()
        ));
        let session = ControlSession::generate();
        let token = session.token().to_string();
        let server = AppServer::new_with_control_session(&db_path, ".", session).unwrap();
        Self {
            server: Arc::new(server),
            token,
            db_path,
        }
    }

    async fn request(
        &self,
        method: Method,
        uri: &str,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let builder = Request::builder()
            .method(method)
            .uri(uri)
            .header(CONTROL_SESSION_HEADER, &self.token)
            .header(CONTENT_TYPE, "application/json");

        let req_body = match body {
            Some(v) => Body::from(v.to_string()),
            None => Body::empty(),
        };

        let response = build_router(Arc::clone(&self.server))
            .oneshot(builder.body(req_body).unwrap())
            .await
            .unwrap();

        let status = response.status();
        let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap();
        let json = if bytes.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_slice(&bytes).unwrap_or(serde_json::json!({
                "raw": String::from_utf8_lossy(&bytes).to_string()
            }))
        };
        (status, json)
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.db_path);
    }
}

#[tokio::test]
async fn p5_01_create_conversation_failure_returns_500_and_no_fake_object() {
    let env = TestEnv::new("p5-01");

    // Install trigger to force write failure
    {
        let conn = env.server.db.conn();
        conn.execute(
            "CREATE TRIGGER fail_test_conversation_insert
             BEFORE INSERT ON conversations
             BEGIN SELECT RAISE(ABORT, 'test_write_failure'); END;",
            [],
        )
        .unwrap();
    }

    let (status, body) = env
        .request(
            Method::POST,
            "/api/conversations",
            Some(serde_json::json!({"title": "故障测试对话"})),
        )
        .await;

    // Must be HTTP 500, not 200
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["code"], "local_storage_unavailable");
    assert_eq!(body["error"], "本地数据暂时不可用，请重试");
    // Must NOT contain a newly minted conversation summary or id
    assert!(body.get("id").is_none());

    // Verify database has 0 conversation records
    let count: i64 = env
        .server
        .db
        .conn()
        .query_row("SELECT count(*) FROM conversations", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn p5_02_list_query_failure_vs_empty_database_distinction() {
    let env = TestEnv::new("p5-02");

    // 1. Fresh empty database: returns 200 with empty array []
    let (status, body) = env.request(Method::GET, "/api/conversations", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, serde_json::json!([]));

    // 2. Corrupt or unreadable database query
    {
        let conn = env.server.db.conn();
        conn.execute("DROP TABLE conversations", []).unwrap();
    }

    let (status, body) = env.request(Method::GET, "/api/conversations", None).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["code"], "local_storage_unavailable");
    assert_eq!(body["error"], "本地数据暂时不可用，请重试");
}

#[tokio::test]
async fn p5_03_delete_db_failure_returns_500_and_preserves_record() {
    let env = TestEnv::new("p5-03");

    // Create a conversation first
    let conv = env.server.db.create_conversation("待删除对话").unwrap();

    // Install trigger to prevent deletion
    {
        let conn = env.server.db.conn();
        conn.execute(
            "CREATE TRIGGER fail_test_conversation_delete
             BEFORE DELETE ON conversations
             BEGIN SELECT RAISE(ABORT, 'test_delete_failure'); END;",
            [],
        )
        .unwrap();
    }

    let (status, body) = env
        .request(
            Method::DELETE,
            &format!("/api/conversations/{}", conv.id),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["code"], "local_storage_unavailable");

    // Verify record is preserved in database
    let exists: bool = env
        .server
        .db
        .conn()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1)",
            [&conv.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(exists);
}

#[tokio::test]
async fn p5_04_get_not_found_404_vs_unreadable_500() {
    let env = TestEnv::new("p5-04");

    // 1. Conversation not found -> 404
    let (status, body) = env
        .request(Method::GET, "/api/conversations/non-existent-id", None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "not_found");
    assert_eq!(body["error"], "对话不存在");

    // 2. Memory not found -> 404
    let (status, body) = env
        .request(Method::GET, "/api/memories/non-existent-id", None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "not_found");
    assert_eq!(body["error"], "记忆不存在");

    // 3. Database table unreadable -> 500
    {
        let conn = env.server.db.conn();
        conn.execute("DROP TABLE conversations", []).unwrap();
    }
    let (status, body) = env
        .request(Method::GET, "/api/conversations/some-id", None)
        .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["code"], "local_storage_unavailable");
}

#[tokio::test]
async fn p5_05_memory_persisted_when_embedding_generation_fails() {
    let env = TestEnv::new("p5-05");

    // Point embedding configuration to a non-functional endpoint
    {
        let mut config = env.server.config.write();
        config.model.embedding_model = "test-model".to_string();
        config.model.embedding_base_url = "http://127.0.0.1:9".to_string();
        config.model.embedding_api_key = "test-key".to_string();
    }

    let (status, body) = env
        .request(
            Method::POST,
            "/api/memories",
            Some(serde_json::json!({
                "content": "用户的项目路径偏好",
                "category": "preference"
            })),
        )
        .await;

    // Status 200, returns persisted memory without embedding
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["content"], "用户的项目路径偏好");
    assert_eq!(body["category"], "preference");
    assert!(body["id"].is_string());

    let mem_id = body["id"].as_str().unwrap();

    // Verify exactly 1 memory exists in SQLite
    let mem = env.server.db.get_memory(mem_id).unwrap();
    assert_eq!(mem.content, "用户的项目路径偏好");
    assert!(mem.embedding.is_none());

    let total: i64 = env
        .server
        .db
        .conn()
        .query_row("SELECT count(*) FROM memories", [], |row| row.get(0))
        .unwrap();
    assert_eq!(total, 1);
}

#[tokio::test]
async fn p5_06_reindex_counting_failure_and_partial_failure() {
    let env = TestEnv::new("p5-06");

    // Create a mock embedding server that fails on the 2nd request
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = Arc::clone(&counter);
    let mock_app = AxumRouter::new().route(
        "/embeddings",
        post(move || {
            let cnt = counter_clone.fetch_add(1, Ordering::SeqCst);
            async move {
                if cnt == 1 {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "embed fail"})),
                    )
                } else {
                    (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "data": [{"embedding": [0.1, 0.2, 0.3]}]
                        })),
                    )
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    {
        let mut config = env.server.config.write();
        config.model.embedding_model = "test-model".to_string();
        config.model.embedding_base_url = addr;
        config.model.embedding_api_key = "test-key".to_string();
    }

    // Seed 2 memories
    let _ = env
        .server
        .db
        .create_memory(&yilian_backend::db::CreateMemoryRequest {
            content: "Memory 1".to_string(),
            category: "fact".to_string(),
            source: "manual".to_string(),
            source_conversation_id: None,
            metadata: None,
        })
        .unwrap();
    let _ = env
        .server
        .db
        .create_memory(&yilian_backend::db::CreateMemoryRequest {
            content: "Memory 2".to_string(),
            category: "fact".to_string(),
            source: "manual".to_string(),
            source_conversation_id: None,
            metadata: None,
        })
        .unwrap();

    // 1. Partial failure: returns 200 with actual succeeded & failed counts
    let (status, body) = env
        .request(
            Method::POST,
            "/api/memories/reindex",
            Some(serde_json::json!({"limit": 10})),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert_eq!(body["processed"], 2);
    assert_eq!(body["succeeded"], 1);
    assert_eq!(body["failed"], 1);
    assert_eq!(body["remaining"], 1);

    // 2. Counting / query failure: drop table or corrupt
    {
        let conn = env.server.db.conn();
        conn.execute("DROP TABLE memories", []).unwrap();
    }

    let (status, body) = env
        .request(
            Method::POST,
            "/api/memories/reindex",
            Some(serde_json::json!({"limit": 10})),
        )
        .await;

    // Must be non-2xx (500)
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["code"], "local_storage_unavailable");
}
