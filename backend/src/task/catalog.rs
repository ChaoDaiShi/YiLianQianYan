use crate::db::{Database, TaskWorldPersistenceError};
use crate::shared::context::{ContextRequest, TaskProjection, TaskProjectionProvider};

use super::TaskProjectionProviderAdapter;

/// Read-only catalog of persisted Task World projections.
///
/// The catalog owns only shared-contract projections. Graph definitions and
/// node states stay inside the v1 Task World and never cross this boundary.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskWorldProjectionCatalog {
    projections: Vec<TaskProjection>,
}

impl TaskWorldProjectionCatalog {
    pub fn load(database: &Database) -> Result<Self, TaskWorldPersistenceError> {
        database.initialize_task_world_schema()?;
        let snapshots = database.load_all_task_supervisor_snapshots()?;
        let mut projections = snapshots
            .iter()
            .map(|snapshot| TaskProjectionProviderAdapter::new(&snapshot.supervisor).project())
            .collect::<Vec<_>>();
        projections.sort_unstable_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| left.title.cmp(&right.title))
        });
        Ok(Self { projections })
    }

    pub fn projections(&self) -> &[TaskProjection] {
        &self.projections
    }
}

impl TaskProjectionProvider for TaskWorldProjectionCatalog {
    fn list(&self, request: &ContextRequest) -> Vec<TaskProjection> {
        if request.max_items == 0 {
            return Vec::new();
        }

        let query = request.query.as_deref().map(str::to_lowercase);
        self.projections
            .iter()
            .filter(|projection| {
                query.as_deref().map_or(true, |query| {
                    projection.id.to_lowercase().contains(query)
                        || projection.title.to_lowercase().contains(query)
                })
            })
            .take(request.max_items)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::shared::context::{ContextRequest, TaskProjectionProvider};
    use crate::task::{
        GraphRevision, TaskGraph, TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind, TaskSupervisor,
    };
    use serde_json::json;
    use std::path::Path;

    fn supervisor(id: &str, node_title: &str) -> TaskSupervisor {
        let graph = TaskGraph::new(
            TaskGraphId::new(id).expect("valid graph id"),
            GraphRevision::initial(),
            vec![TaskNode::new(
                TaskNodeId::new(format!("{id}-node")).expect("valid node id"),
                TaskNodeKind::Work,
                node_title,
                json!({ "source": "catalog-test" }),
            )
            .expect("valid task node")],
            Vec::new(),
        )
        .expect("valid task graph");
        TaskSupervisor::new(graph, 100).expect("valid supervisor")
    }

    #[test]
    fn catalog_loads_empty_and_projects_sorted_snapshots_with_query_bounds() {
        let database = Database::new(Path::new(":memory:")).expect("database opens");
        let empty = TaskWorldProjectionCatalog::load(&database).expect("empty catalog loads");
        assert!(empty.list(&ContextRequest::new("task-world")).is_empty());

        let zeta = supervisor("zeta-graph", "Only zeta node");
        let alpha = supervisor("alpha-graph", "Only alpha node");
        database
            .save_task_supervisor_snapshot(&zeta, 300)
            .expect("zeta snapshot saves");
        database
            .save_task_supervisor_snapshot(&alpha, 400)
            .expect("alpha snapshot saves");

        let catalog = TaskWorldProjectionCatalog::load(&database).expect("catalog loads");
        let expected = catalog.list(&ContextRequest::new("task-world"));
        assert_eq!(
            expected
                .iter()
                .map(|projection| projection.id.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha-graph", "zeta-graph"]
        );
        assert!(expected
            .iter()
            .all(|projection| !projection.simulation.simulated));

        let mut request = ContextRequest::new("unrelated-scope");
        request.query = Some("Only alpha node".to_string());
        assert!(catalog.list(&request).is_empty());
        request.query = Some("TASK GRAPH ZETA-GRAPH".to_string());
        assert_eq!(catalog.list(&request).len(), 1);

        request.query = None;
        request.max_items = 1;
        let limited = catalog.list(&request);
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].id, "alpha-graph");

        let reloaded = TaskWorldProjectionCatalog::load(&database).expect("catalog reloads");
        assert_eq!(reloaded.list(&ContextRequest::new("task-world")), expected);
    }
}
