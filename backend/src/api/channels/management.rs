use super::*;

pub(super) async fn delete(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    let gate = server
        .channel_runtime
        .processing_gate(&format!("management:{id}"));
    let _operation = gate.lock().await;
    let mut current = get_channel(&server, &id)?;
    if server.db.channel_is_sending(&id).map_err(failure)? {
        return Err(error(
            StatusCode::CONFLICT,
            "渠道正在等待发送回执，请待结果明确后再删除",
        ));
    }
    current.enabled = false;
    current.status = "disabled".into();
    server.db.save_channel(&current).map_err(failure)?;
    server.channel_runtime.logins.cancel(&id).await;
    server.channel_runtime.stop(&id).await;
    channel::stop_processing(&server, &id)
        .await
        .map_err(failure)?;
    let ingress = server
        .channel_runtime
        .processing_gate(&format!("ingress:{id}"));
    let _receiving = ingress.lock().await;
    let mut names = vec![
        "app_secret".into(),
        "verification_token".into(),
        "access_token".into(),
        "event_secret".into(),
        "bot_token".into(),
    ];
    names.extend(
        server
            .db
            .channel_message_ids(&id)
            .map_err(failure)?
            .into_iter()
            .map(|message| format!("context.{message}")),
    );
    for name in names {
        server
            .secret_store
            .delete(&protocol::secret_ref(&id, &name))
            .await
            .map_err(|_| {
                error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "清除渠道凭据失败，渠道已停用；可重试删除",
                )
            })?;
    }
    server.db.delete_channel(&id).map_err(failure)?;
    Ok(Json(json!({"deleted":true})))
}

fn wechat_channel(server: &AppServer, id: &str) -> Result<Channel, Error> {
    let channel = get_channel(server, id)?;
    if channel.provider != "wechat_ilink" {
        return Err(error(StatusCode::BAD_REQUEST, "此渠道不使用微信扫码授权"));
    }
    Ok(channel)
}

pub(super) async fn login(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    let gate = server
        .channel_runtime
        .processing_gate(&format!("management:{id}"));
    let _operation = gate.lock().await;
    wechat_channel(&server, &id)?;
    let login = server
        .channel_runtime
        .logins
        .start(server.clone(), &id)
        .await
        .map_err(|e| error(StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(json!({"login":login})))
}

pub(super) async fn login_status(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    wechat_channel(&server, &id)?;
    let login = server
        .channel_runtime
        .logins
        .status(server.clone(), &id)
        .await
        .map_err(failure)?;
    Ok(Json(json!({"login":login})))
}

pub(super) async fn cancel_login(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    let gate = server
        .channel_runtime
        .processing_gate(&format!("management:{id}"));
    let _operation = gate.lock().await;
    let channel = wechat_channel(&server, &id)?;
    server.channel_runtime.logins.cancel(&id).await;
    server
        .db
        .set_channel_status(
            &id,
            if channel.enabled {
                "configured"
            } else {
                "disabled"
            },
            None,
        )
        .map_err(failure)?;
    Ok(Json(
        json!({"login":channel::ChannelLogin {status:"cancelled".into(),..Default::default()}}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VerifyCode {
    code: String,
}
pub(super) async fn verify_login(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<VerifyCode>,
) -> Result<Json<Value>, Error> {
    wechat_channel(&server, &id)?;
    if input.code.is_empty()
        || input.code.len() > 12
        || !input.code.bytes().all(|c| c.is_ascii_digit())
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "请输入手机显示的数字验证码（最多12位）",
        ));
    }
    let login = server
        .channel_runtime
        .logins
        .verify(server.clone(), &id, &input.code)
        .await
        .map_err(|e| error(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(json!({"login":login})))
}

pub(super) async fn regenerate(
    State(server): State<Arc<AppServer>>,
    Path((id, message)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    let channel = get_channel(&server, &id)?;
    if !channel.enabled {
        return Err(error(StatusCode::CONFLICT, "渠道已停用，请先启用"));
    }
    let current = server
        .db
        .channel_message(&id, &message)
        .map_err(failure)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "渠道消息不存在"))?;
    if !channel.allowed_senders.contains(&current.sender_id) {
        return Err(error(StatusCode::FORBIDDEN, "该联系人已不在允许名单内"));
    }
    if !server
        .db
        .regenerate_channel_message(&id, &message)
        .map_err(failure)?
    {
        return Err(error(
            StatusCode::CONFLICT,
            "该消息正在处理、已发送或送达状态未知，不能重新生成",
        ));
    }
    let message = server
        .db
        .channel_message(&id, &message)
        .map_err(failure)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "渠道消息已删除"))?;
    tokio::spawn(channel::process_inbound(server.clone(), message.clone()));
    Ok(Json(json!({"message":message})))
}

pub(super) async fn dismiss(
    State(server): State<Arc<AppServer>>,
    Path((id, message)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    get_channel(&server, &id)?;
    if !server
        .db
        .dismiss_channel_message(&id, &message)
        .map_err(failure)?
    {
        return Err(error(
            StatusCode::CONFLICT,
            "只能放弃尚未发送的已完成草稿或失败消息",
        ));
    }
    Ok(Json(
        json!({"message":server.db.channel_message(&id,&message).map_err(failure)?}),
    ))
}

pub(super) async fn check(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    let mut channel = get_channel(&server, &id)?;
    channel.credentials_configured = credentials_ready(&server, &channel).await?;
    let mut checks = vec![
        json!({"label":"本机凭据","ok":channel.credentials_configured,"detail":if channel.credentials_configured{"所需凭据已存入本机凭据库"}else{"请补全凭据或完成扫码授权"}}),
        json!({"label":"允许联系人","ok":!channel.allowed_senders.is_empty(),"detail":format!("已配置{}位允许联系人",channel.allowed_senders.len())}),
    ];
    if channel.credentials_configured {
        if channel.provider == "feishu"
            || (channel.provider == "onebot11" && channel.connection_mode == "http")
        {
            let result = check_platform(&server, &channel).await;
            checks.push(match result {
                Ok(detail) => json!({"label":"平台账号认证","ok":true,"detail":detail}),
                Err(detail) => json!({"label":"平台账号认证","ok":false,"detail":detail}),
            });
        }
        if channel.connection_mode != "http" {
            let connected =
                channel.enabled && ["connected", "receiving"].contains(&channel.status.as_str());
            checks.push(json!({"label":"运行连接","ok":connected,"detail":if connected{"已观察到平台连接成功"}else if !channel.enabled{"渠道已停用；启用后才能建立连接"}else{"尚未观察到平台连接，请查看渠道状态和最近错误"}}));
        } else {
            checks.push(json!({"label":"事件回调","ok":channel.last_received_at.is_some(),"detail":if channel.last_received_at.is_some(){"已经接收过真实有效回调；不代表此刻公网仍然可达"}else{"尚未收到有效回调；账号认证不能代替回调可达性验证"}}));
        }
    }
    Ok(Json(json!({"checks":checks,"channel":channel})))
}

async fn check_platform(server: &AppServer, channel: &Channel) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法创建平台检查客户端")?;
    if channel.provider == "feishu" {
        let secret = protocol::secret(&server.secret_store, &channel.id, "app_secret").await?;
        let response = client
            .post("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal")
            .json(&json!({"app_id":channel.app_id,"app_secret":secret.expose_secret()}))
            .send()
            .await
            .map_err(|_| "飞书应用认证请求失败")?;
        if !response.status().is_success() {
            return Err(format!("飞书认证HTTP {}", response.status().as_u16()));
        }
        let value: Value = response
            .json()
            .await
            .map_err(|_| "飞书认证响应不是有效JSON")?;
        if value["code"].as_i64() != Some(0)
            || value["tenant_access_token"]
                .as_str()
                .is_none_or(str::is_empty)
        {
            return Err("飞书拒绝了此App ID和App Secret".into());
        }
        Ok("飞书应用凭据通过认证；消息权限和事件订阅仍需在平台完成".into())
    } else {
        let token = protocol::secret(&server.secret_store, &channel.id, "access_token").await?;
        let base = channel
            .api_base_url
            .as_deref()
            .ok_or("缺少OneBot HTTP API地址")?
            .trim_end_matches('/');
        let response = client
            .post(format!("{base}/get_login_info"))
            .bearer_auth(token.expose_secret())
            .json(&json!({}))
            .send()
            .await
            .map_err(|_| "OneBot账号查询失败")?;
        if !response.status().is_success() {
            return Err(format!("OneBot账号查询HTTP {}", response.status().as_u16()));
        }
        let value: Value = response
            .json()
            .await
            .map_err(|_| "OneBot账号响应不是有效JSON")?;
        if value["retcode"].as_i64() != Some(0) || value["status"].as_str() != Some("ok") {
            return Err("OneBot没有确认账号查询成功".into());
        }
        let id = value["data"]["user_id"]
            .as_i64()
            .map(|v| v.to_string())
            .ok_or("OneBot未返回登录账号")?;
        if Some(id.as_str()) != channel.self_id.as_deref() {
            return Err("OneBot实际登录账号与配置self_id不一致".into());
        }
        let status = client
            .post(format!("{base}/get_status"))
            .bearer_auth(token.expose_secret())
            .json(&json!({}))
            .send()
            .await
            .map_err(|_| "OneBot在线状态查询失败")?;
        if !status.status().is_success() {
            return Err(format!("OneBot在线状态HTTP {}", status.status().as_u16()));
        }
        let status: Value = status.json().await.map_err(|_| "OneBot在线状态响应无效")?;
        if status["status"].as_str() != Some("ok")
            || status["retcode"].as_i64() != Some(0)
            || status["data"]["online"].as_bool() != Some(true)
        {
            return Err("OneBot尚未确认账号在线".into());
        }
        Ok(format!("OneBot已确认账号{id}在线；事件上报仍需单独验证"))
    }
}
