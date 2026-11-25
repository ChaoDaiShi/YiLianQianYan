use super::*;

#[test]
fn personal_management_readiness_blocks_unconfigured_discovery_without_granting_anything() {
    let f = Fixture::new();
    let mut a = f.save(5);
    for grant in f.server.db.list_grants("local-user").unwrap() {
        if grant.permission == PermissionId::FilesystemRead {
            f.server.db.delete_grant(&grant.id).unwrap();
        }
    }
    if let Some(AutomationTarget::PersonalAssistant {
        allow_file_discovery,
        ..
    }) = &mut a.target
    {
        *allow_file_discovery = true;
    }
    a.revision += 1;
    f.server.db.save_automation(&a, false).unwrap();
    let before = f.server.db.list_grants("local-user").unwrap().len();
    let state = detail(&f.server, &a.id).unwrap();
    assert_eq!(state["actions"]["can_start"], false);
    assert!(state["readiness"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value["code"] == "discovery_permission" && value["severity"] == "error"));
    assert_eq!(f.server.db.list_grants("local-user").unwrap().len(), before);
}

#[test]
fn personal_management_orphaned_approval_blocks_new_work_and_exposes_stop() {
    let f = Fixture::new();
    let a = f.save(5);
    let mut item = f.occurrence(&a, "interrupted");
    let conversation = f.conversation(&a);
    f.server
        .db
        .set_conversation_run_status(
            conversation,
            crate::db::ConversationRunStatus::Interrupted,
            None,
        )
        .unwrap();
    let mut event = AgentEvent::error_event(conversation, String::new());
    event.event_type = "approval_required".into();
    event.error = None;
    event.tool_call_id = Some("orphan-approval".into());
    event.tool_name = Some("write_file".into());
    event.approval_id = Some("orphan-approval-id".into());
    event.args = Some(json!({"path":f.root.join("pending.md"),"content":"not executed"}));
    f.server.db.apply_execution_event(&event).unwrap();
    let state = detail(&f.server, &a.id).unwrap();
    assert_eq!(state["actions"]["can_start"], false);
    assert_eq!(state["actions"]["can_edit"], false);
    assert_eq!(state["actions"]["can_stop"], true);
    assert_eq!(state["readiness"]["can_start"], false);
    assert!(state["readiness"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["code"] == "pending_recovery" && issue["severity"] == "error"));
    assert_eq!(state["pending_approvals"].as_array().unwrap().len(), 1);
    assert!(!f.root.join("pending.md").exists());
    // An ordinary active approval continues through the existing conversation,
    // rather than showing the restart-cleanup instruction.
    item.status = "waiting_approval".into();
    f.server.db.update_automation_occurrence(&item).unwrap();
    f.server
        .db
        .set_conversation_run_status(
            conversation,
            crate::db::ConversationRunStatus::WaitingApproval,
            None,
        )
        .unwrap();
    let active = detail(&f.server, &a.id).unwrap();
    assert!(!active["readiness"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["code"] == "pending_recovery"));
    assert_eq!(active["actions"]["can_stop"], true);
    assert_eq!(active["actions"]["can_start"], false);
    assert_eq!(active["pending_approvals"][0]["approval_status"], "pending");
}

#[test]
fn personal_management_unfinished_tool_and_protocol_require_cleanup_but_not_during_live_work() {
    let f = Fixture::new();
    let a = f.save(5);
    let mut item = f.occurrence(&a, "interrupted");
    let conversation = f.conversation(&a);
    f.server
        .db
        .set_conversation_run_status(
            conversation,
            crate::db::ConversationRunStatus::Interrupted,
            None,
        )
        .unwrap();
    let mut event = AgentEvent::error_event(conversation, String::new());
    event.event_type = "tool_start".into();
    event.error = None;
    event.tool_call_id = Some("uncertain-read".into());
    event.tool_name = Some("read_file".into());
    event.args = Some(json!({"path":f.root.join("notes.txt")}));
    f.server.db.apply_execution_event(&event).unwrap();
    let unfinished = detail(&f.server, &a.id).unwrap();
    assert!(unfinished["readiness"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["code"] == "unfinished_execution"));
    assert_eq!(unfinished["actions"]["can_start"], false);
    assert_eq!(unfinished["actions"]["can_edit"], false);
    assert_eq!(unfinished["actions"]["can_stop"], true);
    f.server.db.add_message(&crate::db::MessageRow{id:"partial-call".into(),conversation_id:conversation.into(),role:"assistant".into(),content:String::new(),
        tool_calls:Some(json!([{"id":"uncertain-read","type":"function","function":{"name":"read_file","arguments":"{}"}}]).to_string()),
        tool_call_id:None,tool_name:None,tool_result:None,created_at:1}).unwrap();
    let broken = detail(&f.server, &a.id).unwrap();
    assert!(broken["readiness"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["code"] == "protocol_recovery" && issue["severity"] == "error"));
    assert_eq!(broken["actions"]["can_stop"], true);
    assert!(!f.root.join("notes.txt").exists());
    // A tool legitimately in progress is not diagnosed as a restart orphan.
    item.status = "running".into();
    f.server.db.update_automation_occurrence(&item).unwrap();
    f.server
        .db
        .set_conversation_run_status(
            conversation,
            crate::db::ConversationRunStatus::Running,
            None,
        )
        .unwrap();
    let live = detail(&f.server, &a.id).unwrap();
    assert!(!live["readiness"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| matches!(
            issue["code"].as_str(),
            Some("protocol_recovery" | "unfinished_execution")
        )));
    // Only settling both persisted facts clears the recovery gate. This test
    // records an uncertain outcome; it never dispatches the original tool.
    item.status = "interrupted".into();
    f.server.db.update_automation_occurrence(&item).unwrap();
    f.server
        .db
        .set_conversation_run_status(
            conversation,
            crate::db::ConversationRunStatus::Interrupted,
            None,
        )
        .unwrap();
    event.event_type = "tool_end".into();
    event.status = Some("error".into());
    event.result = Some("结果不确定，需人工核实".into());
    f.server.db.apply_execution_event(&event).unwrap();
    f.server
        .db
        .add_message(&crate::db::MessageRow {
            id: "uncertain-result".into(),
            conversation_id: conversation.into(),
            role: "tool".into(),
            content: "结果不确定，需人工核实".into(),
            tool_calls: None,
            tool_call_id: Some("uncertain-read".into()),
            tool_name: Some("read_file".into()),
            tool_result: None,
            created_at: 2,
        })
        .unwrap();
    let settled = detail(&f.server, &a.id).unwrap();
    assert!(!settled["readiness"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| matches!(
            issue["code"].as_str(),
            Some("protocol_recovery" | "unfinished_execution")
        )));
    assert_eq!(settled["actions"]["can_edit"], true);
    assert_eq!(settled["actions"]["requires_review"], true);
}
