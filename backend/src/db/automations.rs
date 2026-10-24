use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use crate::modules::automation::model::{Automation, Occurrence};
use rusqlite::{params, OptionalExtension};

const SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    1052, "1052_automations", MigrationOwner::V1TaskWorld,
    "CREATE TABLE automations(id TEXT PRIMARY KEY, payload TEXT NOT NULL);
     CREATE TABLE automation_occurrences(id TEXT PRIMARY KEY,automation_id TEXT NOT NULL,occurrence_key TEXT NOT NULL,payload TEXT NOT NULL,created_at INTEGER NOT NULL,UNIQUE(automation_id,occurrence_key));
     CREATE INDEX automation_history ON automation_occurrences(automation_id,created_at DESC);",
);
const AGENT_TURNS:ProductMigrationSpec=ProductMigrationSpec::new(1065,"1065_automation_agent_turns",MigrationOwner::V1TaskWorld,
    "UPDATE automations SET payload=json_set(payload,'$.target',json_object('kind','workflow','graph_id',json_extract(payload,'$.workflow_graph_id'))) WHERE json_type(payload,'$.target') IS NULL;
     UPDATE automation_occurrences SET payload=json_set(payload,'$.robot_run_id',NULL) WHERE json_type(payload,'$.robot_run_id') IS NULL;");

impl Database {
    pub fn initialize_automations_schema(&self) -> Result<(), rusqlite::Error> {
        self.apply_product_migration(&SCHEMA)?;
        self.apply_product_migration(&AGENT_TURNS)
    }
    pub fn list_automations(&self) -> Result<Vec<Automation>, String> {
        let conn = self.conn();
        let mut statement = conn
            .prepare("SELECT payload FROM automations ORDER BY rowid DESC")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|row| {
            serde_json::from_str(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect()
    }
    pub fn get_automation(&self, id: &str) -> Result<Option<Automation>, String> {
        let payload: Option<String> = self
            .conn()
            .query_row("SELECT payload FROM automations WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())?;
        payload
            .map(|value| serde_json::from_str(&value).map_err(|e| e.to_string()))
            .transpose()
    }
    pub fn save_automation(&self, value: &Automation, create: bool) -> Result<(), String> {
        let payload = serde_json::to_string(value).map_err(|e| e.to_string())?;
        let query = if create {
            "INSERT INTO automations(id,payload) VALUES(?1,?2)"
        } else {
            "UPDATE automations SET payload=?2 WHERE id=?1"
        };
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        if !create {
            let stored: String = tx
                .query_row(
                    "SELECT payload FROM automations WHERE id=?1",
                    [&value.id],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            let current: Automation = serde_json::from_str(&stored).map_err(|e| e.to_string())?;
            if value.revision != current.revision + 1 {
                return Err("时间任务版本已变化，请刷新后重试".into());
            }
        }
        let count = tx
            .execute(query, params![value.id, payload])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        if count == 0 {
            return Err("时间任务不存在".into());
        }
        Ok(())
    }
    pub fn delete_automation(&self, id: &str) -> Result<bool, String> {
        Ok(self
            .conn()
            .execute("DELETE FROM automations WHERE id=?1", [id])
            .map_err(|e| e.to_string())?
            > 0)
    }
    /// Claim and advance the schedule in one transaction. A repeated tick cannot
    /// dispatch the same occurrence, including after process restart.
    pub fn claim_automation_occurrence(
        &self,
        expected: &Automation,
        next: Option<i64>,
        occurrence: &mut Occurrence,
        manual: bool,
    ) -> Result<bool, String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let payload: Option<String> = tx
            .query_row(
                "SELECT payload FROM automations WHERE id=?1",
                [&expected.id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let Some(payload) = payload else {
            return Ok(false);
        };
        let mut current: Automation = serde_json::from_str(&payload).map_err(|e| e.to_string())?;
        if current.revision != expected.revision
            || (!manual
                && (!current.enabled || current.next_run_at != Some(occurrence.scheduled_at)))
        {
            return Ok(false);
        }
        let mut statement = tx
            .prepare("SELECT payload FROM automation_occurrences WHERE automation_id=?1")
            .map_err(|e| e.to_string())?;
        let previous = statement
            .query_map([&expected.id], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let mut active = false;
        for value in previous {
            let item: Occurrence = serde_json::from_str(&value.map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            if item.status == "dispatching" {
                active = true;
                break;
            }
            if let Some(id) = item.robot_run_id {
                let status:Option<String> = tx.query_row("SELECT json_extract(payload,'$.status') FROM channel_robot_runs WHERE id=?1",[id],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
                if status
                    .as_deref()
                    .is_some_and(|v| ["queued", "running", "awaiting_approval"].contains(&v))
                {
                    active = true;
                    break;
                }
            }
            if let Some(id) = item.run_id {
                let status: Option<String> = tx
                    .query_row(
                        "SELECT status FROM workflow_runs WHERE run_id=?1",
                        [id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?;
                if status.as_deref().is_some_and(|status| {
                    ["created", "running", "waiting_approval"].contains(&status)
                }) {
                    active = true;
                    break;
                }
            }
        }
        drop(statement);
        if active && occurrence.status == "dispatching" {
            occurrence.status = "skipped".into();
            occurrence.run_id = None;
            occurrence.error = Some("上一次执行尚未结束，本次已跳过".into());
        }
        let inserted = tx.execute("INSERT INTO automation_occurrences(id,automation_id,occurrence_key,payload,created_at) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(automation_id,occurrence_key) DO NOTHING", params![occurrence.id, occurrence.automation_id, occurrence.occurrence_key, serde_json::to_string(occurrence).map_err(|e| e.to_string())?, occurrence.created_at]).map_err(|e| e.to_string())?;
        if inserted == 0 {
            return Ok(false);
        }
        if !manual {
            current.next_run_at = next;
            if next.is_none() {
                current.enabled = false;
            }
            current.updated_at = occurrence.created_at.max(current.updated_at + 1);
            tx.execute(
                "UPDATE automations SET payload=?2 WHERE id=?1",
                params![
                    current.id,
                    serde_json::to_string(&current).map_err(|e| e.to_string())?
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(true)
    }
    pub fn update_automation_occurrence(&self, value: &Occurrence) -> Result<(), String> {
        self.conn()
            .execute(
                "UPDATE automation_occurrences SET payload=?2 WHERE id=?1",
                params![
                    value.id,
                    serde_json::to_string(value).map_err(|e| e.to_string())?
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn automation_history(&self, id: &str) -> Result<Vec<Occurrence>, String> {
        let conn = self.conn();
        let mut statement = conn.prepare("SELECT payload FROM automation_occurrences WHERE automation_id=?1 ORDER BY created_at DESC LIMIT 100").map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([id], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|row| {
            serde_json::from_str(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect()
    }
    pub fn automation_occurrences_for_recovery(&self) -> Result<Vec<Occurrence>, String> {
        let conn = self.conn();
        let mut statement = conn
            .prepare("SELECT payload FROM automation_occurrences")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|row| {
            serde_json::from_str(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::automation::model::Schedule;
    fn automation() -> Automation {
        Automation {
            id: "schedule".into(),
            revision: 1,
            name: "daily".into(),
            workflow_graph_id: "graph".into(),
            target: Some(
                crate::modules::automation::model::AutomationTarget::Workflow {
                    graph_id: "graph".into(),
                },
            ),
            workspace_id: "workspace".into(),
            timezone: "Asia/Shanghai".into(),
            schedule: Schedule::Daily {
                time: "09:00".into(),
            },
            enabled: true,
            next_run_at: Some(1000),
            created_at: 1,
            updated_at: 1,
        }
    }
    fn item(id: &str, due: i64) -> Occurrence {
        Occurrence {
            id: id.into(),
            automation_id: "schedule".into(),
            automation_revision: 1,
            timezone: "Asia/Shanghai".into(),
            occurrence_key: format!("scheduled:{due}"),
            trigger: "scheduled".into(),
            scheduled_at: due,
            workspace_id: "workspace".into(),
            workflow_graph_id: "graph".into(),
            run_id: Some(id.into()),
            robot_run_id: None,
            robot_channel_id: None,
            conversation_id: None,
            status: "dispatching".into(),
            error: None,
            created_at: due,
            updated_at: due,
        }
    }
    #[test]
    fn claims_once_and_skips_overlapping_dispatches() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let first = automation();
        db.save_automation(&first, true).unwrap();
        assert!(db
            .claim_automation_occurrence(&first, Some(2000), &mut item("first", 1000), false)
            .unwrap());
        assert!(!db
            .claim_automation_occurrence(&first, Some(2000), &mut item("duplicate", 1000), false)
            .unwrap());
        let current = db.get_automation("schedule").unwrap().unwrap();
        let mut second = item("second", 2000);
        assert!(db
            .claim_automation_occurrence(&current, Some(3000), &mut second, false)
            .unwrap());
        assert_eq!(second.status, "skipped");
        assert!(second.run_id.is_none());
        assert_eq!(
            db.get_automation("schedule").unwrap().unwrap().next_run_at,
            Some(3000)
        );
    }
    #[test]
    fn revision_and_occurrence_survive_database_reopen() {
        let path =
            std::env::temp_dir().join(format!("automation-revision-{}.db", uuid::Uuid::new_v4()));
        {
            let db = Database::new(&path).unwrap();
            let mut value = automation();
            db.save_automation(&value, true).unwrap();
            db.claim_automation_occurrence(&value, Some(2000), &mut item("first", 1000), false)
                .unwrap();
            value.revision = 2;
            value.name = "updated".into();
            value.next_run_at = Some(5000);
            db.save_automation(&value, false).unwrap();
            assert!(db.save_automation(&value, false).is_err());
        }
        {
            let db = Database::new(&path).unwrap();
            let value = db.get_automation("schedule").unwrap().unwrap();
            assert_eq!(value.revision, 2);
            assert_eq!(value.name, "updated");
            assert_eq!(value.next_run_at, Some(5000));
            let history = db.automation_history("schedule").unwrap();
            assert_eq!(history[0].automation_revision, 1);
            assert_eq!(history[0].timezone, "Asia/Shanghai");
        }
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn persisted_workflow_approval_blocks_overlap_until_actual_completion() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let first = automation();
        db.save_automation(&first, true).unwrap();
        let mut running = item("first", 1000);
        db.claim_automation_occurrence(&first, Some(2000), &mut running, false)
            .unwrap();
        running.status = "created".into();
        db.update_automation_occurrence(&running).unwrap();
        db.conn().execute("INSERT INTO workflow_runs(run_id,workflow_graph_id,execution_id,subject_id,agent_name,status,definition_snapshot_json,state_json,created_at,updated_at) VALUES('first','graph','execution','user','test','waiting_approval','{}','[]',1,1)", []).unwrap();
        let current = db.get_automation("schedule").unwrap().unwrap();
        let mut next = item("second", 2000);
        db.claim_automation_occurrence(&current, Some(3000), &mut next, false)
            .unwrap();
        assert_eq!(next.status, "skipped");
        db.conn()
            .execute(
                "UPDATE workflow_runs SET status='completed' WHERE run_id='first'",
                [],
            )
            .unwrap();
        let current = db.get_automation("schedule").unwrap().unwrap();
        let mut last = item("third", 3000);
        db.claim_automation_occurrence(&current, Some(4000), &mut last, false)
            .unwrap();
        assert_eq!(last.status, "dispatching");
    }
}
