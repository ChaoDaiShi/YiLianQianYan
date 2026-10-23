//! Native text protocols. References are kept in protocol.md beside this module.
use super::{Channel, ChannelMessage, InboundMessage};
use crate::integrations::secret::{SecretRef, SecretStore};
use hmac::{Hmac, Mac};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{json, Value};
use sha1::Sha1;
use std::sync::Arc;

pub fn secret_ref(channel: &str, name: &str) -> SecretRef {
    SecretRef::new(format!("channel.{channel}.{name}"))
}
pub async fn secret(
    store: &Arc<dyn SecretStore>,
    channel: &str,
    name: &str,
) -> Result<SecretString, String> {
    store
        .get(&secret_ref(channel, name))
        .await
        .map_err(|_| "渠道凭据存储不可用")?
        .ok_or_else(|| format!("渠道缺少{name}配置"))
}
pub fn verify_token(actual: &str, expected: &str) -> bool {
    if actual.len() != expected.len() || expected.is_empty() {
        return false;
    }
    actual
        .bytes()
        .zip(expected.bytes())
        .fold(0u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}
pub fn verify_onebot_signature(body: &[u8], signature: &str, key: &str) -> Result<(), String> {
    let hex = signature
        .strip_prefix("sha1=")
        .ok_or("OneBot签名头格式无效")?;
    if hex.len() != 40 {
        return Err("OneBot签名长度无效".into());
    }
    let mut bytes = Vec::with_capacity(20);
    for pair in hex.as_bytes().chunks_exact(2) {
        let text = std::str::from_utf8(pair).map_err(|_| "OneBot签名格式无效")?;
        bytes.push(u8::from_str_radix(text, 16).map_err(|_| "OneBot签名格式无效")?);
    }
    let mut mac = Hmac::<Sha1>::new_from_slice(key.as_bytes()).map_err(|_| "OneBot签名密钥无效")?;
    mac.update(body);
    mac.verify_slice(&bytes)
        .map_err(|_| "OneBot事件签名不匹配".into())
}
fn required(value: &Value, pointer: &str) -> Result<String, String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("平台事件缺少{pointer}"))
}
fn numeric_id(value: &Value, pointer: &str) -> Result<String, String> {
    value
        .pointer(pointer)
        .and_then(Value::as_i64)
        .map(|id| id.to_string())
        .ok_or_else(|| format!("OneBot事件缺少数字字段{pointer}"))
}
pub fn parse_feishu(channel: &Channel, value: &Value) -> Result<Option<InboundMessage>, String> {
    if value.pointer("/header/event_type").and_then(Value::as_str) != Some("im.message.receive_v1")
    {
        return Ok(None);
    }
    if value.pointer("/header/app_id").and_then(Value::as_str) != channel.app_id.as_deref() {
        return Err("飞书事件App ID与渠道配置不一致".into());
    }
    if value
        .pointer("/event/message/message_type")
        .and_then(Value::as_str)
        != Some("text")
        || value
            .pointer("/event/message/chat_type")
            .and_then(Value::as_str)
            != Some("p2p")
        || value
            .pointer("/event/sender/sender_type")
            .and_then(Value::as_str)
            != Some("user")
    {
        return Ok(None);
    }
    let sender = required(value, "/event/sender/sender_id/open_id")?;
    let raw = required(value, "/event/message/content")?;
    let content: Value = serde_json::from_str(&raw).map_err(|_| "飞书text消息content不是JSON")?;
    let content = required(&content, "/text")?;
    Ok(Some(InboundMessage {
        event_id: required(value, "/header/event_id")?,
        external_message_id: required(value, "/event/message/message_id")?,
        peer_id: sender.clone(),
        sender_id: sender,
        content,
        context_token: None,
    }))
}
pub fn parse_onebot(channel: &Channel, value: &Value) -> Result<Option<InboundMessage>, String> {
    if value.get("post_type").and_then(Value::as_str) != Some("message")
        || value.get("message_type").and_then(Value::as_str) != Some("private")
    {
        return Ok(None);
    }
    let self_id = numeric_id(value, "/self_id")?;
    if Some(self_id.as_str()) != channel.self_id.as_deref() {
        return Err("OneBot self_id与渠道配置不一致".into());
    }
    let sender = numeric_id(value, "/user_id")?;
    if sender == self_id {
        return Ok(None);
    }
    let message_id = numeric_id(value, "/message_id")?;
    let content = match value.get("message") {
        Some(Value::String(message)) if !message.contains("[CQ:") => message
            .replace("&#91;", "[")
            .replace("&#93;", "]")
            .replace("&amp;", "&"),
        Some(Value::Array(segments)) => {
            if segments
                .iter()
                .any(|segment| segment.get("type").and_then(Value::as_str) != Some("text"))
            {
                return Ok(None);
            }
            segments
                .iter()
                .map(|segment| required(segment, "/data/text"))
                .collect::<Result<Vec<_>, _>>()?
                .concat()
        }
        _ => return Ok(None),
    };
    Ok(Some(InboundMessage {
        event_id: format!("{self_id}:{message_id}"),
        external_message_id: message_id,
        peer_id: sender.clone(),
        sender_id: sender,
        content,
        context_token: None,
    }))
}

pub async fn send_reply(
    channel: &Channel,
    message: &ChannelMessage,
    text: &str,
    store: &Arc<dyn SecretStore>,
    dispatched: &std::sync::atomic::AtomicBool,
) -> Result<String, String> {
    use std::sync::atomic::Ordering;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法创建渠道HTTP客户端")?;
    if channel.provider == "feishu" {
        let app_secret = secret(store, &channel.id, "app_secret").await?;
        let auth = client
            .post("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal")
            .json(&json!({"app_id":channel.app_id,"app_secret":app_secret.expose_secret()}))
            .send()
            .await
            .map_err(|_| "飞书凭据请求失败，未发送回复")?;
        if !auth.status().is_success() {
            return Err(format!("飞书凭据请求HTTP {}", auth.status().as_u16()));
        }
        let auth: Value = auth.json().await.map_err(|_| "飞书凭据响应无效")?;
        if auth.get("code").and_then(Value::as_i64) != Some(0) {
            return Err("飞书拒绝了应用凭据".into());
        }
        let token = auth
            .get("tenant_access_token")
            .and_then(Value::as_str)
            .ok_or("飞书没有返回tenant_access_token")?;
        let mut endpoint = url::Url::parse("https://open.feishu.cn/open-apis/im/v1/messages/")
            .map_err(|_| "飞书地址无效")?;
        endpoint
            .path_segments_mut()
            .map_err(|_| "飞书地址路径无效")?
            .pop_if_empty()
            .push(&message.external_message_id)
            .push("reply");
        dispatched.store(true, Ordering::SeqCst);
        let response=client.post(endpoint).bearer_auth(token).json(&json!({"msg_type":"text","content":json!({"text":text}).to_string(),"uuid":message.id})).send().await.map_err(|_|"飞书回复请求中断，送达状态未知")?;
        if !response.status().is_success() {
            if response.status().is_client_error() {
                dispatched.store(false, Ordering::SeqCst);
            }
            return Err(format!(
                "飞书回复HTTP {}，未确认送达",
                response.status().as_u16()
            ));
        }
        let body: Value = response
            .json()
            .await
            .map_err(|_| "飞书回复响应格式无效，送达状态未知")?;
        if body.get("code").and_then(Value::as_i64) != Some(0) {
            dispatched.store(false, Ordering::SeqCst);
            return Err(format!(
                "飞书回复失败（code={}）",
                body.get("code").and_then(Value::as_i64).unwrap_or(-1)
            ));
        }
        required(&body, "/data/message_id")
    } else if channel.provider == "onebot11" {
        let token = secret(store, &channel.id, "access_token").await?;
        let base = channel
            .api_base_url
            .as_deref()
            .ok_or("OneBot API地址缺失")?;
        let user_id = message
            .sender_id
            .parse::<i64>()
            .map_err(|_| "OneBot收件人ID无效")?;
        dispatched.store(true, Ordering::SeqCst);
        let response = client
            .post(format!("{}/send_private_msg", base.trim_end_matches('/')))
            .bearer_auth(token.expose_secret())
            .json(&json!({"user_id":user_id,"message":[{"type":"text","data":{"text":text}}]}))
            .send()
            .await
            .map_err(|_| "OneBot回复请求中断，送达状态未知")?;
        if !response.status().is_success() {
            if response.status().is_client_error() {
                dispatched.store(false, Ordering::SeqCst);
            }
            return Err(format!(
                "OneBot回复HTTP {}，未确认送达",
                response.status().as_u16()
            ));
        }
        let body: Value = response
            .json()
            .await
            .map_err(|_| "OneBot回复响应格式无效，送达状态未知")?;
        if body.get("status").and_then(Value::as_str) != Some("ok")
            || body.get("retcode").and_then(Value::as_i64) != Some(0)
        {
            if body.get("status").and_then(Value::as_str) == Some("failed") {
                dispatched.store(false, Ordering::SeqCst);
            }
            return Err("OneBot未确认同步发送成功".into());
        }
        numeric_id(&body, "/data/message_id")
    } else {
        Err("此渠道协议尚未实现".into())
    }
}
