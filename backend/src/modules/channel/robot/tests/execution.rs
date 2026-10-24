use super::*;
use crate::modules::channel::{protocol, receive};

#[tokio::test]
async fn channel_robot_desktop_send_replays_receipt_without_sending_again() {
    let server = server();
    let mut channel = configured(&server);
    let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured = calls.clone();
    let router=Router::new().route("/send_private_msg",post(move|Json(body):Json<Value>|{let captured=captured.clone();async move{
        let unknown=body.pointer("/message/0/data/text").and_then(Value::as_str)==Some("unknown");captured.lock().unwrap().push(body);
        Json(json!({"status":"ok","retcode":0,"data":if unknown{json!({})}else{json!({"message_id":901})}}))
    }}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    channel.api_base_url = Some(format!("http://{}", listener.local_addr().unwrap()));
    let service = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    server.db.save_channel(&channel).unwrap();
    server
        .secret_store
        .put(
            &protocol::secret_ref(&channel.id, "access_token"),
            secrecy::SecretString::from("controlled-fixture".to_string()),
        )
        .await
        .unwrap();
    let request = uuid::Uuid::new_v4().to_string();
    let first = send_text(
        &server,
        &channel,
        None,
        "desktop_send",
        "private",
        "300",
        "hello",
        true,
        Some(&request),
    )
    .await
    .unwrap();
    let replay = send_text(
        &server,
        &channel,
        None,
        "desktop_send",
        "private",
        "300",
        "hello",
        true,
        Some(&request),
    )
    .await
    .unwrap();
    assert_eq!(first.id, replay.id);
    assert_eq!(replay.status, "sent");
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert!(send_text(
        &server,
        &channel,
        None,
        "desktop_send",
        "private",
        "300",
        "changed",
        true,
        Some(&request)
    )
    .await
    .is_err());
    let request = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        send_text(
            &server,
            &channel,
            None,
            "desktop_send",
            "private",
            "300",
            "unknown",
            true,
            Some(&request)
        )
        .await
        .unwrap()
        .status,
        "delivery_unknown"
    );
    assert_eq!(
        send_text(
            &server,
            &channel,
            None,
            "desktop_send",
            "private",
            "300",
            "unknown",
            true,
            Some(&request)
        )
        .await
        .unwrap()
        .status,
        "delivery_unknown"
    );
    assert_eq!(calls.lock().unwrap().len(), 2);
    let mut changed = channel.clone();
    changed.self_id = Some("101".into());
    server.db.save_channel(&changed).unwrap();
    assert!(send_text(
        &server,
        &channel,
        None,
        "desktop_send",
        "private",
        "300",
        "stale account",
        true,
        Some(&uuid::Uuid::new_v4().to_string())
    )
    .await
    .is_err());
    assert_eq!(calls.lock().unwrap().len(), 2);
    service.abort();
}

#[tokio::test]
async fn channel_robot_stop_reconciles_inflight_send_as_unknown_without_retry() {
    let server = server();
    let mut channel = configured(&server);
    let entered = Arc::new(tokio::sync::Notify::new());
    let seen = entered.clone();
    let pending = Arc::new(tokio::sync::Notify::new());
    let pending_send = pending.clone();
    let router=Router::new().route("/chat/completions",post(||async{
        let tool=json!({"id":"cancel-fixture","choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"inflight-send","type":"function","function":{"name":"qq_send_private","arguments":"{\"user_id\":\"300\",\"text\":\"cancel fixture\"}"}}]},"finish_reason":null}]}).to_string();
        let end=json!({"id":"cancel-fixture","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}).to_string();
        ([("content-type","text/event-stream")],format!("data: {tool}\n\ndata: {end}\n\ndata: [DONE]\n\n"))
    })).route("/send_private_msg",post(move||{let seen=seen.clone();let pending=pending_send.clone();async move{
        seen.notify_one();pending.notified().await;Json(json!({"status":"ok","retcode":0,"data":{"message_id":999}}))
    }}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let service = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    channel.api_base_url = Some(origin.clone());
    server.db.save_channel(&channel).unwrap();
    let key = SecretRef::new("robot.inflight.fixture");
    for reference in [&key, &protocol::secret_ref(&channel.id, "access_token")] {
        server
            .secret_store
            .put(
                reference,
                secrecy::SecretString::from("controlled-fixture".to_string()),
            )
            .await
            .unwrap();
    }
    {
        let mut config = server.config.write();
        config.model.base_url = origin;
        config.model.name = "fixture-model".into();
        config.model.provider = "openai".into();
        config.model.api_key_ref = Some(key);
        config.model.api_key_env = String::new();
    }
    let mission = RobotMission {
        id: "cancel-mission".into(),
        channel_id: channel.id.clone(),
        name: "cancel".into(),
        goal: "向300发送受控测试".into(),
        preset_id: None,
        workspace_id: None,
        created_at: 1,
        updated_at: 1,
    };
    server.db.save_robot_mission(&mission).unwrap();
    let run = start_mission(server.clone(), mission, "desktop")
        .await
        .unwrap();
    if tokio::time::timeout(std::time::Duration::from_secs(15), entered.notified())
        .await
        .is_err()
    {
        panic!(
            "send never started: run={:?}, actions={:?}, approvals={:?}",
            server.db.robot_run(&run.id).unwrap(),
            server.db.robot_actions(&run.id).unwrap(),
            server
                .approval_store
                .list_pending_for_conversation(&run.conversation_id)
        );
    }
    tokio::time::timeout(std::time::Duration::from_secs(5), stop(&server, &run.id))
        .await
        .unwrap()
        .unwrap();
    let actions = server.db.robot_actions(&run.id).unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].status, "delivery_unknown");
    assert!(server
        .db
        .robot_ambiguous_send(&run.id, "private", "300")
        .unwrap());
    assert_eq!(
        server.db.robot_run(&run.id).unwrap().unwrap().status,
        "stopped"
    );
    pending.notify_waiters();
    service.abort();
}
#[tokio::test]
async fn channel_robot_stopped_queued_run_never_starts_a_model_turn() {
    let server = server();
    let channel = configured(&server);
    let conversation = server.db.create_conversation("queued robot").unwrap();
    let run = RobotRun {
        account_id: "100".into(),
        id: "queued-stop".into(),
        channel_id: channel.id,
        mission_id: None,
        conversation_id: conversation.id.clone(),
        user_message_id: None,
        origin: "desktop".into(),
        owner_id: None,
        goal: "do not start".into(),
        preset_id: None,
        status: "queued".into(),
        answer: None,
        error: None,
        created_at: 1,
        updated_at: 1,
    };
    server.db.create_robot_run(&run, None).unwrap();
    stop(&server, &run.id).await.unwrap();
    execute(server.clone(), run.clone()).await.unwrap();
    assert_eq!(
        server.db.robot_run(&run.id).unwrap().unwrap().status,
        "stopped"
    );
    assert!(server
        .db
        .get_conversation(&conversation.id)
        .unwrap()
        .messages
        .is_empty());
}

#[tokio::test]
async fn channel_robot_automation_executes_agent_turn_and_cancels_that_turn() {
    use crate::modules::automation::{
        self,
        model::{AutomationInput, AutomationTarget, Schedule},
    };
    let server = server();
    let channel = configured(&server);
    let entered = Arc::new(tokio::sync::Notify::new());
    let notified = entered.clone();
    let release = Arc::new(tokio::sync::Notify::new());
    let pending = release.clone();
    let router = Router::new().route(
        "/chat/completions",
        post(move |Json(body): Json<Value>| {
            let notified = notified.clone();
            let pending = pending.clone();
            async move {
                assert!(body["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|t| t["function"]["name"] == "qq_friends"));
                notified.notify_one();
                pending.notified().await;
                ([("content-type", "text/event-stream")], "data: [DONE]\n\n")
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let service = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let key = SecretRef::new("robot.schedule.fixture");
    server
        .secret_store
        .put(
            &key,
            secrecy::SecretString::from("controlled-fixture".to_string()),
        )
        .await
        .unwrap();
    {
        let mut config = server.config.write();
        config.model.base_url = origin;
        config.model.name = "fixture-model".into();
        config.model.provider = "openai".into();
        config.model.api_key_ref = Some(key);
        config.model.api_key_env = String::new();
    }
    let workspace = crate::workspace::Workspace::new(
        crate::workspace::WorkspaceId::generate(),
        "scheduled QQ".into(),
        String::new(),
        None,
        1,
    )
    .unwrap();
    server.db.create_workspace(&workspace).unwrap();
    let mission = RobotMission {
        id: "scheduled-mission".into(),
        channel_id: channel.id.clone(),
        name: "explore QQ".into(),
        goal: "请探索好友".into(),
        preset_id: None,
        workspace_id: None,
        created_at: 1,
        updated_at: 1,
    };
    server.db.save_robot_mission(&mission).unwrap();
    let schedule = automation::save(
        &server,
        None,
        AutomationInput {
            expected_revision: None,
            name: "scheduled goal".into(),
            workflow_graph_id: String::new(),
            target: Some(AutomationTarget::AgentTurn {
                mission_id: mission.id,
            }),
            workspace_id: workspace.id.to_string(),
            timezone: "Asia/Shanghai".into(),
            schedule: Schedule::Daily {
                time: "09:00".into(),
            },
            enabled: true,
        },
    )
    .unwrap();
    let item = automation::run_now(&server, &schedule.id).await.unwrap();
    assert!(item.run_id.is_none());
    let run_id = item.robot_run_id.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(15), entered.notified())
        .await
        .unwrap();
    let run = stop(&server, &run_id).await.unwrap();
    assert_eq!(run.status, "stopped");
    let history = automation::history(&server, &schedule.id).unwrap();
    assert_eq!(history[0].status, "stopped");
    assert_eq!(
        history[0].conversation_id.as_deref(),
        Some(run.conversation_id.as_str())
    );
    assert_eq!(
        server
            .db
            .conversation_workspace(&run.conversation_id)
            .unwrap(),
        Some(workspace.id.to_string())
    );
    assert_eq!(
        server.db.conversation_origins().unwrap()[&run.conversation_id]["kind"],
        "channel"
    );
    release.notify_waiters();
    service.abort();
}

#[tokio::test]
async fn channel_robot_owner_uses_qq_tools_and_reports_once_without_passive_reply() {
    let server = server();
    let mut channel = configured(&server);
    let requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let capture = requests.clone();
    let sends = Arc::new(Mutex::new(Vec::<Value>::new()));
    let sent = sends.clone();
    let router=Router::new().route("/chat/completions",post(move|Json(body):Json<Value>|{
        let capture=capture.clone();async move{
            let mut requests=capture.lock().unwrap();let index=requests.len();requests.push(body);drop(requests);
            let delta=if index%2==0 {json!({"role":"assistant","tool_calls":[{"index":0,"id":"friends-call","type":"function","function":{"name":"qq_friends","arguments":"{}"}}]})}else{json!({"content":"好友探索完成"})};
            let finish=if index%2==0{"tool_calls"}else{"stop"};
            let chunk=json!({"id":"robot-fixture","choices":[{"index":0,"delta":delta,"finish_reason":null}]}).to_string();
            let done=json!({"id":"robot-fixture","choices":[{"index":0,"delta":{},"finish_reason":finish}]}).to_string();
            ([("content-type","text/event-stream")],format!("data: {chunk}\n\ndata: {done}\n\ndata: [DONE]\n\n"))
        }
    })).route("/get_friend_list",post(||async{Json(json!({"status":"ok","retcode":0,"data":[{"user_id":300,"nickname":"受控联系人"}]}))}))
    .route("/send_private_msg",post(move|Json(body):Json<Value>|{let sent=sent.clone();async move{sent.lock().unwrap().push(body);Json(json!({"status":"ok","retcode":0,"data":{"message_id":701}}))}}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let service = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    channel.api_base_url = Some(origin.clone());
    channel.reply_mode = "automatic".into();
    server.db.save_channel(&channel).unwrap();
    let key = SecretRef::new("robot.fixture.model");
    for reference in [&key, &protocol::secret_ref(&channel.id, "access_token")] {
        server
            .secret_store
            .put(
                reference,
                secrecy::SecretString::from("controlled-fixture".to_string()),
            )
            .await
            .unwrap();
    }
    {
        let mut config = server.config.write();
        config.model.base_url = origin;
        config.model.name = "fixture-model".into();
        config.model.provider = "openai".into();
        config.model.api_key_ref = Some(key);
        config.model.api_key_env = String::new();
    }
    let inbound = InboundMessage {
        event_id: "owner-event".into(),
        external_message_id: "77".into(),
        peer_type: "private".into(),
        peer_id: "200".into(),
        sender_id: "200".into(),
        content: "请查看好友名单并告诉我结果".into(),
        context_token: None,
    };
    assert!(receive(server.clone(), &channel, inbound).await.unwrap());
    let run = server
        .db
        .robot_run_for_event(&channel.id, "owner-event")
        .unwrap()
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let run = server.db.robot_run(&run.id).unwrap().unwrap();
            if !run.active() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let run = server.db.robot_run(&run.id).unwrap().unwrap();
    assert_eq!(run.status, "completed", "{:?}", run.error);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while sends.lock().unwrap().is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(sends.lock().unwrap().len(), 1);
    assert_eq!(sends.lock().unwrap()[0]["user_id"], 200);
    assert!(server.db.channel_messages(&channel.id).unwrap().is_empty());
    let captured = requests.lock().unwrap();
    let names = captured[0]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(names.contains(&"qq_friends"));
    assert!(!names.contains(&"bash"));
    assert!(!names.contains(&"read_file"));
    assert!(captured.len() >= 2);
    drop(captured);
    assert!(server
        .db
        .robot_actions(&run.id)
        .unwrap()
        .iter()
        .any(|a| a.id == format!("robot:{}:friends-call", run.id)));
    let next = InboundMessage {
        event_id: "owner-next-event".into(),
        external_message_id: "78".into(),
        peer_type: "private".into(),
        peer_id: "200".into(),
        sender_id: "200".into(),
        content: "再查一次好友名单".into(),
        context_token: None,
    };
    assert!(receive(server.clone(), &channel, next).await.unwrap());
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let next = server
                .db
                .robot_run_for_event(&channel.id, "owner-next-event")
                .unwrap()
                .unwrap();
            if next.status == "failed" {
                panic!("second run failed: {:?}", next.error);
            }
            if next.status == "completed" && sends.lock().unwrap().len() == 2 {
                assert_ne!(run.id, next.id);
                assert!(server
                    .db
                    .robot_actions(&next.id)
                    .unwrap()
                    .iter()
                    .any(|a| a.id == format!("robot:{}:friends-call", next.id)));
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(requests.lock().unwrap().len(), 4);
    assert!(server.db.channel_messages(&channel.id).unwrap().is_empty());
    service.abort();
}
