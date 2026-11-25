//! Durable handoff receipts and a bounded background acceptance queue.
//! These are rebuildable integration records, never another task state machine.
use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    ConversationRunStatus, Database,
};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

const SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    1080,
    "1080_execution_handoffs",
    MigrationOwner::V1TaskWorld,
    "CREATE TABLE execution_outcomes (
       sequence INTEGER PRIMARY KEY AUTOINCREMENT,
       conversation_id TEXT NOT NULL, turn_key TEXT NOT NULL, user_message_id TEXT,
       status TEXT NOT NULL, error TEXT, created_at INTEGER NOT NULL,
       projected INTEGER NOT NULL DEFAULT 0, attempts INTEGER NOT NULL DEFAULT 0, last_error TEXT,
       UNIQUE(conversation_id,turn_key,status));
     CREATE INDEX execution_outcomes_pending ON execution_outcomes(projected,attempts,sequence);
     CREATE TABLE goal_evaluation_queue (
       graph_id TEXT PRIMARY KEY, generation INTEGER NOT NULL, dirty INTEGER NOT NULL,
       updated_at INTEGER NOT NULL, attempts INTEGER NOT NULL DEFAULT 0, last_error TEXT);",
);

const FAIRNESS: ProductMigrationSpec = ProductMigrationSpec::new(1081,"1081_execution_handoff_fairness",MigrationOwner::V1TaskWorld,
    "ALTER TABLE execution_outcomes ADD COLUMN last_checked_at INTEGER NOT NULL DEFAULT 0;
     CREATE INDEX execution_outcomes_fairness ON execution_outcomes(projected,attempts,last_checked_at,sequence);");

const SEGMENTS: ProductMigrationSpec = ProductMigrationSpec::new(1082,"1082_execution_handoff_segments",MigrationOwner::V1TaskWorld,
    "CREATE TABLE execution_outcomes_segments (
       sequence INTEGER PRIMARY KEY AUTOINCREMENT,conversation_id TEXT NOT NULL,turn_key TEXT NOT NULL,user_message_id TEXT,
       status TEXT NOT NULL,error TEXT,created_at INTEGER NOT NULL,projected INTEGER NOT NULL DEFAULT 0,
       attempts INTEGER NOT NULL DEFAULT 0,last_error TEXT,last_checked_at INTEGER NOT NULL DEFAULT 0,
       segment_id TEXT NOT NULL,UNIQUE(conversation_id,turn_key,segment_id));
     INSERT INTO execution_outcomes_segments(sequence,conversation_id,turn_key,user_message_id,status,error,created_at,projected,attempts,last_error,last_checked_at,segment_id)
       SELECT sequence,conversation_id,turn_key,user_message_id,status,error,created_at,projected,attempts,last_error,last_checked_at,'legacy:'||status FROM execution_outcomes;
     DROP TABLE execution_outcomes;
     ALTER TABLE execution_outcomes_segments RENAME TO execution_outcomes;
     CREATE INDEX execution_outcomes_pending ON execution_outcomes(projected,attempts,sequence);
     CREATE INDEX execution_outcomes_fairness ON execution_outcomes(projected,attempts,last_checked_at,sequence);");

#[derive(Clone, Debug, Serialize)]
pub struct ConversationOutcome {
    pub sequence: i64,
    pub conversation_id: String,
    pub turn_key: String,
    pub user_message_id: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub created_at: i64,
    pub segment_id: String,
}

#[derive(Clone, Debug)]
pub struct GoalEvaluationWork {
    pub graph_id: String,
    pub generation: i64,
}

impl Database {
    pub fn initialize_execution_handoffs(&self) -> Result<(), rusqlite::Error> {
        self.apply_product_migration(&SCHEMA)?;
        self.apply_product_migration(&FAIRNESS)?;
        self.apply_product_migration(&SEGMENTS)
    }

    /// A receipt is written before any local outcome projection. Same-turn,
    /// same-status retries return the original receipt and do not execute work.
    pub fn record_conversation_outcome(
        &self,
        conversation: &str,
        status: ConversationRunStatus,
        error: Option<&str>,
    ) -> Result<ConversationOutcome, String> {
        self.record_conversation_segment_outcome(
            conversation,
            &format!("legacy:{}", status.as_str()),
            status,
            error,
        )
    }

    pub fn record_conversation_segment_outcome(
        &self,
        conversation: &str,
        segment: &str,
        status: ConversationRunStatus,
        error: Option<&str>,
    ) -> Result<ConversationOutcome, String> {
        if segment.is_empty() || segment.len() > 160 || segment.chars().any(char::is_control) {
            return Err("invalid outcome segment identity".into());
        }
        self.initialize_execution_handoffs()
            .map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().timestamp_millis();
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let turn: Option<(String,String)> = tx.query_row("SELECT id,user_message_id FROM conversation_turns WHERE conversation_id=?1 ORDER BY started_at DESC,rowid DESC LIMIT 1",[conversation],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
        let (key, user) = if let Some((key, user)) = turn {
            (key, Some(user))
        } else {
            let started: Option<i64> = tx
                .query_row(
                    "SELECT run_started_at FROM conversations WHERE id=?1",
                    [conversation],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            (format!("legacy:{}", started.unwrap_or(0)), None)
        };
        tx.execute("INSERT INTO execution_outcomes(conversation_id,turn_key,user_message_id,status,error,created_at,segment_id) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(conversation_id,turn_key,segment_id) DO NOTHING",params![conversation,key,user,status.as_str(),error,now,segment]).map_err(|e|e.to_string())?;
        let receipt = tx.query_row("SELECT sequence,conversation_id,turn_key,user_message_id,status,error,created_at,segment_id FROM execution_outcomes WHERE conversation_id=?1 AND turn_key=?2 AND segment_id=?3",params![conversation,key,segment],map_outcome).map_err(|e|e.to_string())?;
        if receipt.status != status.as_str() || receipt.error.as_deref() != error {
            return Err("outcome segment replay changed its payload".into());
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(receipt)
    }

    pub fn pending_conversation_outcomes(
        &self,
        limit: usize,
    ) -> Result<Vec<ConversationOutcome>, String> {
        let conn = self.conn();
        let mut stmt=conn.prepare("SELECT sequence,conversation_id,turn_key,user_message_id,status,error,created_at,segment_id FROM execution_outcomes WHERE projected=0 AND attempts<5 ORDER BY last_checked_at,sequence LIMIT ?1").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map([limit.clamp(1, 100) as i64], map_outcome)
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn defer_conversation_outcome(&self, sequence: i64) -> Result<(), String> {
        self.conn()
            .execute(
                "UPDATE execution_outcomes SET last_checked_at=?2 WHERE sequence=?1",
                params![sequence, chrono::Utc::now().timestamp_millis()],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Current turn, cancellation and projection commit are checked in one
    /// transaction, including conversations without a ProofLoop binding.
    pub fn commit_conversation_outcome_guarded(
        &self,
        conversation: &str,
        requested: ConversationRunStatus,
        error: Option<&str>,
        expected_turn: Option<&str>,
    ) -> Result<ConversationRunStatus, String> {
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let (raw, current_error, started): (String, Option<String>, Option<i64>) = tx
            .query_row(
                "SELECT run_status,run_error,run_started_at FROM conversations WHERE id=?1",
                [conversation],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(|e| e.to_string())?;
        let current: ConversationRunStatus = serde_json::from_value(serde_json::Value::String(raw))
            .map_err(|_| "invalid persisted conversation status".to_string())?;
        let turn:Option<String>=tx.query_row("SELECT id FROM conversation_turns WHERE conversation_id=?1 ORDER BY started_at DESC,rowid DESC LIMIT 1",[conversation],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
        let current_key = turn
            .clone()
            .unwrap_or_else(|| format!("legacy:{}", started.unwrap_or(0)));
        if expected_turn.is_some_and(|expected| expected != current_key)
            || current == ConversationRunStatus::Cancelled
        {
            return Ok(current);
        }
        if current == requested && current_error.as_deref() == error {
            return Ok(current);
        }
        let now = chrono::Utc::now().timestamp_millis();
        let terminal = matches!(
            requested,
            ConversationRunStatus::Completed
                | ConversationRunStatus::Failed
                | ConversationRunStatus::Cancelled
                | ConversationRunStatus::Interrupted
        );
        tx.execute("UPDATE conversations SET run_status=?2,run_error=?3,run_finished_at=CASE WHEN ?5 THEN ?4 ELSE NULL END,updated_at=?4 WHERE id=?1",params![conversation,requested.as_str(),error,now,terminal]).map_err(|e|e.to_string())?;
        if let Some(turn) = turn {
            tx.execute("UPDATE conversation_turns SET status=?2,finished_at=?3 WHERE id=?1 AND finished_at IS NULL",params![turn,requested.as_str(),if terminal{Some(now)}else{None}]).map_err(|e|e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        drop(conn);
        self.publish_goal_conversation_change(
            conversation,
            "conversation_status",
            Some(requested.as_str()),
        );
        Ok(requested)
    }

    pub fn pending_conversation_outcomes_after(
        &self,
        after: i64,
        limit: usize,
    ) -> Result<Vec<ConversationOutcome>, String> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT sequence,conversation_id,turn_key,user_message_id,status,error,created_at,segment_id FROM execution_outcomes WHERE projected=0 AND attempts<5 AND sequence>?1 ORDER BY sequence LIMIT ?2").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map(params![after, limit.clamp(1, 100) as i64], map_outcome)
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    /// Late outcomes are never applied to a new accepted user turn. A later
    /// receipt for the same turn also supersedes an older waiting projection.
    pub fn conversation_outcome_is_current(
        &self,
        outcome: &ConversationOutcome,
    ) -> Result<bool, String> {
        let conn = self.conn();
        let current:Option<String>=conn.query_row("SELECT id FROM conversation_turns WHERE conversation_id=?1 ORDER BY started_at DESC,rowid DESC LIMIT 1",[&outcome.conversation_id],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
        let same = match current {
            Some(id) => id == outcome.turn_key,
            None => {
                let started: Option<Option<i64>> = conn
                    .query_row(
                        "SELECT run_started_at FROM conversations WHERE id=?1",
                        [&outcome.conversation_id],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?;
                started.is_some_and(|started| {
                    outcome.turn_key == format!("legacy:{}", started.unwrap_or(0))
                })
            }
        };
        let newer:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM execution_outcomes WHERE conversation_id=?1 AND turn_key=?2 AND sequence>?3)",params![outcome.conversation_id,outcome.turn_key,outcome.sequence],|r|r.get(0)).map_err(|e|e.to_string())?;
        Ok(same && !newer)
    }

    pub fn settle_conversation_outcome(
        &self,
        sequence: i64,
        error: Option<&str>,
    ) -> Result<(), String> {
        self.conn().execute("UPDATE execution_outcomes SET projected=CASE WHEN ?2 IS NULL THEN 1 ELSE 0 END,attempts=attempts+CASE WHEN ?2 IS NULL THEN 0 ELSE 1 END,last_error=?2 WHERE sequence=?1",params![sequence,error]).map_err(|e|e.to_string())?;
        Ok(())
    }

    /// This hook is independent of EventHub subscribers. Generation matching
    /// makes an artifact/attempt arriving during evaluation remain dirty.
    pub fn queue_goal_evaluation(&self, graph: &str) -> Result<(), String> {
        self.conn().execute("INSERT INTO goal_evaluation_queue(graph_id,generation,dirty,updated_at) VALUES(?1,1,1,?2) ON CONFLICT(graph_id) DO UPDATE SET generation=generation+1,dirty=1,updated_at=excluded.updated_at,attempts=0,last_error=NULL",params![graph,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        Ok(())
    }

    pub fn recover_integration_handoffs(&self) -> Result<(), String> {
        self.initialize_execution_handoffs()
            .map_err(|e| e.to_string())?;
        self.initialize_proofloop_schema()
            .map_err(|e| e.to_string())?;
        self.conn()
            .execute(
                "UPDATE execution_outcomes SET attempts=0 WHERE projected=0",
                [],
            )
            .map_err(|e| e.to_string())?;
        self.conn().execute("INSERT INTO goal_evaluation_queue(graph_id,generation,dirty,updated_at) SELECT graph_id,1,1,?1 FROM proofloop_drafts WHERE status='confirmed' GROUP BY graph_id ON CONFLICT(graph_id) DO UPDATE SET generation=generation+1,dirty=1,attempts=0,last_error=NULL,updated_at=excluded.updated_at",[chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        Ok(())
    }

    pub fn pending_goal_evaluations(&self) -> Result<Vec<GoalEvaluationWork>, String> {
        let conn = self.conn();
        let mut stmt=conn.prepare("SELECT graph_id,generation FROM goal_evaluation_queue WHERE dirty=1 AND attempts<5 ORDER BY updated_at,graph_id LIMIT 32").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(GoalEvaluationWork {
                    graph_id: r.get(0)?,
                    generation: r.get(1)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn settle_goal_evaluation(
        &self,
        work: &GoalEvaluationWork,
        error: Option<&str>,
    ) -> Result<(), String> {
        self.conn().execute("UPDATE goal_evaluation_queue SET dirty=CASE WHEN ?3 IS NULL THEN 0 ELSE 1 END,attempts=attempts+CASE WHEN ?3 IS NULL THEN 0 ELSE 1 END,last_error=?3 WHERE graph_id=?1 AND generation=?2",params![work.graph_id,work.generation,error]).map_err(|e|e.to_string())?;
        Ok(())
    }

    pub fn defer_goal_evaluation(&self, work: &GoalEvaluationWork) -> Result<(), String> {
        self.conn().execute("UPDATE goal_evaluation_queue SET updated_at=?3 WHERE graph_id=?1 AND generation=?2",params![work.graph_id,work.generation,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        Ok(())
    }

    pub fn integration_projection_issues(&self) -> Result<Vec<serde_json::Value>, String> {
        let conn = self.conn();
        let mut stmt=conn.prepare("SELECT 'conversation' AS kind,conversation_id AS id,last_error,attempts FROM execution_outcomes WHERE projected=0 AND last_error IS NOT NULL UNION ALL SELECT 'task_graph',graph_id,last_error,attempts FROM goal_evaluation_queue WHERE dirty=1 AND last_error IS NOT NULL LIMIT 50").map_err(|e|e.to_string())?;
        let rows=stmt.query_map([],|r|Ok(serde_json::json!({"kind":r.get::<_,String>(0)?,"id":r.get::<_,String>(1)?,"error":r.get::<_,String>(2)?,"attempts":r.get::<_,i64>(3)?}))).map_err(|e|e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
}

fn map_outcome(r: &rusqlite::Row<'_>) -> rusqlite::Result<ConversationOutcome> {
    Ok(ConversationOutcome {
        sequence: r.get(0)?,
        conversation_id: r.get(1)?,
        turn_key: r.get(2)?,
        user_message_id: r.get(3)?,
        status: r.get(4)?,
        error: r.get(5)?,
        created_at: r.get(6)?,
        segment_id: r.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Database {
        Database::new(&std::env::temp_dir().join(format!("handoff-db-{}.db", uuid::Uuid::new_v4())))
            .unwrap()
    }
    #[test]
    fn integration_segment_receipts_distinguish_repeated_waits_and_replay_exactly() {
        let db = db();
        let c = db.create_conversation("segment fixture").unwrap();
        let first = db
            .record_conversation_segment_outcome(
                &c.id,
                "segment-one",
                ConversationRunStatus::WaitingApproval,
                None,
            )
            .unwrap();
        db.settle_conversation_outcome(first.sequence, None)
            .unwrap();
        let second = db
            .record_conversation_segment_outcome(
                &c.id,
                "segment-two",
                ConversationRunStatus::WaitingApproval,
                None,
            )
            .unwrap();
        assert_ne!(first.sequence, second.sequence);
        assert_eq!(db.pending_conversation_outcomes(10).unwrap().len(), 1);
        let replay = db
            .record_conversation_segment_outcome(
                &c.id,
                "segment-two",
                ConversationRunStatus::WaitingApproval,
                None,
            )
            .unwrap();
        assert_eq!(second.sequence, replay.sequence);
        assert!(db
            .record_conversation_segment_outcome(
                &c.id,
                "segment-two",
                ConversationRunStatus::Completed,
                None
            )
            .is_err());
        assert!(!db.conversation_outcome_is_current(&first).unwrap());
        assert!(db.conversation_outcome_is_current(&second).unwrap());
    }

    #[test]
    fn integration_segment_migration_preserves_receipts_and_queue_and_is_repeatable() {
        let db = db();
        // Only this freshly-created isolated fixture is rolled back to the
        // previous schema so the actual additive migration can be exercised.
        db.conn().execute_batch("DROP TABLE execution_outcomes; DROP TABLE goal_evaluation_queue; DELETE FROM schema_migrations WHERE version IN (1080,1081,1082);").unwrap();
        db.apply_product_migration(&SCHEMA).unwrap();
        db.apply_product_migration(&FAIRNESS).unwrap();
        db.conn().execute("INSERT INTO execution_outcomes(sequence,conversation_id,turn_key,status,error,created_at,projected,attempts,last_error,last_checked_at) VALUES(77,'fixture','old-turn','completed','source error',10,0,2,'projection error',12)",[]).unwrap();
        db.queue_goal_evaluation("retained-graph").unwrap();
        db.initialize_execution_handoffs().unwrap();
        db.initialize_execution_handoffs().unwrap();
        let row = db.pending_conversation_outcomes(10).unwrap().remove(0);
        assert_eq!(row.sequence, 77);
        assert_eq!(row.segment_id, "legacy:completed");
        assert_eq!(row.error.as_deref(), Some("source error"));
        assert_eq!(
            db.pending_goal_evaluations().unwrap()[0].graph_id,
            "retained-graph"
        );
        let attempts: i64 = db
            .conn()
            .query_row(
                "SELECT attempts FROM execution_outcomes WHERE sequence=77",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(attempts, 2);
    }
    #[test]
    fn integration_outcome_receipt_is_idempotent_and_retry_bounded() {
        let db = db();
        db.initialize_execution_handoffs().unwrap();
        let c = db.create_conversation("test").unwrap();
        let a = db
            .record_conversation_outcome(&c.id, ConversationRunStatus::Completed, None)
            .unwrap();
        let b = db
            .record_conversation_outcome(&c.id, ConversationRunStatus::Completed, None)
            .unwrap();
        assert_eq!(a.sequence, b.sequence);
        for _ in 0..5 {
            db.settle_conversation_outcome(a.sequence, Some("injected projection failure"))
                .unwrap();
        }
        assert!(db.pending_conversation_outcomes(10).unwrap().is_empty());
        assert_eq!(db.integration_projection_issues().unwrap().len(), 1);
        db.recover_integration_handoffs().unwrap();
        assert_eq!(db.pending_conversation_outcomes(10).unwrap().len(), 1);
    }
    #[test]
    fn integration_queue_keeps_change_arriving_during_evaluation() {
        let db = db();
        db.initialize_execution_handoffs().unwrap();
        db.queue_goal_evaluation("graph").unwrap();
        let old = db.pending_goal_evaluations().unwrap().remove(0);
        db.queue_goal_evaluation("graph").unwrap();
        db.settle_goal_evaluation(&old, None).unwrap();
        let current = db.pending_goal_evaluations().unwrap().remove(0);
        assert!(current.generation > old.generation);
        db.settle_goal_evaluation(&current, None).unwrap();
        assert!(db.pending_goal_evaluations().unwrap().is_empty());
    }
    #[test]
    fn integration_new_outcome_supersedes_waiting_receipt() {
        let db = db();
        db.initialize_execution_handoffs().unwrap();
        let c = db.create_conversation("test").unwrap();
        let wait = db
            .record_conversation_outcome(&c.id, ConversationRunStatus::WaitingApproval, None)
            .unwrap();
        let done = db
            .record_conversation_outcome(&c.id, ConversationRunStatus::Completed, None)
            .unwrap();
        assert!(!db.conversation_outcome_is_current(&wait).unwrap());
        assert!(db.conversation_outcome_is_current(&done).unwrap());
    }
    #[test]
    fn integration_old_legacy_receipt_cannot_replace_a_new_run() {
        let db = db();
        db.initialize_execution_handoffs().unwrap();
        let c = db.create_conversation("test").unwrap();
        db.conn()
            .execute(
                "UPDATE conversations SET run_started_at=10 WHERE id=?1",
                [&c.id],
            )
            .unwrap();
        let old = db
            .record_conversation_outcome(&c.id, ConversationRunStatus::Completed, None)
            .unwrap();
        db.conn()
            .execute(
                "UPDATE conversations SET run_started_at=11 WHERE id=?1",
                [&c.id],
            )
            .unwrap();
        assert!(!db.conversation_outcome_is_current(&old).unwrap());
    }
}
