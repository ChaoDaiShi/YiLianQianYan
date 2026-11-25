//! First-run drafts and explicitly supplied user preferences belong to the active local database.
use crate::{
    app::state::AppServer, db::Database, modules::capability::system_preferences as preferences,
};
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};

const FIELDS: &[&str] = &[
    "display_name",
    "language",
    "time_zone",
    "background",
    "scenario",
    "reply_length",
    "style",
    "initiative",
    "depth",
    "show_steps",
];
const CAPABILITIES: &[&str] = &["memory", "screen", "microphone", "files", "external_model"];

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Profile {
    fields: BTreeMap<String, String>,
    permissions: BTreeMap<String, bool>,
}

#[derive(Serialize)]
struct Snapshot {
    flow_version: u32,
    scope: String,
    revision: i64,
    current_step: u8,
    draft: Option<Profile>,
    draft_workspace: Option<String>,
    profile: Profile,
    field_dates: BTreeMap<String, i64>,
    completed_at: Option<i64>,
    setup_completed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    expected_revision: i64,
    current_step: u8,
    profile: Profile,
    workspace_id: Option<String>,
}

#[derive(Debug)]
enum Error {
    Storage,
    Invalid,
    Conflict,
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        tracing::error!(%error, "local onboarding database error");
        Self::Storage
    }
}
impl From<serde_json::Error> for Error {
    fn from(_: serde_json::Error) -> Self {
        Self::Storage
    }
}

fn snapshot(conn: &Connection) -> Result<Snapshot, Error> {
    let (scope, revision, current_step, draft, profile, dates, completed_at):
        (String, i64, u8, Option<String>, String, String, Option<i64>) = conn.query_row(
        "SELECT scope,revision,current_step,draft,profile,field_dates,completed_at FROM local_onboarding WHERE singleton=1", [],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?)))?;
    let draft_workspace = conn.query_row(
        "SELECT draft_workspace FROM local_onboarding WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    Ok(Snapshot {
        flow_version: 1,
        scope,
        revision,
        current_step,
        draft_workspace,
        draft: draft.map(|v| serde_json::from_str(&v)).transpose()?,
        profile: serde_json::from_str(&profile)?,
        field_dates: serde_json::from_str(&dates)?,
        completed_at,
        setup_completed: preferences::read(conn)
            .map_err(|_| Error::Storage)?
            .setup_completed,
    })
}

enum Operation {
    Draft,
    Complete,
    Edit,
}
fn update(db: &Database, mut input: Change, op: Operation) -> Result<Snapshot, Error> {
    if input.current_step > 6
        || input
            .profile
            .fields
            .iter()
            .any(|(k, v)| !FIELDS.contains(&k.as_str()) || v.chars().count() > 512)
        || input
            .profile
            .permissions
            .keys()
            .any(|k| !CAPABILITIES.contains(&k.as_str()))
    {
        return Err(Error::Invalid);
    }
    input.profile.fields.retain(|_, v| {
        *v = v.trim().to_owned();
        !v.is_empty()
    });
    let mut conn = db.conn();
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let old = snapshot(&tx)?;
    if input.expected_revision != old.revision {
        return Err(Error::Conflict);
    }
    let encoded = serde_json::to_string(&input.profile)?;
    let now = chrono::Utc::now().timestamp_millis();
    if matches!(op, Operation::Draft) {
        tx.execute("UPDATE local_onboarding SET revision=revision+1,current_step=?1,draft=?2,draft_workspace=?3 WHERE singleton=1", params![input.current_step,encoded,input.workspace_id])?;
    } else {
        let mut dates = old.field_dates;
        dates.retain(|k, _| input.profile.fields.contains_key(k));
        for (key, value) in &input.profile.fields {
            if old.profile.fields.get(key) != Some(value) {
                dates.insert(key.clone(), now);
            }
        }
        tx.execute("UPDATE local_onboarding SET revision=revision+1,current_step=0,draft=NULL,draft_workspace=NULL,profile=?1,field_dates=?2,
            completed_at=CASE WHEN ?3 THEN ?4 ELSE completed_at END WHERE singleton=1",
            params![encoded,serde_json::to_string(&dates)?,matches!(op, Operation::Complete),now])?;
        if matches!(op, Operation::Complete) {
            let mut prefs = preferences::read(&tx).map_err(|_| Error::Storage)?;
            prefs.setup_completed = true;
            prefs.revision += 1;
            tx.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![preferences::KEY,serde_json::to_string(&prefs)?])?;
            if let Some(id) = input.workspace_id {
                if tx.execute("INSERT INTO settings(key,value) SELECT 'default_workspace_id',id FROM workspaces WHERE id=?1 AND status='active' ON CONFLICT(key) DO UPDATE SET value=excluded.value", [id])? != 1 {return Err(Error::Invalid);}
            }
        }
    }
    let next = snapshot(&tx)?;
    tx.commit()?;
    Ok(next)
}

pub fn allowed(db: &Database, capability: &str) -> bool {
    if !CAPABILITIES.contains(&capability) {
        return false;
    }
    let Ok(profile) = committed_profile(db) else {
        return false;
    };
    profile
        .permissions
        .get(capability)
        .copied()
        .unwrap_or(capability != "screen")
}

/// Existing mounts carry their own user-selected target and observation grant.
pub(crate) fn existing_window_capture_enabled(db: &Database) -> bool {
    committed_profile(db)
        .is_ok_and(|profile| profile.permissions.get("screen").copied().unwrap_or(true))
}

fn committed_profile(db: &Database) -> Result<Profile, Error> {
    let text: String = db.conn().query_row(
        "SELECT profile FROM local_onboarding WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&text)?)
}

pub(crate) fn conversation_preferences(db: &Database) -> Result<Option<String>, String> {
    let profile = committed_profile(db).map_err(|_| "用户资料读取失败".to_string())?;
    if profile.fields.is_empty() {
        return Ok(None);
    }
    serde_json::to_string(&profile.fields)
        .map(Some)
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfilePatch {
    expected_revision: i64,
    #[serde(default)]
    fields: BTreeMap<String, Option<String>>,
    #[serde(default)]
    permissions: BTreeMap<String, bool>,
}
async fn patch_profile(
    State(s): State<Arc<AppServer>>,
    Json(patch): Json<ProfilePatch>,
) -> Result<Json<Snapshot>, HttpError> {
    let mut profile = committed_profile(&s.db).map_err(response)?;
    for (name, value) in patch.fields {
        if !FIELDS.contains(&name.as_str()) {
            return Err(response(Error::Invalid));
        }
        if let Some(value) = value {
            profile.fields.insert(name, value);
        } else {
            profile.fields.remove(&name);
        }
    }
    profile.permissions.extend(patch.permissions);
    change(
        s,
        Change {
            expected_revision: patch.expected_revision,
            current_step: 0,
            profile,
            workspace_id: None,
        },
        Operation::Edit,
    )
    .await
}
type HttpError = (StatusCode, Json<Value>);
fn response(error: Error) -> HttpError {
    let (status, code, message) = match error {
        Error::Conflict => (
            StatusCode::CONFLICT,
            "revision_conflict",
            "资料已在其他窗口改变，请重新载入后编辑。",
        ),
        Error::Invalid => (
            StatusCode::BAD_REQUEST,
            "invalid_profile",
            "资料、步骤或工作空间无效。",
        ),
        Error::Storage => (
            StatusCode::SERVICE_UNAVAILABLE,
            "profile_storage",
            "本地资料暂时无法读取或保存。",
        ),
    };
    (status, Json(json!({"code":code,"message":message})))
}

async fn status(State(s): State<Arc<AppServer>>) -> Result<Json<Snapshot>, HttpError> {
    snapshot(&s.db.conn()).map(Json).map_err(response)
}
async fn profile(State(s): State<Arc<AppServer>>) -> Result<Json<Value>, HttpError> {
    let v = snapshot(&s.db.conn()).map_err(response)?;
    Ok(Json(
        json!({"scope":v.scope,"revision":v.revision,"profile":v.profile,"field_dates":v.field_dates,"source":"user"}),
    ))
}
async fn change(
    s: Arc<AppServer>,
    input: Change,
    op: Operation,
) -> Result<Json<Snapshot>, HttpError> {
    let draft = matches!(op, Operation::Draft);
    let result = update(&s.db, input, op).map_err(response)?;
    if !draft {
        if result.profile.permissions.get("microphone") == Some(&false) {
            if let Some(session) = s.voice_runtime.session() {
                let _ = s
                    .voice_runtime
                    .end(&session.voice_session_id, session.generation);
            }
        }
        let _ = s.event_hub.publish(
            crate::shared::event::YiEvent::new(
                "profile.updated",
                "onboarding",
                json!({"revision":result.revision}),
            )
            .with_scope(&result.scope),
        );
    }
    Ok(Json(result))
}
pub(crate) fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route("/api/v1/onboarding/status", get(status))
        .route(
            "/api/v1/onboarding/steps",
            post(|State(s), Json(input)| change(s, input, Operation::Draft)),
        )
        .route(
            "/api/v1/onboarding/complete",
            post(|State(s), Json(input)| change(s, input, Operation::Complete)),
        )
        .route(
            "/api/v1/profile",
            get(profile)
                .patch(patch_profile)
                .put(|State(s), Json(input)| change(s, input, Operation::Edit)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn change_at(rev: i64) -> Change {
        Change {
            expected_revision: rev,
            current_step: 3,
            profile: Profile {
                fields: BTreeMap::from([("language".into(), "中文".into())]),
                permissions: BTreeMap::from([("screen".into(), true)]),
            },
            workspace_id: None,
        }
    }
    #[test]
    fn invalid_workspace_rolls_back_profile_and_legacy_completion_together() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let mut input = change_at(0);
        input.workspace_id = Some("deleted-workspace".into());
        assert!(matches!(
            update(&db, input, Operation::Complete),
            Err(Error::Invalid)
        ));
        let old = snapshot(&db.conn()).unwrap();
        assert_eq!(old.revision, 0);
        assert!(!old.setup_completed && !allowed(&db, "screen"));
        let mut prefs = preferences::load(&db).unwrap();
        prefs.setup_completed = true;
        preferences::save(&db, prefs).unwrap();
        assert!(snapshot(&db.conn()).unwrap().setup_completed);
    }

    #[test]
    fn interrupted_setup_and_conflicts_survive_reopen() {
        let path =
            std::env::temp_dir().join(format!("yilian-onboarding-{}.db", uuid::Uuid::new_v4()));
        let db = Database::new(&path).unwrap();
        let saved = update(&db, change_at(0), Operation::Draft).unwrap();
        assert!(!allowed(&db, "screen"));
        drop(db);
        let db = Database::new(&path).unwrap();
        let resumed = snapshot(&db.conn()).unwrap();
        assert_eq!((resumed.current_step, resumed.scope), (3, saved.scope));
        assert_eq!(resumed.draft.unwrap().fields["language"], "中文");
        assert!(matches!(
            update(&db, change_at(0), Operation::Complete),
            Err(Error::Conflict)
        ));
        let done = update(&db, change_at(1), Operation::Complete).unwrap();
        assert!(done.setup_completed && allowed(&db, "screen"));
        let mut edit = change_at(2);
        edit.profile = Profile::default();
        let cleared = update(&db, edit, Operation::Edit).unwrap();
        assert!(
            cleared.draft.is_none()
                && cleared.field_dates.is_empty()
                && conversation_preferences(&db).unwrap().is_none()
        );
        assert!(!allowed(&db, "screen"));
        drop(db);
        std::fs::remove_file(path).unwrap();
    }
}
