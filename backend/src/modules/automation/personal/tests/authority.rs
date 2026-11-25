use super::*;

#[tokio::test]
async fn personal_assistant_selected_model_cannot_expand_output_cap() {
    let f = Fixture::new();
    let a = f.save(false, 5);
    let _item = claim(&f, &a);
    let row = f
        .server
        .db
        .create_llm_model(&crate::db::LlmModelInput {
            id: "large-model-fixture".into(),
            provider: "openai".into(),
            label: "Local metadata fixture".into(),
            model: "controlled".into(),
            base_url: "http://127.0.0.1:12345".into(),
            api_format: "openai".into(),
            api_key_ref: "personal.controlled.fixture".into(),
            api_key_env: String::new(),
            temperature: 0.0,
            max_tokens: 16000,
            invoke_timeout_ms: 10000,
        })
        .unwrap();
    let mut runtime = crate::api::chat::runtime::prepare(&f.server, Some(conversation(&a)), None)
        .await
        .unwrap();
    let options = crate::api::chat::options::ChatRequestOptions {
        model_profile_id: Some(row.id),
        ..Default::default()
    };
    options.apply(&f.server, &mut runtime).unwrap();
    assert_eq!(runtime.config.model.max_tokens, 2048);
    assert!(runtime.execution_scope.is_some());
    assert!(runtime.tools.get("bash").is_none());
}

#[tokio::test]
async fn personal_assistant_legacy_branch_ancestry_cannot_restore_unrestricted_runtime() {
    let f = Fixture::new();
    let a = f.save(false, 5);
    f.server
        .db
        .persist_chat_message(
            conversation(&a),
            &crate::integrations::llm::types::ChatMessage {
                role: "assistant".into(),
                content: Some("Legacy answer".into()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
            1,
        )
        .unwrap();
    let answer = f
        .server
        .db
        .get_conversation(conversation(&a))
        .unwrap()
        .messages[0]
        .id
        .clone();
    // Reproduce an older version's branch, then restore the original ownership
    // metadata. Only this isolated fixture database is changed.
    let key = format!("personal_assistant:{}", conversation(&a));
    f.server
        .db
        .conn()
        .execute("DELETE FROM settings WHERE key=?1", [&key])
        .unwrap();
    let branch = f
        .server
        .db
        .fork_message_canvas_conversation(conversation(&a), &answer)
        .unwrap();
    f.server
        .db
        .conn()
        .execute(
            "INSERT INTO settings(key,value) VALUES(?1,?2)",
            params![key, a.id],
        )
        .unwrap();
    assert!(is_delegated(&f.server, &branch).unwrap());
    assert!(
        crate::api::chat::runtime::prepare(&f.server, Some(&branch), None)
            .await
            .is_err()
    );
    assert!(f
        .server
        .db
        .fork_message_canvas_conversation(&branch, "any-answer")
        .is_err());
    let ordinary = f.server.db.create_conversation("Independent").unwrap();
    assert!(!is_delegated(&f.server, &ordinary.id).unwrap());
}

#[tokio::test]
async fn personal_assistant_scopes_files_blocks_overwrite_and_revokes_old_snapshots() {
    let f = Fixture::new();
    let a = f.save(true, 5);
    let item = claim(&f, &a);
    assert_ne!(conversation(&a), "forged-conversation");
    let root = std::fs::canonicalize(&f.root).unwrap();
    let registry = Arc::new(
        scoped_tools::registry(&f.server, &a, &item.id, root.to_str().unwrap(), true).unwrap(),
    );
    assert!(registry.get("bash").is_none());
    assert!(registry.get("http_request").is_none());
    assert!(registry.get("keyboard").is_none());
    assert!(registry
        .get("read_file")
        .unwrap()
        .security_descriptor(&json!({"path":root.join("..").join("outside.txt")}))
        .is_err());
    assert!(registry
        .get("read_file")
        .unwrap()
        .security_descriptor(&json!({"path":"relative.txt"}))
        .is_err());
    let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
        f.server.config.read().sandbox.clone(),
        f.root.to_str().unwrap(),
        registry.clone(),
    )
    .with_db(Arc::new(f.server.db.clone_connection()));
    let path = root.join("report.md");
    let request = SecurityExecutionRequest {
        conversation_id: conversation(&a).into(),
        tool_call_id: "first-write".into(),
        tool_name: "write_file".into(),
        arguments: json!({"path":path,"content":"verified local report"}),
        subject: SecuritySubject::local_user(),
    };
    let result = gateway
        .execute_approved(&request, RiskLevel::High)
        .await
        .unwrap();
    assert!(
        matches!(result, crate::safety::execution_gateway::SecurityExecutionOutcome::Executed { tool_result, .. } if tool_result.ok)
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "verified local report"
    );
    let result = gateway
        .execute_approved(&request, RiskLevel::High)
        .await
        .unwrap();
    assert!(
        matches!(result, crate::safety::execution_gateway::SecurityExecutionOutcome::Executed { tool_result, .. } if !tool_result.ok)
    );
    stop(&f.server, &a.id).await.unwrap();
    assert!(registry
        .get("read_file")
        .unwrap()
        .security_descriptor(&json!({"path":path}))
        .is_err());
    assert!(!f.server.db.get_automation(&a.id).unwrap().unwrap().enabled);
    assert_eq!(attempts(&f.server, &a.id).unwrap()[0].status, "cancelled");
    f.server.db.delete_automation(&a.id).unwrap();
    assert!(
        crate::api::chat::runtime::prepare(&f.server, Some(conversation(&a)), None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn personal_assistant_claims_once_waits_and_counts_budget_beyond_history_page() {
    let f = Fixture::new();
    let a = f.save(false, 1);
    let mut item = occurrence(&a, a.next_run_at.unwrap(), false, false);
    assert!(f
        .server
        .db
        .claim_automation_occurrence(&a, None, &mut item, false)
        .unwrap());
    assert!(!f
        .server
        .db
        .claim_automation_occurrence(&a, None, &mut item, false)
        .unwrap());
    item.conversation_id = Some(conversation(&a).into());
    item.status = "waiting_input".into();
    item.run_id = None;
    f.server.db.update_automation_occurrence(&item).unwrap();
    let current = f.server.db.get_automation(&a.id).unwrap().unwrap();
    assert_eq!(
        super::super::super::run_now(&f.server, &a.id)
            .await
            .unwrap()
            .status,
        "skipped"
    );
    mutate(&f.server, &item.id, |item| item.status = "failed".into()).unwrap();
    // A long list of skipped triggers must not erase budget or the last failure.
    for _ in 0..105 {
        let mut skip = occurrence(&current, 1, true, false);
        skip.status = "skipped".into();
        assert!(f
            .server
            .db
            .claim_automation_occurrence(&current, None, &mut skip, true)
            .unwrap());
    }
    let result = super::super::super::run_now(&f.server, &a.id)
        .await
        .unwrap();
    assert_eq!(result.status, "blocked");
    assert!(result.error.unwrap().contains("预算"));
    assert_eq!(activity(&f.server, &a.id).unwrap()["used_runs"], 1);
}

#[tokio::test]
async fn personal_assistant_real_chat_executor_reads_writes_verifies_reports_and_retains_context() {
    let f = Fixture::new();
    std::fs::write(f.root.join("notes.txt"), "Controlled notes").unwrap();
    let a = f.save(true, 2);
    let (service, calls) = model(&f, 0).await;
    super::super::super::run_now(&f.server, &a.id)
        .await
        .unwrap();
    let item = wait_status(&f, &a, "completed").await;
    assert!(item.report.unwrap().contains("核验"));
    assert!(item.assistant_turn_id.is_some());
    assert_eq!(
        std::fs::read_to_string(f.root.join("report.md")).unwrap(),
        "# Report\nControlled notes summarized.\n"
    );
    let messages = f
        .server
        .db
        .get_conversation(conversation(&a))
        .unwrap()
        .messages
        .len();
    super::super::super::run_now(&f.server, &a.id)
        .await
        .unwrap();
    wait_status(&f, &a, "completed").await;
    assert!(
        f.server
            .db
            .get_conversation(conversation(&a))
            .unwrap()
            .messages
            .len()
            > messages
    );
    assert_eq!(activity(&f.server, &a.id).unwrap()["used_runs"], 2);
    assert_eq!(
        super::super::super::run_now(&f.server, &a.id)
            .await
            .unwrap()
            .status,
        "blocked"
    );
    assert!(calls.load(Ordering::SeqCst) >= 5);
    service.abort();
}
