use crate::modules::channel::InboundMessage;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client, Response, Url,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub(super) const DEFAULT_API_BASE_URL: &str = "https://ilinkai.weixin.qq.com";
const BOT_TYPE: &str = "3";
const HTTP_TIMEOUT: Duration = Duration::from_secs(40);
const APP_ID: &str = "bot";

pub(super) struct IlinkClient {
    http: Client,
    base_url: Url,
    uin: String,
}

pub(super) struct QrCode {
    pub qrcode: String,
    pub scan_content: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct QrStatus {
    pub status: String,
    pub bot_token: Option<String>,
    pub ilink_bot_id: Option<String>,
    pub baseurl: Option<String>,
    pub ilink_user_id: Option<String>,
    pub redirect_host: Option<String>,
}

pub(super) struct Updates {
    pub messages: Vec<Value>,
    pub get_updates_buf: Option<String>,
}

pub(super) enum ParsedInbound {
    Message(InboundMessage),
    Ignored(&'static str),
    Unsupported {
        kind: &'static str,
        sender_id: Option<String>,
    },
}

impl IlinkClient {
    pub(super) fn new(base_url: &str) -> Result<Self, String> {
        let parsed = Url::parse(base_url).map_err(|_| "微信 iLink API 地址无效")?;
        validate_api_base_url(&parsed)?;
        let http = Client::builder()
            .timeout(HTTP_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "无法创建微信 iLink HTTP 客户端")?;
        let uin = random_wechat_uin();
        Self::with_http(http, parsed, uin)
    }

    pub(super) fn with_http(http: Client, mut base_url: Url, uin: String) -> Result<Self, String> {
        if !base_url.path().ends_with('/') {
            let mut path = base_url.path().to_owned();
            path.push('/');
            base_url.set_path(&path);
        }
        Ok(Self {
            http,
            base_url,
            uin,
        })
    }

    pub(super) async fn fetch_qr(&self) -> Result<QrCode, String> {
        let mut url = self.endpoint("ilink/bot/get_bot_qrcode")?;
        url.query_pairs_mut().append_pair("bot_type", BOT_TYPE);
        let response = self
            .http
            .post(url)
            .headers(self.qr_start_headers()?)
            .json(&json!({ "local_token_list": [] }))
            .send()
            .await
            .map_err(|_| "微信 iLink 二维码请求失败")?;
        let value = Self::json_response(response, "二维码请求").await?;
        let qrcode = value
            .get("qrcode")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or("微信 iLink 没有返回二维码状态值")?;
        let scan_content = value
            .get("qrcode_img_content")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or("微信 iLink 没有返回可扫码内容")?;
        Ok(QrCode {
            qrcode: qrcode.to_owned(),
            scan_content: scan_content.to_owned(),
        })
    }

    pub(super) async fn qr_status(
        &self,
        qrcode: &str,
        verify_code: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<QrStatus, String> {
        let mut url = self.endpoint("ilink/bot/get_qrcode_status")?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("qrcode", qrcode);
            if let Some(code) = verify_code {
                query.append_pair("verify_code", code);
            }
        }
        let request = self.http.get(url).headers(self.qr_status_headers()?);
        let result = async {
            let response = request
                .send()
                .await
                .map_err(|_| "微信 iLink 扫码状态请求失败")?;
            let value = Self::json_response(response, "扫码状态请求").await?;
            serde_json::from_value(value).map_err(|_| "微信 iLink 扫码状态响应无效".into())
        };
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err("微信 iLink 扫码状态轮询已取消".into()),
            response = result => response,
        }
    }

    pub(super) async fn get_updates(
        &self,
        bot_token: &str,
        cursor: &str,
        cancel: &CancellationToken,
    ) -> Result<Updates, String> {
        let url = self.endpoint("ilink/bot/getupdates")?;
        let request = self
            .http
            .post(url)
            .headers(self.bot_headers(bot_token)?)
            .json(&json!({
                "get_updates_buf": cursor,
                "base_info": base_info(),
            }));
        let result = async {
            let response = request
                .send()
                .await
                .map_err(|_| "微信 iLink 长轮询请求失败")?;
            let value = Self::json_response(response, "长轮询").await?;
            let ret = value
                .get("ret")
                .map(|ret| ret.as_i64().ok_or("微信 iLink 长轮询返回码无效"))
                .transpose()?;
            let errcode = value
                .get("errcode")
                .map(|code| code.as_i64().ok_or("微信 iLink 长轮询errcode无效"))
                .transpose()?;
            if let Some(ret) = ret
                .filter(|ret| *ret != 0)
                .or_else(|| errcode.filter(|code| *code != 0))
            {
                let message = value
                    .get("errmsg")
                    .and_then(Value::as_str)
                    .filter(|message| !message.is_empty())
                    .unwrap_or("微信 iLink 拒绝了长轮询请求");
                return Err(if ret == -14 {
                    "微信 iLink 登录凭据已失效，请重新扫码".into()
                } else {
                    format!("{message} (ret={ret})")
                });
            }
            let messages = match value.get("msgs") {
                None | Some(Value::Null) => Vec::new(),
                Some(Value::Array(messages)) => messages.clone(),
                _ => return Err("微信 iLink 长轮询消息列表无效".into()),
            };
            let get_updates_buf = match value.get("get_updates_buf") {
                None | Some(Value::Null) => None,
                Some(Value::String(cursor)) => Some(cursor.clone()),
                _ => return Err("微信 iLink 长轮询游标无效".into()),
            };
            Ok(Updates {
                messages,
                get_updates_buf,
            })
        };
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err("微信 iLink 长轮询已取消".into()),
            response = result => response,
        }
    }

    pub(super) async fn send_reply(
        &self,
        bot_token: &str,
        peer_id: &str,
        text: &str,
        context_token: &str,
        dispatched: &std::sync::atomic::AtomicBool,
    ) -> Result<String, String> {
        use std::sync::atomic::Ordering;

        let client_id = uuid::Uuid::new_v4().to_string();
        let url = self.endpoint("ilink/bot/sendmessage")?;
        let request = self
            .http
            .post(url)
            .headers(self.bot_headers(bot_token)?)
            .json(&json!({
                "msg": {
                    "from_user_id": "",
                    "to_user_id": peer_id,
                    "client_id": client_id,
                    "message_type": 2,
                    "message_state": 2,
                    "context_token": context_token,
                    "item_list": [{ "type": 1, "text_item": { "text": text } }],
                },
                "base_info": base_info(),
            }))
            .build()
            .map_err(|_| "微信 iLink 回复请求无效")?;

        if dispatched.swap(true, Ordering::SeqCst) {
            return Err("微信 iLink 回复请求已提交，不会重复发送".into());
        }
        let response = self
            .http
            .execute(request)
            .await
            .map_err(|_| "微信 iLink 回复请求中断，送达状态未知")?;
        if response.status().is_client_error() {
            dispatched.store(false, Ordering::SeqCst);
        }
        let value = Self::json_response(response, "回复").await?;
        // SendMessageResp.ret is optional. Tencent's official client and test
        // explicitly accept HTTP 200 `{}` (protobuf's omitted zero value).
        let object = value
            .as_object()
            .ok_or("微信 iLink 回复响应不是JSON对象，送达状态未知")?;
        let ret = match object.get("ret") {
            None => 0,
            Some(value) => value
                .as_i64()
                .ok_or("微信 iLink 回复返回码无效，送达状态未知")?,
        };
        if ret != 0 {
            dispatched.store(false, Ordering::SeqCst);
            let message = value
                .get("errmsg")
                .and_then(Value::as_str)
                .filter(|message| !message.is_empty())
                .unwrap_or("微信 iLink 未确认接收回复");
            return Err(format!("{message} (ret={ret})"));
        }
        Ok(client_id)
    }

    fn endpoint(&self, path: &str) -> Result<Url, String> {
        self.base_url
            .join(path)
            .map_err(|_| "微信 iLink API 地址无效".into())
    }

    fn qr_start_headers(&self) -> Result<HeaderMap, String> {
        let mut headers = self.app_headers()?;
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        headers.insert(
            "authorizationtype",
            HeaderValue::from_static("ilink_bot_token"),
        );
        headers.insert(
            "x-wechat-uin",
            HeaderValue::from_str(&self.uin).map_err(|_| "微信 iLink UIN 请求头无效")?,
        );
        Ok(headers)
    }

    fn qr_status_headers(&self) -> Result<HeaderMap, String> {
        self.app_headers()
    }

    fn bot_headers(&self, bot_token: &str) -> Result<HeaderMap, String> {
        let mut headers = self.qr_start_headers()?;
        headers.insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {bot_token}"))
                .map_err(|_| "微信 iLink bot_token 请求头无效")?,
        );
        Ok(headers)
    }

    fn app_headers(&self) -> Result<HeaderMap, String> {
        let mut headers = HeaderMap::new();
        headers.insert("ilink-app-id", HeaderValue::from_static(APP_ID));
        headers.insert(
            "ilink-app-clientversion",
            HeaderValue::from_str(&client_version()).map_err(|_| "微信 iLink 客户端版本无效")?,
        );
        Ok(headers)
    }

    async fn json_response(response: Response, operation: &str) -> Result<Value, String> {
        let status = response.status();
        if !status.is_success() {
            return Err(format!("微信 iLink {operation} HTTP {}", status.as_u16()));
        }
        response
            .json()
            .await
            .map_err(|_| format!("微信 iLink {operation}响应不是有效 JSON"))
    }
}

pub(super) fn validate_api_base_url(url: &Url) -> Result<(), String> {
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("微信 iLink API 地址必须是无凭据的 HTTPS 地址".into());
    }
    Ok(())
}

pub(super) fn parse_message(value: &Value, bot_id: &str) -> Result<ParsedInbound, String> {
    let sender_id = value
        .get("from_user_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let Some(message_type) = value.get("message_type").and_then(Value::as_i64) else {
        return Ok(ParsedInbound::Unsupported {
            kind: "malformed_message",
            sender_id,
        });
    };
    if message_type != 1 {
        return Ok(if message_type == 2 {
            ParsedInbound::Ignored("平台回显消息")
        } else {
            ParsedInbound::Unsupported {
                kind: "unsupported_message_type",
                sender_id,
            }
        });
    }
    if value.get("group_id").is_some_and(|group_id| {
        !group_id.is_null() && group_id.as_str().map(str::is_empty) != Some(true)
    }) {
        return Ok(ParsedInbound::Unsupported {
            kind: "group_message",
            sender_id,
        });
    }
    let Some(sender) = sender_id.as_deref() else {
        return Ok(ParsedInbound::Unsupported {
            kind: "malformed_message",
            sender_id: None,
        });
    };
    let Some(recipient) = value
        .get("to_user_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    else {
        return Ok(ParsedInbound::Unsupported {
            kind: "malformed_message",
            sender_id: Some(sender.to_owned()),
        });
    };
    if recipient != bot_id {
        return Ok(ParsedInbound::Unsupported {
            kind: "unexpected_recipient",
            sender_id: Some(sender.to_owned()),
        });
    }

    let Some(items) = value.get("item_list").and_then(Value::as_array) else {
        return Ok(ParsedInbound::Unsupported {
            kind: "malformed_message",
            sender_id: Some(sender.to_owned()),
        });
    };
    if items.is_empty() {
        return Ok(ParsedInbound::Unsupported {
            kind: "non_text_message",
            sender_id: Some(sender.to_owned()),
        });
    }
    let mut content = String::new();
    for item in items {
        if item.get("type").and_then(Value::as_i64) != Some(1) {
            return Ok(ParsedInbound::Unsupported {
                kind: "media_or_mixed_message",
                sender_id: Some(sender.to_owned()),
            });
        }
        let Some(text) = item.pointer("/text_item/text").and_then(Value::as_str) else {
            return Ok(ParsedInbound::Unsupported {
                kind: "malformed_text_item",
                sender_id: Some(sender.to_owned()),
            });
        };
        content.push_str(text);
    }
    if content.trim().is_empty() {
        return Ok(ParsedInbound::Unsupported {
            kind: "empty_text_message",
            sender_id: Some(sender.to_owned()),
        });
    }
    if content.chars().count() > 16000 {
        return Ok(ParsedInbound::Unsupported {
            kind: "oversized_text_message",
            sender_id: Some(sender.to_owned()),
        });
    }

    let Some(external_message_id) =
        identifier(value.get("message_id")).or_else(|| identifier(value.get("seq")))
    else {
        return Ok(ParsedInbound::Unsupported {
            kind: "malformed_message",
            sender_id: Some(sender.to_owned()),
        });
    };
    let context_token = value
        .get("context_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .map(str::to_owned);
    Ok(ParsedInbound::Message(InboundMessage {
        event_id: external_message_id.clone(),
        external_message_id,
        peer_id: sender.to_owned(),
        sender_id: sender.to_owned(),
        content,
        context_token,
    }))
}

fn identifier(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(value) if !value.is_empty() => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn base_info() -> Value {
    json!({
        "channel_version": env!("CARGO_PKG_VERSION"),
        "bot_agent": concat!("YiLianQianYan/", env!("CARGO_PKG_VERSION")),
    })
}

fn client_version() -> String {
    let mut parts = env!("CARGO_PKG_VERSION")
        .split('.')
        .map(|part| part.parse::<u32>().unwrap_or(0));
    let major = parts.next().unwrap_or(0) & 0xff;
    let minor = parts.next().unwrap_or(0) & 0xff;
    let patch = parts.next().unwrap_or(0) & 0xff;
    ((major << 16) | (minor << 8) | patch).to_string()
}

fn random_wechat_uin() -> String {
    let number = (uuid::Uuid::new_v4().as_u128() as u32).to_string();
    STANDARD.encode(number.as_bytes())
}
