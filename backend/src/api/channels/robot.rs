use super::*;
use crate::modules::channel::robot::{self, types::*};
use axum::extract::Query;
use std::sync::atomic::AtomicBool;

fn bad(message: String) -> Error {
    error(StatusCode::BAD_REQUEST, message)
}
fn qq(server: &AppServer, id: &str) -> Result<Channel, Error> {
    robot::channel(server, id).map_err(bad)
}
pub(super) fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route(
            "/api/channels/:id/robot/policy",
            get(policy).put(save_policy),
        )
        .route("/api/channels/:id/robot/directory", get(directory))
        .route("/api/channels/:id/robot/history", post(history))
        .route("/api/channels/:id/robot/send", post(send))
        .route("/api/channels/:id/robot/sends", get(manual_sends))
        .route(
            "/api/channels/:id/robot/missions",
            get(missions).post(save_mission),
        )
        .route(
            "/api/channels/:id/robot/missions/:mission",
            put(update_mission).delete(delete_mission),
        )
        .route("/api/channels/:id/robot/missions/:mission/run", post(run))
        .route("/api/channels/:id/robot/runs", get(runs))
        .route("/api/channels/:id/robot/runs/:run", get(detail))
        .route("/api/channels/:id/robot/runs/:run/stop", post(stop))
}
async fn policy(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    qq(&server, &id)?;
    Ok(Json(
        json!({"policy":server.db.robot_policy(&id).map_err(failure)?}),
    ))
}
async fn save_policy(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(mut input): Json<RobotPolicy>,
) -> Result<Json<Value>, Error> {
    let channel = qq(&server, &id)?;
    input.channel_id = id.clone();
    input.updated_at = chrono::Utc::now().timestamp_millis();
    input.account_id = channel.self_id.unwrap_or_default();
    input.validate().map_err(bad)?;
    server.db.save_robot_policy(&input).map_err(failure)?;
    if !input.enabled {
        for item in server
            .db
            .robot_runs(&id)
            .map_err(failure)?
            .into_iter()
            .filter(|r| r.active())
        {
            robot::stop(&server, &item.id).await.map_err(failure)?;
        }
    }
    Ok(Json(json!({"policy":input})))
}
#[derive(Deserialize)]
struct DirectoryQuery {
    kind: String,
}
fn string_id(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_string)
        .or_else(|| value.as_i64().map(|v| v.to_string()))
}
pub(crate) fn contacts(data: &Value, kind: &str) -> Result<Vec<RobotContact>, String> {
    let rows = data
        .as_array()
        .or_else(|| data.get("list").and_then(Value::as_array))
        .ok_or("平台没有返回有效的联系人列表")?;
    rows.iter()
        .map(|row| {
            let group = kind == "groups"
                || (kind == "recent"
                    && (row.get("chatType").and_then(Value::as_i64) == Some(2)
                        || row.get("chat_type").and_then(Value::as_i64) == Some(2)
                        || row.get("message_type").and_then(Value::as_str) == Some("group")));
            let id = row
                .get(if group { "group_id" } else { "user_id" })
                .and_then(string_id)
                .or_else(|| row.get("peerUin").and_then(string_id))
                .or_else(|| row.get("peer_uin").and_then(string_id))
                .ok_or("联系人记录缺少实际 QQ ID")?;
            let name = ["remark", "group_name", "nickname", "nick", "peerName"]
                .iter()
                .find_map(|k| row.get(k).and_then(Value::as_str).filter(|v| !v.is_empty()))
                .unwrap_or(&id)
                .to_string();
            Ok(RobotContact {
                id,
                name,
                peer_type: if group { "group" } else { "private" }.into(),
            })
        })
        .collect()
}
async fn directory(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Query(query): Query<DirectoryQuery>,
) -> Result<Json<Value>, Error> {
    let channel = qq(&server, &id)?;
    if !channel.enabled {
        return Err(bad("渠道已停用，请先启用再读取通讯录".into()));
    }
    let policy = server.db.robot_policy(&id).map_err(failure)?;
    if !policy.account_id.is_empty()
        && channel.self_id.as_deref() != Some(policy.account_id.as_str())
    {
        return Err(bad("账号已变更，请重新保存读取范围".into()));
    }
    let action = match query.kind.as_str() {
        "friends" if policy.read_friends => "get_friend_list",
        "groups" if policy.read_groups => "get_group_list",
        "recent" if policy.read_friends && policy.read_groups => "get_recent_contact",
        _ => return Err(bad("该联系人类型未授权读取".into())),
    };
    let data = robot::tools::call_action(
        &server,
        &channel,
        action,
        json!({}),
        &AtomicBool::new(false),
    )
    .await
    .map_err(|e| error(StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(
        json!({"items":contacts(&data,&query.kind).map_err(|e|error(StatusCode::BAD_GATEWAY,e))?}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryInput {
    peer_type: String,
    peer_id: String,
    count: u32,
}
async fn history(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<HistoryInput>,
) -> Result<Json<Value>, Error> {
    let channel = qq(&server, &id)?;
    if !channel.enabled {
        return Err(bad("渠道已停用，请先启用再读取历史".into()));
    }
    let policy = server.db.robot_policy(&id).map_err(failure)?;
    if !policy.account_id.is_empty()
        && channel.self_id.as_deref() != Some(policy.account_id.as_str())
    {
        return Err(bad("账号已变更，请重新保存读取范围".into()));
    }
    if !(1..=100).contains(&input.count) {
        return Err(bad("历史条数须为 1–100".into()));
    }
    let target = input
        .peer_id
        .parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| bad("QQ 目标无效".into()))?;
    let (action, field) = match input.peer_type.as_str() {
        "private" if policy.read_friends => ("get_friend_msg_history", "user_id"),
        "group" if policy.read_groups => ("get_group_msg_history", "group_id"),
        _ => return Err(bad("该会话类型未授权读取".into())),
    };
    if !robot::tools::validate_membership(&server, &channel, &input.peer_type, &input.peer_id)
        .await
        .map_err(bad)?
    {
        return Err(bad("该目标不在当前账号的好友或群列表中".into()));
    }
    let mut params = json!({"count":input.count});
    params[field] = json!(target);
    let data =
        robot::tools::call_action(&server, &channel, action, params, &AtomicBool::new(false))
            .await
            .map_err(|e| error(StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(json!({"data":data})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SendInput {
    account_id: String,
    peer_type: String,
    peer_id: String,
    text: String,
    request_id: String,
    #[serde(default)]
    approved: bool,
}
async fn send(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<SendInput>,
) -> Result<Json<Value>, Error> {
    let channel = qq(&server, &id)?;
    if channel.self_id.as_deref() != Some(input.account_id.as_str()) {
        return Err(bad("发送账号已变更，请重新确认当前账号与消息".into()));
    }
    uuid::Uuid::parse_str(&input.request_id).map_err(|_| bad("发送请求标识须为UUID".into()))?;
    let action = robot::send_text(
        &server,
        &channel,
        None,
        "desktop_send",
        &input.peer_type,
        &input.peer_id,
        &input.text,
        input.approved,
        Some(&input.request_id),
    )
    .await
    .map_err(bad)?;
    Ok(Json(json!({"action":action})))
}
async fn manual_sends(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    qq(&server, &id)?;
    Ok(Json(
        json!({"actions":server.db.robot_manual_sends(&id).map_err(failure)?}),
    ))
}
async fn missions(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    qq(&server, &id)?;
    Ok(Json(
        json!({"missions":server.db.robot_missions(&id).map_err(failure)?}),
    ))
}
fn persist_mission(
    server: &AppServer,
    channel: &str,
    id: Option<&str>,
    input: RobotMissionInput,
) -> Result<RobotMission, Error> {
    qq(server, channel)?;
    if input.name.trim().is_empty()
        || input.name.chars().count() > 80
        || input.goal.trim().is_empty()
        || input.goal.chars().count() > 16000
    {
        return Err(bad("任务名称须为1–80字符，目标须为1–16000字符".into()));
    }
    if let Some(id) = input.preset_id.as_deref() {
        server
            .db
            .channel_preset(id)
            .map_err(failure)?
            .ok_or_else(|| bad("所选预设不存在".into()))?;
    }
    if let Some(id) = input.workspace_id.as_deref() {
        crate::modules::automation::workspace(server, id).map_err(bad)?;
    }
    let old = id
        .map(|id| server.db.robot_mission(id))
        .transpose()
        .map_err(failure)?
        .flatten();
    if id.is_some() && !old.as_ref().is_some_and(|m| m.channel_id == channel) {
        return Err(error(StatusCode::NOT_FOUND, "任务不存在"));
    }
    let now = chrono::Utc::now().timestamp_millis();
    let mission = RobotMission {
        id: id
            .map(str::to_string)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        channel_id: channel.into(),
        name: input.name.trim().into(),
        goal: input.goal.trim().into(),
        preset_id: input.preset_id,
        workspace_id: input.workspace_id,
        created_at: old.map(|m| m.created_at).unwrap_or(now),
        updated_at: now,
    };
    server.db.save_robot_mission(&mission).map_err(failure)?;
    Ok(mission)
}
async fn save_mission(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<RobotMissionInput>,
) -> Result<Json<Value>, Error> {
    Ok(Json(
        json!({"mission":persist_mission(&server,&id,None,input)?}),
    ))
}
async fn update_mission(
    State(server): State<Arc<AppServer>>,
    Path((id, mission)): Path<(String, String)>,
    Json(input): Json<RobotMissionInput>,
) -> Result<Json<Value>, Error> {
    Ok(Json(
        json!({"mission":persist_mission(&server,&id,Some(&mission),input)?}),
    ))
}
async fn delete_mission(
    State(server): State<Arc<AppServer>>,
    Path((id, mission)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    qq(&server, &id)?;
    Ok(Json(
        json!({"deleted":server.db.delete_robot_mission(&id,&mission).map_err(failure)?}),
    ))
}
async fn run(
    State(server): State<Arc<AppServer>>,
    Path((id, mission)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    qq(&server, &id)?;
    let mission = server
        .db
        .robot_mission(&mission)
        .map_err(failure)?
        .filter(|m| m.channel_id == id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "任务不存在"))?;
    Ok(Json(
        json!({"run":robot::start_mission(server,mission,"desktop").await.map_err(bad)?}),
    ))
}
async fn runs(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    qq(&server, &id)?;
    Ok(Json(
        json!({"runs":server.db.robot_runs(&id).map_err(failure)?}),
    ))
}
fn get_run(server: &AppServer, channel: &str, id: &str) -> Result<RobotRun, Error> {
    qq(server, channel)?;
    server
        .db
        .robot_run(id)
        .map_err(failure)?
        .filter(|r| r.channel_id == channel)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "运行不存在"))
}
async fn detail(
    State(server): State<Arc<AppServer>>,
    Path((id, run)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    let run = get_run(&server, &id, &run)?;
    Ok(Json(
        json!({"actions":server.db.robot_actions(&run.id).map_err(failure)?,"run":run}),
    ))
}
async fn stop(
    State(server): State<Arc<AppServer>>,
    Path((id, run)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    get_run(&server, &id, &run)?;
    Ok(Json(
        json!({"run":robot::stop(&server,&run).await.map_err(failure)?}),
    ))
}
