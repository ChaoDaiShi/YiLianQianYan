use super::*;
use crate::modules::channel::{ChannelPreset, ChannelPresetInput};

pub(super) async fn list(State(server): State<Arc<AppServer>>) -> Result<Json<Value>, Error> {
    Ok(Json(
        json!({"presets":server.db.channel_presets().map_err(failure)?}),
    ))
}

fn build(
    server: &AppServer,
    input: ChannelPresetInput,
    old: Option<ChannelPreset>,
) -> Result<ChannelPreset, Error> {
    channel::presets::validate(&server.db, &input)
        .map_err(|e| error(StatusCode::BAD_REQUEST, e))?;
    if old.as_ref().is_some_and(|p| p.builtin) {
        return Err(error(
            StatusCode::CONFLICT,
            "内置预设只读，请复制为自定义预设",
        ));
    }
    let now = chrono::Utc::now().timestamp_millis();
    Ok(ChannelPreset {
        id: old
            .as_ref()
            .map(|p| p.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name: input.name.trim().into(),
        agent_id: input.agent_id,
        model_profile_id: input.model_profile_id,
        instructions: input.instructions,
        builtin: false,
        created_at: old.as_ref().map(|p| p.created_at).unwrap_or(now),
        updated_at: now,
    })
}

fn get(server: &AppServer, id: &str) -> Result<ChannelPreset, Error> {
    server
        .db
        .channel_preset(id)
        .map_err(failure)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "回复预设不存在"))
}

pub(super) async fn create(
    State(server): State<Arc<AppServer>>,
    Json(input): Json<ChannelPresetInput>,
) -> Result<Json<Value>, Error> {
    let preset = build(&server, input, None)?;
    server.db.save_channel_preset(&preset).map_err(failure)?;
    Ok(Json(json!({"preset":preset})))
}

pub(super) async fn update(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<ChannelPresetInput>,
) -> Result<Json<Value>, Error> {
    let preset = build(&server, input, Some(get(&server, &id)?))?;
    if !server.db.update_channel_preset(&preset).map_err(failure)? {
        return Err(error(StatusCode::NOT_FOUND, "预设已删除，未重新创建"));
    }
    Ok(Json(json!({"preset":preset})))
}

pub(super) async fn delete(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    get(&server, &id)?;
    server
        .db
        .delete_channel_preset(&id)
        .map_err(|e| error(StatusCode::CONFLICT, e))?;
    Ok(Json(json!({"deleted":true})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Preview {
    message: String,
}

pub(super) async fn preview(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<Preview>,
) -> Result<Json<Value>, Error> {
    use crate::integrations::llm::types::ChatMessage;
    if input.message.trim().is_empty() || input.message.chars().count() > 8000 {
        return Err(error(StatusCode::BAD_REQUEST, "试用消息须为1–8000字符"));
    }
    let preset = get(&server, &id)?;
    channel::presets::validate_references(
        &server.db,
        preset.agent_id.as_deref(),
        preset.model_profile_id.as_deref(),
    )
    .map_err(|e| error(StatusCode::BAD_REQUEST, e))?;
    let conversation = server
        .db
        .create_conversation(&format!("预设试用 · {}", preset.name))
        .map_err(failure)?;
    server
        .db
        .bind_channel_agent(&conversation.id, preset.agent_id.as_deref())
        .map_err(failure)?;
    let mut runtime = crate::api::chat::runtime::prepare_with_model(
        &server,
        Some(&conversation.id),
        None,
        preset.model_profile_id.as_deref(),
    )
    .await
    .map_err(failure)?;
    runtime.prompt = channel::presets::prompt(&server.db, Some(&preset)).map_err(failure)?;
    runtime.tools = Arc::new(crate::tools::ToolRegistry::new());
    runtime.bindings = Default::default();
    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    let client = crate::api::chat::runtime::client(&server, &runtime, &conversation.id, &tx)
        .map_err(failure)?;
    let user = ChatMessage::user(&input.message);
    server
        .db
        .persist_chat_message(
            &conversation.id,
            &user,
            chrono::Utc::now().timestamp_millis(),
        )
        .map_err(failure)?;
    let result = client
        .invoke(&[ChatMessage::system(&runtime.prompt), user], &[])
        .await
        .map_err(|e| {
            error(
                StatusCode::BAD_GATEWAY,
                match e {
                    crate::integrations::llm::client::LlmError::NoApiKey => {
                        "模型尚未配置API凭据，请先完成模型设置".to_string()
                    }
                    crate::integrations::llm::client::LlmError::SecretUnavailable => {
                        "本机凭据库不可用".to_string()
                    }
                    crate::integrations::llm::client::LlmError::Timeout => {
                        "模型试用请求超时".to_string()
                    }
                    _ => "模型试用失败，请检查模型服务、档案和凭据配置".to_string(),
                },
            )
        })?;
    let answer = result
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| error(StatusCode::BAD_GATEWAY, "模型未返回回复"))?
        .message;
    let reply = answer
        .content
        .clone()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| error(StatusCode::BAD_GATEWAY, "模型没有生成文本回复"))?;
    if answer
        .tool_calls
        .as_ref()
        .is_some_and(|calls| !calls.is_empty())
    {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            "预设试用仅生成文本，不执行模型返回的工具调用",
        ));
    }
    server
        .db
        .persist_chat_message(
            &conversation.id,
            &answer,
            chrono::Utc::now().timestamp_millis(),
        )
        .map_err(failure)?;
    Ok(Json(
        json!({"reply":reply,"conversation_id":conversation.id}),
    ))
}
