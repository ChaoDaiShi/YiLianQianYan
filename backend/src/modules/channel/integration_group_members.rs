use super::*;

#[tokio::test]
async fn group_member_directory_prefers_cards_and_checks_read_scope() {
    let server = server();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let capture = calls.clone();
    let router = Router::new()
        .route(
            "/get_group_list",
            post(|| async { Json(json!({"status":"ok","retcode":0,"data":[{"group_id":3001}]})) }),
        )
        .route(
            "/get_group_member_list",
            post(move |Json(input): Json<Value>| {
                let capture = capture.clone();
                async move {
                    capture.lock().unwrap().push(input);
                    Json(json!({"status":"ok","retcode":0,"data":[
                        {"group_id":3001,"user_id":2001,"card":" 活动负责人 ","nickname":"小雨"},
                        {"group_id":3001,"user_id":2002,"card":"  ","nickname":"阿木"},
                        {"group_id":3001,"user_id":2003,"card":"","nickname":""}
                    ]}))
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut channel = super::super::tests::channel("onebot11");
    channel.api_base_url = Some(format!("http://{}", listener.local_addr().unwrap()));
    let service = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    server.db.save_channel(&channel).unwrap();
    server
        .secret_store
        .put(
            &protocol::secret_ref(&channel.id, "access_token"),
            secrecy::SecretString::from("fixture-token".to_string()),
        )
        .await
        .unwrap();

    let path = "/api/channels/channel/robot/directory?kind=members&group_id=3001";
    let (status, data) = call(&server, "GET", path, None, true).await;
    assert_eq!(status, StatusCode::OK, "{data}");
    assert_eq!(
        data["items"],
        json!([
            {"id":"2001","name":"活动负责人","peer_type":"private"},
            {"id":"2002","name":"阿木","peer_type":"private"},
            {"id":"2003","name":"2003","peer_type":"private"}
        ])
    );

    let unknown = path.replace("3001", "3002");
    assert_eq!(
        call(&server, "GET", &unknown, None, true).await.0,
        StatusCode::BAD_REQUEST
    );
    let mut policy = server.db.robot_policy(&channel.id).unwrap();
    policy.read_groups = false;
    server.db.save_robot_policy(&policy).unwrap();
    assert_eq!(
        call(&server, "GET", path, None, true).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(*calls.lock().unwrap(), vec![json!({"group_id":3001})]);
    service.abort();
}
