use chrono::{Datelike, Duration, LocalResult, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Schedule {
    Once { local_at: String },
    Daily { time: String },
    Weekly { time: String, weekdays: Vec<u32> },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AutomationTarget {
    Workflow { graph_id: String },
    AgentTurn { mission_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Automation {
    pub id: String,
    pub revision: u64,
    pub name: String,
    pub workflow_graph_id: String,
    #[serde(default)]
    pub target: Option<AutomationTarget>,
    pub workspace_id: String,
    pub timezone: String,
    pub schedule: Schedule,
    pub enabled: bool,
    pub next_run_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationInput {
    pub expected_revision: Option<u64>,
    pub name: String,
    pub workflow_graph_id: String,
    #[serde(default)]
    pub target: Option<AutomationTarget>,
    pub workspace_id: String,
    pub timezone: String,
    pub schedule: Schedule,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Occurrence {
    pub id: String,
    pub automation_id: String,
    pub automation_revision: u64,
    pub timezone: String,
    pub occurrence_key: String,
    pub trigger: String,
    pub scheduled_at: i64,
    pub workspace_id: String,
    pub workflow_graph_id: String,
    pub run_id: Option<String>,
    #[serde(default)]
    pub robot_run_id: Option<String>,
    #[serde(default)]
    pub robot_channel_id: Option<String>,
    #[serde(default)]
    pub conversation_id: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Local recurrence uses IANA rules. Missing DST wall times are skipped;
/// repeated wall times select the first instant, never two occurrences.
pub fn next_after(schedule: &Schedule, timezone: &str, after: i64) -> Result<Option<i64>, String> {
    let zone: Tz = timezone
        .parse()
        .map_err(|_| "请输入有效 IANA 时区，例如 Asia/Shanghai".to_string())?;
    let after_utc = Utc
        .timestamp_millis_opt(after)
        .single()
        .ok_or("时间超出范围")?;
    if let Schedule::Once { local_at } = schedule {
        let local = NaiveDateTime::parse_from_str(local_at, "%Y-%m-%dT%H:%M")
            .map_err(|_| "一次性时间格式应为 YYYY-MM-DDTHH:mm".to_string())?;
        let instant = match zone.from_local_datetime(&local) {
            LocalResult::Single(value) => value.timestamp_millis(),
            LocalResult::Ambiguous(_, _) => {
                return Err("该时间处于夏令时重复时段，请选择明确的时间".into())
            }
            LocalResult::None => return Err("该本地时间因夏令时切换而不存在".into()),
        };
        return Ok((instant > after).then_some(instant));
    }
    let (time, weekdays) = match schedule {
        Schedule::Daily { time } => (time, None),
        Schedule::Weekly { time, weekdays } => {
            if weekdays.is_empty() || weekdays.iter().any(|day| !(1..=7).contains(day)) {
                return Err("每周任务须选择星期一至星期日中的至少一天".into());
            }
            (time, Some(weekdays))
        }
        Schedule::Once { .. } => unreachable!(),
    };
    let time =
        NaiveTime::parse_from_str(time, "%H:%M").map_err(|_| "时间格式应为 HH:mm".to_string())?;
    let today = after_utc.with_timezone(&zone).date_naive();
    for offset in 0..=15 {
        let date = today
            .checked_add_signed(Duration::days(offset))
            .ok_or("日期超出范围")?;
        if weekdays.is_some_and(|days| !days.contains(&date.weekday().number_from_monday())) {
            continue;
        }
        let instant = match zone.from_local_datetime(&date.and_time(time)) {
            LocalResult::Single(value) => value.timestamp_millis(),
            LocalResult::Ambiguous(first, _) => first.timestamp_millis(),
            LocalResult::None => continue,
        };
        if instant > after {
            return Ok(Some(instant));
        }
    }
    Err("未来两周没有可执行时间，请调整计划".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn utc(value: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(value)
            .unwrap()
            .timestamp_millis()
    }
    #[test]
    fn daily_schedule_observes_timezone_and_never_catches_up() {
        let schedule = Schedule::Daily {
            time: "09:00".into(),
        };
        assert_eq!(
            next_after(&schedule, "Asia/Shanghai", utc("2026-10-23T01:01:00Z")).unwrap(),
            Some(utc("2026-10-24T01:00:00Z"))
        );
    }
    #[test]
    fn recurring_dst_gap_is_skipped_and_fold_only_runs_once() {
        assert_eq!(
            next_after(
                &Schedule::Daily {
                    time: "02:30".into()
                },
                "America/New_York",
                utc("2026-03-08T05:00:00Z")
            )
            .unwrap(),
            Some(utc("2026-03-09T06:30:00Z"))
        );
        assert_eq!(
            next_after(
                &Schedule::Daily {
                    time: "01:30".into()
                },
                "America/New_York",
                utc("2026-11-01T05:45:00Z")
            )
            .unwrap(),
            Some(utc("2026-11-02T06:30:00Z"))
        );
    }
    #[test]
    fn once_rejects_ambiguous_time_and_weekly_requires_days() {
        assert!(next_after(
            &Schedule::Once {
                local_at: "2026-11-01T01:30".into()
            },
            "America/New_York",
            0
        )
        .is_err());
        assert!(next_after(
            &Schedule::Weekly {
                time: "09:00".into(),
                weekdays: vec![]
            },
            "UTC",
            0
        )
        .is_err());
    }
}
