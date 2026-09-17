use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

const RESOURCE_BINDINGS: ProductMigrationSpec = ProductMigrationSpec::new(1010, "1010_resource_bindings", MigrationOwner::V1TaskWorld,
    "CREATE TABLE resource_bindings (id TEXT PRIMARY KEY, resource_id TEXT NOT NULL REFERENCES resources(id), target_kind TEXT NOT NULL, target_id TEXT NOT NULL, node_id TEXT NOT NULL DEFAULT '', owner TEXT NOT NULL, created_at INTEGER NOT NULL, UNIQUE(resource_id,target_kind,target_id,node_id,owner)); CREATE INDEX resource_bindings_target ON resource_bindings(target_kind,target_id,node_id,owner);");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourceTarget {
    Conversation { conversation_id: String },
    Task { task_id: String },
    Graph { graph_id: String },
    Node { graph_id: String, node_id: String },
}

impl ResourceTarget {
    fn parts(&self) -> (&str, &str, &str) {
        match self {
            Self::Conversation { conversation_id } => ("conversation", conversation_id, ""),
            Self::Task { task_id } => ("task", task_id, ""),
            Self::Graph { graph_id } => ("graph", graph_id, ""),
            Self::Node { graph_id, node_id } => ("node", graph_id, node_id),
        }
    }
}

fn validate_target(conn: &Connection, target: &ResourceTarget) -> Result<(), String> {
    let (kind, id, node_id) = target.parts();
    if id.is_empty() || id.len() > 128 || node_id.len() > 128 {
        return Err("绑定目标无效".into());
    }
    let exists = match kind {
        "conversation" => conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM conversations WHERE id=?1)",
            [id],
            |row| row.get::<_, bool>(0),
        ),
        "task" => conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
            [id],
            |row| row.get::<_, bool>(0),
        ),
        _ => {
            let json = conn
                .query_row(
                    "SELECT graph_json FROM task_world_supervisor_snapshots WHERE graph_id=?1",
                    [id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|_| "任务图不可用")?;
            let graph: crate::task::TaskGraph =
                serde_json::from_str(&json.ok_or("绑定任务图不存在")?).map_err(|_| "任务图无效")?;
            Ok(kind == "graph" || graph.nodes.iter().any(|node| node.id.as_str() == node_id))
        }
    }
    .map_err(|_| "无法检查绑定目标")?;
    if !exists {
        return Err("绑定目标不存在".into());
    }
    Ok(())
}

fn bindings_on(conn: &Connection, target: &ResourceTarget) -> Result<Vec<ResourceBinding>, String> {
    let (kind, id, node_id) = target.parts();
    let mut statement = conn.prepare("SELECT id,resource_id,created_at FROM resource_bindings WHERE target_kind=?1 AND target_id=?2 AND node_id=?3 AND owner='local-user' ORDER BY created_at,id").map_err(|_| "无法读取资源绑定")?;
    let rows = statement
        .query_map(params![kind, id, node_id], |row| {
            Ok(ResourceBinding {
                id: row.get(0)?,
                resource_id: row.get(1)?,
                target: target.clone(),
                created_at: row.get(2)?,
            })
        })
        .map_err(|_| "无法读取资源绑定")?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| "资源绑定记录无效".into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceBinding {
    pub id: String,
    pub resource_id: String,
    pub target: ResourceTarget,
    pub created_at: i64,
}

impl Database {
    pub fn initialize_resource_bindings(&self) -> Result<(), String> {
        self.apply_product_migration(&RESOURCE_BINDINGS)
            .map_err(|error| error.to_string())
    }
    pub fn bind_resources(
        &self,
        target: &ResourceTarget,
        resources: &[String],
    ) -> Result<Vec<ResourceBinding>, String> {
        if resources.is_empty() || resources.len() > crate::resource_input::MAX_INPUT_FILES {
            return Err("一次最多绑定 8 个资源".into());
        }
        self.initialize_resource_bindings()?;
        let mut conn = self.conn();
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| "无法开始资源绑定")?;
        validate_target(&transaction, target)?;
        let current = bindings_on(&transaction, target)?;
        let mut ids = current
            .iter()
            .map(|item| item.resource_id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        ids.extend(resources.iter().cloned());
        if ids.len() > crate::resource_input::MAX_INPUT_FILES {
            return Err("此目标最多绑定 8 个资源".into());
        }
        for resource in resources {
            let exists = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM resources WHERE id=?1)",
                    [resource],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(|_| "无法检查资源")?;
            if !exists {
                return Err("资源不存在，绑定未保存".into());
            }
        }
        let (kind, id, node_id) = target.parts();
        for resource in resources {
            transaction.execute("INSERT OR IGNORE INTO resource_bindings(id,resource_id,target_kind,target_id,node_id,owner,created_at) VALUES (?1,?2,?3,?4,?5,'local-user',?6)", params![uuid::Uuid::new_v4().to_string(),resource,kind,id,node_id,chrono::Utc::now().timestamp_millis()]).map_err(|_| "资源绑定保存失败")?;
        }
        let result = bindings_on(&transaction, target)?;
        transaction.commit().map_err(|_| "资源绑定提交失败")?;
        Ok(result)
    }
    pub fn resource_bindings(
        &self,
        target: &ResourceTarget,
    ) -> Result<Vec<ResourceBinding>, String> {
        self.initialize_resource_bindings()?;
        let conn = self.conn();
        validate_target(&conn, target)?;
        bindings_on(&conn, target)
    }
    pub fn unbind_resource(&self, id: &str) -> Result<bool, String> {
        self.initialize_resource_bindings()?;
        self.conn()
            .execute(
                "DELETE FROM resource_bindings WHERE id=?1 AND owner='local-user'",
                [id],
            )
            .map(|count| count > 0)
            .map_err(|_| "解除资源绑定失败".into())
    }

    pub fn managed_resource_storage_root(&self) -> Result<std::path::PathBuf, String> {
        let conn = self.conn();
        let path = conn
            .path()
            .filter(|path| !path.is_empty())
            .ok_or("当前数据库没有托管资源目录")?;
        Ok(std::path::Path::new(path)
            .parent()
            .ok_or("资源目录不可用")?
            .join("resources"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        shared::{event::EventHub, resource::ResourceService},
        task::{TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind, TaskWorldRuntime},
    };
    #[test]
    fn resource_bindings_are_explicit_independent_and_bounded() {
        let path = std::env::temp_dir().join(format!("resource-bindings-{}", uuid::Uuid::new_v4()));
        let db = Database::new(&path.join("test.db")).unwrap();
        db.initialize_resource_bindings().unwrap();
        let runtime = TaskWorldRuntime::new(&db, EventHub::new(8)).unwrap();
        runtime
            .create_graph(
                TaskGraphId::new("graph").unwrap(),
                vec![TaskNode::new(
                    TaskNodeId::new("node").unwrap(),
                    TaskNodeKind::Work,
                    "Node",
                    serde_json::json!({}),
                )
                .unwrap()],
                vec![],
                1,
            )
            .unwrap();
        let service = ResourceService::new(db.clone(), path.join("resources"), EventHub::new(2));
        let resource = service
            .ingest(
                "notes.txt",
                "text/plain",
                b"bound content",
                serde_json::json!({}),
            )
            .unwrap();
        let graph = ResourceTarget::Graph {
            graph_id: "graph".into(),
        };
        let node = ResourceTarget::Node {
            graph_id: "graph".into(),
            node_id: "node".into(),
        };
        assert_eq!(
            db.bind_resources(&graph, &[resource.id.clone()])
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            db.bind_resources(&node, &[resource.id.clone()])
                .unwrap()
                .len(),
            1
        );
        assert_eq!(db.resource_bindings(&graph).unwrap().len(), 1);
        let binding = db.resource_bindings(&node).unwrap().remove(0);
        assert!(db.unbind_resource(&binding.id).unwrap());
        assert_eq!(db.resource_bindings(&graph).unwrap().len(), 1);
        assert!(db
            .bind_resources(
                &ResourceTarget::Node {
                    graph_id: "graph".into(),
                    node_id: "missing".into()
                },
                &[resource.id.clone()]
            )
            .is_err());
        assert!(db.bind_resources(&graph, &vec![resource.id; 9]).is_err());
        let _ = std::fs::remove_dir_all(path);
    }
}
