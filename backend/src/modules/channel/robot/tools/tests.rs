use super::*;
use crate::modules::channel::robot::types::RobotPolicy;

fn bound_send_tool() -> QqTool {
    QqTool {
        server: Weak::new(),
        channel_id: "channel-bound".into(),
        run_id: "run-bound".into(),
        account_id: "100".into(),
        name: "qq_send_private",
        description: "fixture",
    }
}

#[test]
fn send_tool_action_and_channel_are_server_bound() {
    let tool = bound_send_tool();
    let args = json!({
        "user_id": "200",
        "text": "hello",
        "action": "delete_friend",
        "channel_id": "attacker-selected"
    });

    let details = tool.details(&args).unwrap();
    let descriptor = tool.descriptor(&args).unwrap();

    assert_eq!(details.action, "send_private_msg");
    assert_eq!(details.peer_id, "200");
    assert!(matches!(
        descriptor.resources.as_slice(),
        [ResourceDescriptor::ChannelMessage {
            channel_id,
            account_id,
            run_id,
            action,
            peer_type,
            peer_id,
        }] if channel_id == "channel-bound"
            && account_id == "100"
            && run_id == "run-bound"
            && action == "send_private_msg"
            && peer_type == "private"
            && peer_id == "200"
    ));
}

#[test]
fn history_count_is_bounded_to_one_through_one_hundred() {
    assert_eq!(history_count(&json!({ "count": 1 })).unwrap(), 1);
    assert_eq!(history_count(&json!({ "count": 100 })).unwrap(), 100);
    assert!(history_count(&json!({ "count": 0 })).is_err());
    assert!(history_count(&json!({ "count": 101 })).is_err());
    assert!(history_count(&json!({ "count": "10" })).is_err());
}

#[test]
fn message_lookup_accepts_signed_onebot_ids_and_requires_both_read_scopes() {
    let mut tool = bound_send_tool();
    tool.name = "qq_message";
    let details = tool.details(&json!({ "message_id": "-12345" })).unwrap();
    assert_eq!(details.action, "get_msg");
    assert_eq!(details.params["message_id"], -12345);

    let mut policy = RobotPolicy::new("channel-bound");
    policy.read_friends = true;
    policy.read_groups = false;
    assert!(!policy.allows_tool_read("qq_message"));
    policy.read_groups = true;
    assert!(policy.allows_tool_read("qq_message"));
}

#[test]
fn directory_membership_uses_platform_friend_and_group_ids() {
    let friends = vec![json!({ "user_id": 200 }), json!({ "user_id": "201" })];
    let groups = vec![json!({ "group_id": 300 }), json!({ "group_id": "301" })];

    assert!(directory_contains(&friends, "user_id", 200));
    assert!(directory_contains(&friends, "user_id", 201));
    assert!(!directory_contains(&friends, "user_id", 300));
    assert!(directory_contains(&groups, "group_id", 300));
    assert!(!directory_contains(&groups, "group_id", 200));
}

#[test]
fn send_all_requires_a_current_member_but_explicit_targets_do_not() {
    let mut policy = RobotPolicy::new("channel-bound");
    policy.enabled = true;
    policy.send_all_private = true;
    let friends = vec![json!({ "user_id": 200 })];

    assert!(send_all_requires_membership_check(
        &policy, "private", "200"
    ));
    assert!(directory_contains(&friends, "user_id", 200));
    assert!(!directory_contains(&friends, "user_id", 201));
    assert!(send_all_target_valid(&policy, "private", "200", true));
    assert!(!send_all_target_valid(&policy, "private", "201", false));

    policy.send_private_ids.push("201".into());
    assert!(!send_all_requires_membership_check(
        &policy, "private", "201"
    ));
    assert!(send_all_target_valid(&policy, "private", "201", false));
    policy.send_mode = "manual".into();
    assert!(!send_all_requires_membership_check(
        &policy, "private", "200"
    ));
}

#[test]
fn message_details_require_a_succeeded_history_record_for_the_same_peer() {
    let history = RobotAction {
        account_id: "100".into(),
        id: "history-call".into(),
        run_id: Some("run-1".into()),
        channel_id: "channel-1".into(),
        tool_name: "qq_group_history".into(),
        peer_type: Some("group".into()),
        peer_id: Some("300".into()),
        arguments: json!({ "group_id": "300", "count": 20 }),
        result: Some(json!({ "messages": [
            { "message_id": -77, "message_type": "group", "group_id": 300 },
            { "message_id": 88, "message_type": "group", "group_id": 300 }
        ] })),
        status: "succeeded".into(),
        platform_message_id: None,
        error: None,
        created_at: 0,
        updated_at: 0,
    };

    let provenance = message_provenance(&[history.clone()], "channel-1", -77).unwrap();
    assert_eq!(provenance.peer_type, "group");
    assert_eq!(provenance.peer_id, "300");
    assert!(message_matches_provenance(
        &json!({ "message_id": -77, "message_type": "group", "group_id": 300 }),
        -77,
        &provenance,
    ));
    assert!(!message_matches_provenance(
        &json!({ "message_id": -77, "message_type": "private", "user_id": 300 }),
        -77,
        &provenance,
    ));

    let mut failed = history;
    failed.status = "failed".into();
    assert!(message_provenance(&[failed], "channel-1", -77).is_none());
}

#[test]
fn read_and_send_policy_are_separate_and_target_specific() {
    let mut policy = RobotPolicy::new("channel-bound");
    policy.enabled = true;
    policy.send_private_ids.push("200".into());
    assert!(policy.allows_send("private", "200"));
    assert!(!policy.allows_send("private", "201"));
    assert!(!policy.allows_send("group", "200"));
    assert!(policy.read_friends);

    policy.send_mode = "manual".into();
    assert!(!policy.allows_send("private", "200"));
    assert!(policy.allows_tool_read("qq_private_history"));
    policy.read_groups = false;
    assert!(!policy.allows_tool_read("qq_group_history"));

    policy.enabled = false;
    assert!(!policy.allows_send("private", "200"));
}

#[tokio::test]
async fn context_free_qq_send_does_not_dispatch() {
    let tool = bound_send_tool();
    let result = tool
        .execute(json!({ "user_id": "200", "text": "hello" }))
        .await;
    assert!(!result.ok);
    assert!(result
        .content
        .contains("trusted SecurityExecutionGateway context"));
}

#[test]
fn ambiguous_send_result_preserves_raw_data_and_forbids_retry() {
    let data = json!({ "accepted": true, "note": "no message id" });
    let result = unknown_send_result("delivery is unknown; do not retry", Some(data.clone()));

    assert!(result.ok);
    assert_eq!(
        result.structured.as_ref().unwrap()["status"],
        "delivery_unknown"
    );
    assert_eq!(result.structured.as_ref().unwrap()["retry_allowed"], false);
    assert_eq!(result.structured.as_ref().unwrap()["data"], data);
}
