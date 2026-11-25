//! Per-profile settings for host-owned executor slots and background indexing.
use crate::{app::state::AppServer, db::Database};
use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

const KEY: &str = "resources.executor_policy.v1";
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Mode {
    #[default]
    Auto,
    Efficiency,
    Balanced,
    Performance,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Policy {
    revision: u64,
    mode: Mode,
    cli_limit: Option<usize>,
    background_interval_ms: Option<u64>,
    memory_reserve_percent: Option<u8>,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct Budget {
    pub cli_limit: usize,
    pub delegation_limit: usize,
    pub background_interval_ms: u64,
}

fn machine_mode() -> Mode {
    static MODE: OnceLock<Mode> = OnceLock::new();
    *MODE.get_or_init(|| {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let cores = std::thread::available_parallelism().map_or(1, usize::from);
        if cores <= 4 || sys.total_memory() < 10 * 1024_u64.pow(3) {
            Mode::Efficiency
        } else if cores >= 12 && sys.total_memory() >= 24 * 1024_u64.pow(3) {
            Mode::Performance
        } else {
            Mode::Balanced
        }
    })
}
impl Policy {
    fn budget(self) -> Budget {
        let mode = if self.mode == Mode::Auto {
            machine_mode()
        } else {
            self.mode
        };
        let (cli, delegations, interval) = match mode {
            Mode::Efficiency => (1, 1, 250),
            Mode::Performance => (2, 3, 10),
            _ => (1, 2, 80),
        };
        Budget {
            cli_limit: self.cli_limit.unwrap_or(cli),
            delegation_limit: delegations,
            background_interval_ms: self.background_interval_ms.unwrap_or(interval),
        }
    }
}
fn read(conn: &rusqlite::Connection) -> Result<Policy, String> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key=?1", [KEY], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|e| e.to_string())?;
    raw.map(|s| serde_json::from_str(&s))
        .transpose()
        .map(Option::unwrap_or_default)
        .map_err(|e| e.to_string())
}
pub(crate) fn startup_budget(db: &Database) -> Result<Budget, String> {
    Ok(read(&db.conn())?.budget())
}
pub(crate) fn background_delay(db: &Database, pressure: bool) -> Duration {
    let interval = read(&db.conn())
        .unwrap_or_default()
        .budget()
        .background_interval_ms;
    Duration::from_millis(if pressure {
        interval.max(750)
    } else {
        interval
    })
}

fn under_pressure(available_percent: u64, reserve: u8, previously_low: bool) -> bool {
    let threshold = u64::from(reserve) + if previously_low { 3 } else { 0 };
    available_percent < threshold
}

pub(crate) fn watch_pressure(server: &Arc<AppServer>) {
    let owner = Arc::downgrade(server);
    let cancel = server.cloud_revocation_retry_cancel.child_token();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(15));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut system = sysinfo::System::new();
        loop {
            tokio::select! { _ = cancel.cancelled() => break, _ = interval.tick() => {} }
            let Some(s) = owner.upgrade() else {
                break;
            };
            system.refresh_memory();
            if system.total_memory() == 0 {
                continue;
            }
            let available = system.available_memory().saturating_mul(100) / system.total_memory();
            let Ok(policy) = read(&s.db.conn()) else {
                continue;
            };
            let current = s.memory_pressure.load(std::sync::atomic::Ordering::Acquire);
            let next = under_pressure(
                available,
                policy.memory_reserve_percent.unwrap_or(10),
                current,
            );
            if current != next {
                s.memory_pressure
                    .store(next, std::sync::atomic::Ordering::Release);
                let _ = s.event_hub.publish(crate::shared::event::YiEvent::new(
                    "resource.pressure.changed",
                    "resource-monitor",
                    json!({"low_memory":next,"available_memory_percent":available}),
                ));
            }
        }
    });
}
type Failure = (StatusCode, Json<Value>);
fn failure(status: StatusCode, message: &str) -> Failure {
    (status, Json(json!({"message":message})))
}
fn view(s: &AppServer, p: Policy) -> Value {
    json!({"policy":p,"configured":p.budget(),"running":s.resource_budget,"low_memory":s.memory_pressure.load(std::sync::atomic::Ordering::Acquire),"available_cli_slots":s.external_agent_slots.available_permits(),"available_delegation_slots":s.delegation_slots.available_permits()})
}
async fn inspect(State(s): State<Arc<AppServer>>) -> Result<Json<Value>, Failure> {
    let p = read(&s.db.conn())
        .map_err(|_| failure(StatusCode::SERVICE_UNAVAILABLE, "资源配置读取失败"))?;
    Ok(Json(view(&s, p)))
}
async fn save(
    State(s): State<Arc<AppServer>>,
    Json(mut p): Json<Policy>,
) -> Result<Json<Value>, Failure> {
    if p.cli_limit.is_some_and(|n| !(1..=8).contains(&n))
        || p.background_interval_ms.is_some_and(|n| n > 5000)
        || p.memory_reserve_percent
            .is_some_and(|n| !(1..=45).contains(&n))
    {
        return Err(failure(StatusCode::BAD_REQUEST, "资源预算超出可用范围"));
    }
    let store_err = |_| failure(StatusCode::SERVICE_UNAVAILABLE, "资源配置写入失败");
    {
        let mut conn = s.db.conn();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(store_err)?;
        let previous =
            read(&tx).map_err(|_| failure(StatusCode::SERVICE_UNAVAILABLE, "资源配置读取失败"))?;
        if previous.revision != p.revision {
            return Err(failure(StatusCode::CONFLICT, "资源配置已改变，请重新载入"));
        }
        p.revision += 1;
        tx.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![KEY,json!(p).to_string()]).map_err(store_err)?;
        tx.commit().map_err(store_err)?;
    }
    let _ = s.event_hub.publish(crate::shared::event::YiEvent::new(
        "resource.policy.updated",
        "executor-budget",
        json!({"revision":p.revision}),
    ));
    Ok(Json(view(&s, p)))
}
pub(crate) fn router() -> Router<Arc<AppServer>> {
    Router::new().route("/api/v1/resources/policy", get(inspect).put(save))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn low_memory_recovers_only_after_the_reserve_is_replenished() {
        assert!(under_pressure(9, 10, false));
        assert!(under_pressure(12, 10, true));
        assert!(!under_pressure(13, 10, true));
    }
    #[test]
    fn manual_limits_override_only_selected_defaults() {
        let p = Policy {
            mode: Mode::Efficiency,
            cli_limit: Some(3),
            ..Policy::default()
        };
        let b = p.budget();
        assert_eq!(
            (b.cli_limit, b.delegation_limit, b.background_interval_ms),
            (3, 1, 250)
        );
    }
}
