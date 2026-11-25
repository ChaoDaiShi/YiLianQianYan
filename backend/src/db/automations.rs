use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use crate::modules::automation::{
    attention::SourceObservation,
    model::{Automation, Occurrence},
};
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

// UNION (without ALL) terminates even if corrupt branch metadata has a cycle.
// Retain restrictions on identifiable branches created by older app versions.
pub(super) const PERSONAL_OWNER_QUERY: &str = "WITH RECURSIVE ancestry(id) AS (
    SELECT ?1 UNION
    SELECT m.conversation_id FROM ancestry a JOIN message_canvas_branches b ON b.conversation_id=a.id JOIN messages m ON m.id=b.parent_node_id
    UNION SELECT r.root_conversation_id FROM ancestry a JOIN connector_message_runs r ON r.conversation_id=a.id WHERE r.conversation_id<>r.root_conversation_id
) SELECT s.value FROM ancestry a JOIN settings s ON s.key='personal_assistant:'||a.id LIMIT 1";

impl Database {
    pub fn personal_delegation_owner(&self, conversation: &str) -> Result<Option<String>, String> {
        self.conn()
            .query_row(PERSONAL_OWNER_QUERY, [conversation], |row| row.get(0))
            .optional()
            .map_err(|e| e.to_string())
    }
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
            if matches!(
                current.target,
                Some(crate::modules::automation::model::AutomationTarget::PersonalAssistant { .. })
            ) || matches!(
                value.target,
                Some(crate::modules::automation::model::AutomationTarget::PersonalAssistant { .. })
            ) {
                let mut before =
                    serde_json::to_value(&current.target).map_err(|e| e.to_string())?;
                let mut after = serde_json::to_value(&value.target).map_err(|e| e.to_string())?;
                let mut reduced = true;
                for key in ["allow_file_writes", "allow_file_discovery"] {
                    if after[key].as_bool().unwrap_or(false)
                        && !before[key].as_bool().unwrap_or(false)
                    {
                        reduced = false;
                    }
                    if let Some(map) = before.as_object_mut() {
                        map.remove(key);
                    }
                    if let Some(map) = after.as_object_mut() {
                        map.remove(key);
                    }
                }
                let same_scope = reduced
                    && before == after
                    && value.name == current.name
                    && value.workspace_id == current.workspace_id
                    && value.workflow_graph_id == current.workflow_graph_id
                    && value.timezone == current.timezone
                    && value.archived == current.archived
                    && serde_json::to_value(&value.schedule).map_err(|e| e.to_string())?
                        == serde_json::to_value(&current.schedule).map_err(|e| e.to_string())?;
                if !same_scope {
                    let active: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM automation_occurrences WHERE automation_id=?1 AND json_extract(payload,'$.status') IN ('dispatching','running','waiting_approval','waiting_input'))", [&value.id], |row|row.get(0)).map_err(|e|e.to_string())?;
                    if active {
                        return Err("委托已开始执行，请先停止当前轮次再修改配置".into());
                    }
                }
            }
        }
        let count = tx
            .execute(query, params![value.id, payload])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        if count == 0 {
            return Err("时间任务不存在".into());
        }
        self.automation_schedule_changed.notify_one();
        Ok(())
    }
    pub fn delete_automation(&self, id: &str) -> Result<bool, String> {
        let deleted = self
            .conn()
            .execute("DELETE FROM automations WHERE id=?1", [id])
            .map_err(|e| e.to_string())?
            > 0;
        if deleted {
            self.automation_schedule_changed.notify_one();
        }
        Ok(deleted)
    }
    pub(crate) fn automation_schedule_changed(&self) -> &tokio::sync::Notify {
        &self.automation_schedule_changed
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
        self.claim_automation_occurrence_inner(expected, next, occurrence, manual, None)
    }
    pub fn claim_automation_file_event(
        &self,
        expected: &Automation,
        fingerprint: &str,
        observation: &SourceObservation,
        occurrence: &mut Occurrence,
    ) -> Result<bool, String> {
        self.claim_automation_occurrence_inner(
            expected,
            None,
            occurrence,
            false,
            Some((fingerprint, observation)),
        )
    }
    fn claim_automation_occurrence_inner(
        &self,
        expected: &Automation,
        next: Option<i64>,
        occurrence: &mut Occurrence,
        manual: bool,
        file_event: Option<(&str, &SourceObservation)>,
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
        let fingerprint = file_event.map(|(fingerprint, _)| fingerprint);
        if current.archived
            || current.revision != expected.revision
            || (fingerprint.is_some()
                && (!current.enabled
                    || current.file_fingerprint != expected.file_fingerprint
                    || !matches!(
                        current.schedule,
                        crate::modules::automation::model::Schedule::FileChanged { .. }
                    )
                    || current.file_fingerprint.as_deref() == fingerprint))
            || (fingerprint.is_none()
                && !manual
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
            if item.conversation_id.is_some()
                && item.robot_run_id.is_none()
                && item.run_id.is_none()
                && ["running", "waiting_approval", "waiting_input"].contains(&item.status.as_str())
            {
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
        // Busy sources retain the old checkpoint: the next poll coalesces the
        // latest content instead of losing it or creating skipped history noise.
        if active && (fingerprint.is_some() || occurrence.request_fingerprint.is_some()) {
            return Ok(false);
        }
        if active && occurrence.status == "dispatching" {
            occurrence.status = "skipped".into();
            occurrence.run_id = None;
            occurrence.error = Some("上一次执行尚未结束，本次已跳过".into());
        }
        let inserted = tx.execute("INSERT INTO automation_occurrences(id,automation_id,occurrence_key,payload,created_at) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(automation_id,occurrence_key) DO NOTHING", params![occurrence.id, occurrence.automation_id, occurrence.occurrence_key, serde_json::to_string(occurrence).map_err(|e| e.to_string())?, occurrence.created_at]).map_err(|e| e.to_string())?;
        if inserted == 0 {
            return Ok(false);
        }
        if let Some((_, observation)) = file_event {
            super::automation_observations::insert_source_observation_on(&tx, observation)?;
        }
        if let Some(fingerprint) = fingerprint {
            current.file_fingerprint = Some(fingerprint.into());
            tx.execute(
                "UPDATE automations SET payload=?2 WHERE id=?1",
                params![
                    current.id,
                    serde_json::to_string(&current).map_err(|e| e.to_string())?
                ],
            )
            .map_err(|e| e.to_string())?;
        } else if !manual {
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

    pub(crate) fn automation_runtime_occurrence(
        &self,
        id: &str,
    ) -> Result<Option<Occurrence>, String> {
        let row: Option<(String, String)> = self.conn().query_row(
            "WITH runtime AS (
                SELECT o.payload, o.created_at, o.rowid AS order_id,
                       CASE WHEN json_extract(o.payload,'$.status') IN ('dispatching','interrupted')
                            THEN json_extract(o.payload,'$.status')
                            ELSE COALESCE(w.status, json_extract(r.payload,'$.status'), json_extract(o.payload,'$.status'))
                       END AS status
                FROM automation_occurrences o
                LEFT JOIN workflow_runs w ON w.run_id=json_extract(o.payload,'$.run_id')
                LEFT JOIN channel_robot_runs r ON r.id=json_extract(o.payload,'$.robot_run_id')
                WHERE o.automation_id=?1
            ) SELECT payload,status FROM runtime WHERE status!='skipped'
              ORDER BY CASE WHEN status IN ('dispatching','created','queued','running','waiting_approval','awaiting_approval','waiting_input') THEN 0 ELSE 1 END,
                       created_at DESC,order_id DESC LIMIT 1",
            [id], |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional().map_err(|e| e.to_string())?;
        row.map(|(payload, status)| {
            let mut item: Occurrence = serde_json::from_str(&payload).map_err(|e| e.to_string())?;
            item.status = status;
            Ok(item)
        })
        .transpose()
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
            file_fingerprint: None,
            attention: None,
            archived: false,
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
            assistant_turn_id: None,
            report: None,
            request_fingerprint: None,
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
        assert!(db
            .automation_runtime_occurrence("schedule")
            .unwrap()
            .is_none());
        let mut running = item("first", 1000);
        db.claim_automation_occurrence(&first, Some(2000), &mut running, false)
            .unwrap();
        assert_eq!(
            db.automation_runtime_occurrence("schedule")
                .unwrap()
                .unwrap()
                .status,
            "dispatching"
        );
        running.status = "created".into();
        db.update_automation_occurrence(&running).unwrap();
        db.conn().execute("INSERT INTO workflow_runs(run_id,workflow_graph_id,execution_id,subject_id,agent_name,status,definition_snapshot_json,state_json,created_at,updated_at) VALUES('first','graph','execution','user','test','waiting_approval','{}','[]',1,1)", []).unwrap();
        let current = db.get_automation("schedule").unwrap().unwrap();
        let mut next = item("second", 2000);
        db.claim_automation_occurrence(&current, Some(3000), &mut next, false)
            .unwrap();
        assert_eq!(next.status, "skipped");
        assert_eq!(
            db.automation_runtime_occurrence("schedule")
                .unwrap()
                .unwrap()
                .status,
            "waiting_approval"
        );
        db.conn()
            .execute(
                "UPDATE workflow_runs SET status='completed' WHERE run_id='first'",
                [],
            )
            .unwrap();
        assert_eq!(
            db.automation_runtime_occurrence("schedule")
                .unwrap()
                .unwrap()
                .status,
            "completed"
        );
        let current = db.get_automation("schedule").unwrap().unwrap();
        let mut last = item("third", 3000);
        db.claim_automation_occurrence(&current, Some(4000), &mut last, false)
            .unwrap();
        assert_eq!(last.status, "dispatching");
    }
}
