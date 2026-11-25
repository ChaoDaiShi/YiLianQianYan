use std::sync::Arc;

use serde::Deserialize;
use yilian_backend::{
    config::types::ModelConfig,
    db::{
        memory_governance::{embedding_identity, CorrectMemoryRequest, WithdrawMemoryRequest},
        CreateMemoryRequest, Database, RetrieveQuery,
    },
    integrations::{
        llm::client::LlmClient,
        secret::{InMemorySecretStore, SecretResolver},
    },
    workspace::{Workspace, WorkspaceId},
};

#[derive(Deserialize)]
struct Cases {
    memories: Vec<Seed>,
    queries: Vec<Query>,
}

#[derive(Deserialize)]
struct Seed {
    id: String,
    content: String,
    category: String,
    scope: Option<String>,
    replace: Option<String>,
    #[serde(default)]
    withdraw: bool,
}

#[derive(Deserialize)]
struct Query {
    query: String,
    expected: Option<String>,
    excluded: Option<String>,
}

#[tokio::test]
#[ignore = "requires the installed offline BGE model and a running loopback service"]
async fn compares_real_local_embedding_with_lexical_baseline() {
    let url = std::env::var("YILIAN_MEMORY_QA_ENDPOINT").expect("set the local BGE service URL");
    let config = ModelConfig {
        embedding_model: "BAAI/bge-small-zh-v1.5".into(),
        embedding_base_url: url,
        ..Default::default()
    };
    assert!(config.has_local_embedding());
    let provider = LlmClient::new(
        &config,
        Arc::new(SecretResolver::new(Arc::new(InMemorySecretStore::new()))),
    );
    let identity = embedding_identity(&config);
    let path =
        std::env::temp_dir().join(format!("yilian-memory-quality-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&path).unwrap();
    let workspace = Workspace::new(
        WorkspaceId::generate(),
        "橘猫项目".into(),
        String::new(),
        None,
        0,
    )
    .unwrap();
    db.create_workspace(&workspace).unwrap();
    let mut ids = std::collections::HashMap::new();
    let cases: Cases =
        serde_json::from_str(include_str!("fixtures/local_memory_quality.json")).unwrap();
    for seed in cases.memories {
        let mut memory = db
            .create_memory(&CreateMemoryRequest {
                content: seed.content,
                category: seed.category,
                source: "manual".into(),
                source_conversation_id: None,
                metadata: None,
            })
            .unwrap();
        let revision = db.memory_governance(&memory.id).unwrap().revision;
        let ticket = db.begin_memory_embedding(&memory, &identity).unwrap();
        let vector = provider.embed(&memory.content).await.unwrap();
        assert_eq!(vector.len(), 512);
        db.complete_memory_embedding(&ticket, &vector).unwrap();
        if seed.scope.is_some() || seed.replace.is_some() {
            memory = db
                .correct_memory(
                    &memory.id,
                    &CorrectMemoryRequest {
                        content: seed
                            .replace
                            .unwrap_or_else(|| format!("{} 已确认。", memory.content)),
                        expected_revision: revision,
                        reason: "更新评测样例".into(),
                        scope: seed.scope.map(|_| format!("workspace:{}", workspace.id)),
                        source_ref: None,
                    },
                )
                .unwrap();
            assert!(db.complete_memory_embedding(&ticket, &vector).is_err());
        }
        if seed.withdraw {
            db.withdraw_memory(
                &memory.id,
                &WithdrawMemoryRequest {
                    expected_revision: revision,
                    reason: "撤回评测样例".into(),
                },
            )
            .unwrap();
        }
        ids.insert(seed.id, memory.id);
    }
    let to_rebuild = db.memories_needing_embedding(&identity, 100).unwrap();
    assert_eq!(to_rebuild.len(), 2);
    for memory in to_rebuild {
        let vector = provider.embed(&memory.content).await.unwrap();
        let ticket = db.begin_memory_embedding(&memory, &identity).unwrap();
        db.complete_memory_embedding(&ticket, &vector).unwrap();
    }

    let mut baseline = 0;
    let mut hybrid = 0;
    let mut expected_queries = 0;
    for case in cases.queries {
        let query = RetrieveQuery {
            q: case.query,
            top_k: 4,
            category: None,
        };
        let embedded = provider.embed_query(&query.q).await.unwrap();
        let lexical = db
            .retrieve_scoped_memories(&query, None, Some(&identity), None, true)
            .unwrap();
        let combined = db
            .retrieve_scoped_memories(&query, Some(&embedded), Some(&identity), None, true)
            .unwrap();
        if let Some(expected) = case.expected {
            expected_queries += 1;
            baseline += usize::from(
                lexical
                    .first()
                    .is_some_and(|row| row.memory.id == ids[&expected]),
            );
            hybrid += usize::from(
                combined
                    .first()
                    .is_some_and(|row| row.memory.id == ids[&expected]),
            );
        }
        if let Some(excluded) = case.excluded {
            assert!(lexical
                .iter()
                .chain(&combined)
                .all(|row| row.memory.id != ids[&excluded]));
        }
        assert!(combined
            .iter()
            .all(|row| row.memory.content != "库房盘点在星期五上午。"));
    }
    println!("Top-1 lexical={baseline}/{expected_queries}, hybrid={hybrid}/{expected_queries}");
    assert!(
        hybrid > baseline,
        "hybrid should improve the annotated fixture"
    );
    // Erase only the derived vector values and prove all active records are
    // materialized again from the retained source data and revisions.
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute("UPDATE memories SET embedding=NULL", [])
        .unwrap();
    drop(raw);
    let missing = db.memories_needing_embedding(&identity, 100).unwrap();
    assert_eq!(missing.len(), 6);
    for memory in missing {
        let vector = provider.embed(&memory.content).await.unwrap();
        let ticket = db.begin_memory_embedding(&memory, &identity).unwrap();
        db.complete_memory_embedding(&ticket, &vector).unwrap();
    }
    assert!(db
        .memories_needing_embedding(&identity, 100)
        .unwrap()
        .is_empty());
    drop(db);
    std::fs::remove_file(path).unwrap();
}
