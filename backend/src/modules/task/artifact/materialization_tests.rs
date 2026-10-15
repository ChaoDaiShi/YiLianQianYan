use super::*;
use crate::{
    modules::task::{TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind, TaskWorldRuntime},
    shared::event::EventHub,
    workspace::Workspace,
};
use serde_json::json;
#[test]
fn artifact_materialization_persists_real_bytes_versions_and_provenance() {
    let root = std::env::temp_dir().join(format!("artifact-result-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let db = Database::new(&root.join("test.db")).unwrap();
    let workspace = Workspace::new(
        WorkspaceId::generate(),
        "Result workspace".into(),
        String::new(),
        Some(root.to_string_lossy().into()),
        1,
    )
    .unwrap();
    db.create_workspace(&workspace).unwrap();
    let runtime = TaskWorldRuntime::new(&db, EventHub::new(8)).unwrap();
    let graph = TaskGraphId::new("graph").unwrap();
    let node = TaskNodeId::new("node").unwrap();
    runtime
        .create_graph(
            graph.clone(),
            vec![TaskNode::new(
                node.clone(),
                TaskNodeKind::Work,
                "Actual local output",
                json!({}),
            )
            .unwrap()],
            vec![],
            1,
        )
        .unwrap();
    let execution = runtime.start_execution(&graph, &node, 1, 2).unwrap();
    let source = ArtifactSource::Node {
        graph_id: graph.to_string(),
        node_id: node.to_string(),
        execution_id: execution.id.to_string(),
        workspace_id: workspace.id.to_string(),
    };
    let service = ArtifactService::new(db.clone());
    assert!(service
        .materialize(&source, "Result", root.to_str().unwrap(), 3)
        .is_err());
    runtime
        .complete_execution(
            &graph,
            &execution.id,
            json!({"ok":true,"result":"Verified result bytes"}),
            crate::modules::task::validation::ValidationPolicy::StructuredResult,
            4,
        )
        .unwrap();
    let (first, provenance) = service
        .materialize(&source, "Result", root.to_str().unwrap(), 5)
        .unwrap();
    assert_eq!(provenance.version, 1);
    let (_, bytes) = service
        .read_bytes(&first.id, root.to_str().unwrap())
        .unwrap();
    assert!(String::from_utf8(bytes)
        .unwrap()
        .contains("Verified result bytes"));
    let (_, second) = service
        .materialize(&source, "Result", root.to_str().unwrap(), 6)
        .unwrap();
    assert_eq!(second.version, 2);
    assert!(db.get_artifact(&first.id).unwrap().is_some());
    std::fs::write(first.path.as_ref().unwrap(), b"tampered").unwrap();
    assert!(service
        .read_bytes(&first.id, root.to_str().unwrap())
        .is_err());
    assert!(db.get_artifact(&first.id).unwrap().is_some());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn explicit_rendering_stores_real_source_bytes_with_matching_extension_and_hash() {
    let root = std::env::temp_dir().join(format!("artifact-rendered-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let db = Database::new(&root.join("test.db")).unwrap();
    let workspace = Workspace::new(
        WorkspaceId::generate(),
        "Rendered output workspace".into(),
        String::new(),
        Some(root.to_string_lossy().into()),
        1,
    )
    .unwrap();
    db.create_workspace(&workspace).unwrap();
    let runtime = TaskWorldRuntime::new(&db, EventHub::new(8)).unwrap();
    let graph = TaskGraphId::new("render-graph").unwrap();
    let node = TaskNodeId::new("render-node").unwrap();
    runtime
        .create_graph(
            graph.clone(),
            vec![TaskNode::new(
                node.clone(),
                TaskNodeKind::Work,
                "Return a structured report",
                json!({}),
            )
            .unwrap()],
            vec![],
            1,
        )
        .unwrap();
    let execution = runtime.start_execution(&graph, &node, 1, 2).unwrap();
    let source = ArtifactSource::Node {
        graph_id: graph.to_string(),
        node_id: node.to_string(),
        execution_id: execution.id.to_string(),
        workspace_id: workspace.id.to_string(),
    };
    let structured = json!({
        "report": {
            "title": "实测报告",
            "sections": [{"heading": "结果", "paragraphs": ["来自实际输出"], "table": null}],
            "sources": []
        }
    });
    runtime
        .complete_execution(
            &graph,
            &execution.id,
            json!({"ok":true,"content":"实际工具正文","structured":structured}),
            crate::modules::task::validation::ValidationPolicy::StructuredResult,
            4,
        )
        .unwrap();

    let service = ArtifactService::new(db.clone());
    let (json_artifact, provenance) = service
        .materialize_as(
            &source,
            "实测报告.json",
            DeliverableFormat::Json,
            root.to_str().unwrap(),
            5,
        )
        .unwrap();
    let (read_json, bytes) = service
        .read_bytes(&json_artifact.id, root.to_str().unwrap())
        .unwrap();
    assert_eq!(read_json.mime_type.as_deref(), Some("application/json"));
    assert_eq!(
        Path::new(read_json.path.as_deref().unwrap())
            .extension()
            .unwrap(),
        "json"
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
        structured
    );
    assert_eq!(provenance.sha256, format!("{:x}", Sha256::digest(&bytes)));
    assert_eq!(
        db.artifact_provenance(json_artifact.id.as_str())
            .unwrap()
            .unwrap()
            .sha256,
        provenance.sha256
    );

    let (markdown_artifact, _) = service
        .materialize_as(
            &source,
            "正文",
            DeliverableFormat::Markdown,
            root.to_str().unwrap(),
            6,
        )
        .unwrap();
    let (read_markdown, bytes) = service
        .read_bytes(&markdown_artifact.id, root.to_str().unwrap())
        .unwrap();
    assert_eq!(read_markdown.mime_type.as_deref(), Some("text/markdown"));
    assert_eq!(
        Path::new(read_markdown.path.as_deref().unwrap())
            .extension()
            .unwrap(),
        "md"
    );
    assert_eq!(bytes, "# 实测报告\n\n## 结果\n\n来自实际输出\n".as_bytes());
    let _ = std::fs::remove_dir_all(root);
}
