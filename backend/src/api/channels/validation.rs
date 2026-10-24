use super::{error, failure, Error};
use crate::{
    app::state::AppServer,
    modules::channel::{self, protocol, Channel, ChannelInput},
};
use axum::http::StatusCode;

fn validate_input(input: &ChannelInput) -> Result<(), Error> {
    if !["manual", "automatic"].contains(&input.reply_mode.as_str()) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "回复方式必须为manual或automatic",
        ));
    }
    if input.name.trim().is_empty() || input.name.chars().count() > 80 {
        return Err(error(StatusCode::BAD_REQUEST, "渠道名称必须为1–80字符"));
    }
    if (input.allowed_senders.is_empty()
        && !channel::is_onebot_provider(&input.provider)
        && (input.provider != "wechat_ilink" || input.enabled))
        || input.allowed_senders.len() > 100
        || input
            .allowed_senders
            .iter()
            .any(|id| id.trim().is_empty() || id.len() > 128)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "请配置1–100个允许私聊的用户ID",
        ));
    }
    match input.provider.as_str() {
        "feishu" => {
            if !["http", "websocket"].contains(&input.connection_mode.as_str()) {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "飞书接入方式必须为http或websocket",
                ));
            }
            if input
                .app_id
                .as_deref()
                .is_none_or(|id| id.trim().is_empty())
            {
                return Err(error(StatusCode::BAD_REQUEST, "飞书需要App ID"));
            }
        }
        "onebot11" | "wechat_onebot" => {
            if input.provider == "wechat_onebot" && input.connection_mode != "reverse_websocket" {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "Akasha 微信使用 reverse_websocket 接入",
                ));
            }
            if !["http", "reverse_websocket"].contains(&input.connection_mode.as_str()) {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "OneBot接入方式必须为http或reverse_websocket",
                ));
            }
            if input
                .self_id
                .as_deref()
                .and_then(|id| id.parse::<i64>().ok())
                .is_none()
            {
                return Err(error(StatusCode::BAD_REQUEST, "OneBot需要数字self_id"));
            }
            if input
                .allowed_senders
                .iter()
                .any(|id| id.parse::<i64>().is_err())
            {
                return Err(error(StatusCode::BAD_REQUEST, "OneBot允许用户ID必须为数字"));
            }
            if input
                .allowed_groups
                .iter()
                .any(|id| id.parse::<i64>().ok().filter(|v| *v > 0).is_none())
            {
                return Err(error(StatusCode::BAD_REQUEST, "群ID必须为正整数"));
            }
            if input.connection_mode == "http" {
                let url = input
                    .api_base_url
                    .as_deref()
                    .and_then(|url| url::Url::parse(url).ok())
                    .ok_or_else(|| error(StatusCode::BAD_REQUEST, "OneBot需要HTTP API地址"))?;
                if !matches!(url.scheme(), "http" | "https")
                    || url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || url.query().is_some()
                    || url.fragment().is_some()
                {
                    return Err(error(
                        StatusCode::BAD_REQUEST,
                        "OneBot API地址必须是无凭据、查询参数的HTTP(S)地址",
                    ));
                }
            }
        }
        "wechat_ilink" => {
            if input.connection_mode != "polling" {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "微信iLink使用polling接入方式",
                ));
            }
        }
        _ => {
            return Err(error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "此渠道尚未实现，不能保存为已连接",
            ))
        }
    }
    if input.allowed_groups.len() > 100
        || !matches!(input.group_trigger.as_str(), "mention" | "all")
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "允许群最多100个，触发方式须为mention或all",
        ));
    }
    Ok(())
}
pub(super) async fn validate_save(
    server: &AppServer,
    id: &str,
    input: &ChannelInput,
    old: Option<&Channel>,
) -> Result<(), Error> {
    validate_input(input)?;
    if input.secrets.as_ref().is_some_and(|s| {
        [
            s.app_secret.as_deref(),
            s.verification_token.as_deref(),
            s.access_token.as_deref(),
            s.event_secret.as_deref(),
        ]
        .into_iter()
        .flatten()
        .any(|v| v.trim().is_empty())
    }) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "凭据不能是空字符串；保持未填即可沿用原凭据",
        ));
    }
    if let Some(id) = input.preset_id.as_deref() {
        let preset = server
            .db
            .channel_preset(id)
            .map_err(failure)?
            .ok_or_else(|| error(StatusCode::BAD_REQUEST, "回复预设不存在"))?;
        channel::presets::validate_references(
            &server.db,
            preset.agent_id.as_deref(),
            preset.model_profile_id.as_deref(),
        )
        .map_err(|e| error(StatusCode::BAD_REQUEST, e))?;
    }
    if old
        .as_ref()
        .is_some_and(|old| old.provider != input.provider)
    {
        return Err(error(
            StatusCode::CONFLICT,
            "已有渠道不能更换协议，请新建渠道",
        ));
    }
    if input.enabled {
        let names: &[&str] = match (input.provider.as_str(), input.connection_mode.as_str()) {
            ("feishu", "websocket") => &["app_secret"],
            ("feishu", _) => &["app_secret", "verification_token"],
            ("onebot11" | "wechat_onebot", "reverse_websocket") => &["access_token"],
            ("onebot11", _) => &["access_token", "event_secret"],
            ("wechat_ilink", _) => &["bot_token"],
            _ => &[],
        };
        for name in names {
            let supplied = input.secrets.as_ref().and_then(|s| match *name {
                "app_secret" => s.app_secret.as_deref(),
                "verification_token" => s.verification_token.as_deref(),
                "access_token" => s.access_token.as_deref(),
                "event_secret" => s.event_secret.as_deref(),
                _ => None,
            });
            if supplied.is_some_and(|v| !v.trim().is_empty()) {
                continue;
            }
            if server
                .secret_store
                .get(&protocol::secret_ref(id, name))
                .await
                .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "渠道凭据存储不可用"))?
                .is_none()
            {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    format!("启用前请补全{name}或完成扫码授权"),
                ));
            }
        }
    }
    Ok(())
}
