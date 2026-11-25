use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use crate::modules::automation::{
    attention::{AttentionState, AttentionStatus, FactReference, SourceObservation},
    model::{Automation, Occurrence, Schedule},
};

use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};

const OBSERVATIONS: ProductMigrationSpec = ProductMigrationSpec::new(
    1098,
    "1098_automation_observations",
    MigrationOwner::V1TaskWorld,
    "CREATE TABLE automation_observations(
         id TEXT PRIMARY KEY,
         automation_id TEXT NOT NULL,
         observed_at INTEGER NOT NULL,
         payload TEXT NOT NULL
     );
     CREATE INDEX automation_observation_log
       ON automation_observations(automation_id, observed_at DESC);",
);

impl Database {
    pub(crate) fn attention_observation(
        &self,
        automation: &str,
        observation: &str,
    ) -> Result<Option<SourceObservation>, String> {
        let payload = self
            .conn()
            .query_row(
                "SELECT payload FROM automation_observations WHERE automation_id=?1 AND id=?2",
                params![automation, observation],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(error)?;
        payload
            .map(|value| serde_json::from_str(&value).map_err(error))
            .transpose()
    }

    pub(super) fn initialize_automation_observations(&self) -> rusqlite::Result<()> {
        self.apply_product_migration(&OBSERVATIONS)
    }

    pub(crate) fn claim_attention_check(
        &self,
        expected: &Automation,
        now: i64,
    ) -> Result<Option<Automation>, String> {
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(error)?;
        let Some(mut current) = load(&tx, &expected.id)? else {
            return Ok(None);
        };
        if current.revision != expected.revision
            || !current.enabled
            || current.archived
            || current.next_run_at != expected.next_run_at
            || current.next_run_at.is_none_or(|due| due > now)
        {
            return Ok(None);
        }
        let Schedule::Watch {
            expires_at,
            max_checks,
            ..
        } = current.schedule
        else {
            return Ok(None);
        };
        let state = current
            .attention
            .get_or_insert_with(AttentionState::waiting);
        current.next_run_at = None;
        if now >= expires_at || state.checks_used >= max_checks {
            state.stop("关注期限或检查预算已用尽");
            current.enabled = false;
            write(&tx, &current)?;
            tx.commit().map_err(error)?;
            return Ok(None);
        }
        state.status = AttentionStatus::Checking;
        state.reason = "正在采集并判断来源".into();
        state.pending_check = Some(uuid::Uuid::new_v4().to_string());
        state.checks_used += 1;
        write(&tx, &current)?;
        tx.commit().map_err(error)?;
        Ok(Some(current))
    }

    /// Commit observations, the next check and the queued action together.
    /// The pending identifier and configuration revision belong to this sample.
    pub(crate) fn settle_attention_check(
        &self,
        expected: &Automation,
        mut state: AttentionState,
        mut next: Option<i64>,
        observation: Option<&SourceObservation>,
        mut occurrence: Option<&Occurrence>,
    ) -> Result<Option<bool>, String> {
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(error)?;
        let Some(mut current) = load(&tx, &expected.id)? else {
            return Ok(None);
        };
        if !current.enabled
            || current.archived
            || current.revision != expected.revision
            || current
                .attention
                .as_ref()
                .and_then(|state| state.pending_check.as_deref())
                != expected
                    .attention
                    .as_ref()
                    .and_then(|state| state.pending_check.as_deref())
        {
            return Ok(None);
        }
        if matches!(current.schedule, Schedule::Watch { expires_at, .. } if chrono::Utc::now().timestamp_millis() >= expires_at)
        {
            let prior = current.attention.as_ref();
            state.decision = prior.and_then(|state| state.decision);
            state.proposal = prior.and_then(|state| state.proposal.clone());
            state.stop("关注已到截止时间");
            next = None;
            occurrence = None;
        }
        let mut queued = false;
        if let Some(item) = occurrence {
            queued = tx.execute(
                "INSERT INTO automation_occurrences(id,automation_id,occurrence_key,payload,created_at)
                 SELECT ?1,?2,?3,?4,?5
                 WHERE NOT EXISTS (
                     SELECT 1 FROM automation_occurrences
                     WHERE automation_id=?2
                       AND json_extract(payload,'$.status') IN
                           ('dispatching','running','waiting_approval','waiting_input')
                 )
                 ON CONFLICT(automation_id,occurrence_key) DO NOTHING",
                params![
                    item.id, item.automation_id, item.occurrence_key,
                    serde_json::to_string(item).map_err(error)?, item.created_at
                ],
            ).map_err(error)? == 1;
            if !queued {
                state.status = AttentionStatus::Deferred;
                state.reason = "委托已有执行或等待事项，本次未启动新一轮".into();
            } else {
                state.facts.push(FactReference {
                    kind: "automation_occurrence".into(),
                    id: item.id.clone(),
                });
            }
        }
        if let Some(observation) = observation {
            insert_source_observation_on(&tx, observation)?;
        }
        state.pending_check = None;
        current.next_run_at = next;
        current.enabled = next.is_some();
        current.attention = Some(state);
        write(&tx, &current)?;
        tx.commit().map_err(error)?;
        self.automation_schedule_changed().notify_one();
        Ok(Some(queued))
    }

    pub(crate) fn recover_attention_checks(&self, now: i64) -> Result<(), String> {
        for mut automation in self.list_automations()? {
            let Schedule::Watch {
                expires_at,
                max_checks,
                ..
            } = automation.schedule
            else {
                continue;
            };
            let Some(state) = automation.attention.as_mut() else {
                continue;
            };
            if state.pending_check.take().is_none() || !automation.enabled || automation.archived {
                continue;
            }
            if now < expires_at && state.checks_used < max_checks {
                state.status = AttentionStatus::Unknown;
                state.reason = "上次采集随应用退出中断，等待下一次只读复查".into();
                automation.next_run_at = Some(now);
            } else {
                state.stop("关注期限或检查预算已用尽");
                automation.next_run_at = None;
                automation.enabled = false;
            }
            automation.revision += 1;
            self.save_automation(&automation, false)?;
        }
        Ok(())
    }

    pub(crate) fn list_automation_observations(
        &self,
        automation: &str,
    ) -> Result<Vec<SourceObservation>, String> {
        let conn = self.conn();
        let mut statement = conn
            .prepare(
                "SELECT payload FROM automation_observations WHERE automation_id=?1
             ORDER BY observed_at DESC, rowid DESC LIMIT 100",
            )
            .map_err(error)?;
        let rows = statement
            .query_map([automation], |row| row.get::<_, String>(0))
            .map_err(error)?;
        rows.map(|row| serde_json::from_str(&row.map_err(error)?).map_err(error))
            .collect()
    }
}

pub(super) fn insert_source_observation_on(
    conn: &Connection,
    observation: &SourceObservation,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO automation_observations(id,automation_id,observed_at,payload) VALUES(?1,?2,?3,?4)",
        params![
            observation.id, observation.automation_id, observation.observed_at,
            serde_json::to_string(observation).map_err(error)?
        ],
    ).map_err(error)?;
    Ok(())
}

fn load(conn: &Connection, id: &str) -> Result<Option<Automation>, String> {
    let payload = conn
        .query_row("SELECT payload FROM automations WHERE id=?1", [id], |r| {
            r.get::<_, String>(0)
        })
        .optional()
        .map_err(error)?;
    payload
        .map(|value| serde_json::from_str(&value).map_err(error))
        .transpose()
}

fn write(conn: &Connection, automation: &Automation) -> Result<(), String> {
    conn.execute(
        "UPDATE automations SET payload=?2 WHERE id=?1",
        params![
            automation.id,
            serde_json::to_string(automation).map_err(error)?
        ],
    )
    .map_err(error)?;
    Ok(())
}

fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
