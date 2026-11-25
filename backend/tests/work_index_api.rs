//! HTTP projection regression with a real isolated confirmed goal, no model.
#[allow(dead_code)]
#[path = "support/shared_goal_activity.rs"]
mod support;

use serde_json::Value;
use yilian_backend::modules::task::TaskGraphId;

#[tokio::test]
async fn work_index_uses_confirmed_user_goal_title_and_preserves_source_identity() {
    let (server, contract, graph) = support::setup();
    let plain = TaskGraphId::new("plain-work-index-graph").unwrap();
    server
        .task_world
        .create_graph(plain.clone(), vec![], vec![], support::now())
        .unwrap();
    let fallback = server
        .task_world
        .status_projection(&plain, support::now())
        .unwrap()
        .title;

    let (status, body) = support::call(&server, "GET", "/api/work-index", Value::Null).await;
    assert_eq!(status, 200, "{body}");
    let items = body["items"].as_array().expect("work index items");
    let confirmed = items
        .iter()
        .find(|item| item["kind"] == "task_graph" && item["id"] == graph.as_str())
        .expect("confirmed goal item");
    assert_eq!(confirmed["title"], contract.user_goal);
    assert_eq!(confirmed["source_ids"]["graph_id"], graph.as_str());
    assert_eq!(confirmed["href"], format!("/task-world/{graph}"));
    let unbound = items
        .iter()
        .find(|item| item["kind"] == "task_graph" && item["id"] == plain.as_str())
        .expect("plain graph item");
    assert_eq!(unbound["title"], fallback);
}
