// ============================================================
// Safety module unit tests
// ============================================================

use crate::tools::trait_def::RiskLevel;

mod audit;
mod control_session;
mod descriptor;
mod policy_engine;
mod rbac;
mod redaction;
mod registry_coverage;
mod subject_capability;

use super::permission::{PermissionDecision, PermissionManager};
use super::policy::SafetyPolicy;

fn json_str(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or(serde_json::Value::Null)
}

// ── PermissionManager: risk → decision mapping ──

#[test]
fn low_risk_allows() {
    let d = PermissionManager::evaluate("read_file", RiskLevel::Low, &json_str("{}"));
    assert!(matches!(d, PermissionDecision::Allow));
}

#[test]
fn medium_risk_allows() {
    let d = PermissionManager::evaluate("write_file", RiskLevel::Medium, &json_str("{}"));
    assert!(matches!(d, PermissionDecision::Allow));
}

#[test]
fn high_risk_requires_approval() {
    let d = PermissionManager::evaluate("bash", RiskLevel::High, &json_str("{}"));
    match d {
        PermissionDecision::RequireApproval { risk_level, .. } => {
            assert_eq!(risk_level, RiskLevel::High);
        }
        other => panic!("expected RequireApproval, got {:?}", other),
    }
}

#[test]
fn critical_risk_requires_approval() {
    let d = PermissionManager::evaluate("some_tool", RiskLevel::Critical, &json_str("{}"));
    match d {
        PermissionDecision::RequireApproval { risk_level, .. } => {
            assert_eq!(risk_level, RiskLevel::Critical);
        }
        other => panic!("expected RequireApproval, got {:?}", other),
    }
}

// ── SafetyPolicy: shell danger patterns ──

#[test]
fn bash_pwd_stays_at_default() {
    // bash default is High; a benign command must not go below that.
    let risk = SafetyPolicy::assess("bash", RiskLevel::High, &json_str(r#"{"command": "pwd"}"#));
    assert_eq!(risk, RiskLevel::High);
}

#[test]
fn bash_git_push_is_high() {
    let risk = SafetyPolicy::assess(
        "bash",
        RiskLevel::High,
        &json_str(r#"{"command": "git push origin main"}"#),
    );
    assert_eq!(risk, RiskLevel::High);
}

#[test]
fn bash_diskpart_is_critical() {
    let risk = SafetyPolicy::assess(
        "bash",
        RiskLevel::High,
        &json_str(r#"{"command": "diskpart"}"#),
    );
    assert_eq!(risk, RiskLevel::Critical);
}

#[test]
fn bash_remove_item_is_high() {
    let risk = SafetyPolicy::assess(
        "bash",
        RiskLevel::High,
        &json_str(r#"{"command": "Remove-Item -Recurse -Force test-dir"}"#),
    );
    assert_eq!(risk, RiskLevel::High);
}

#[test]
fn bash_case_insensitive_match() {
    let risk = SafetyPolicy::assess(
        "bash",
        RiskLevel::High,
        &json_str(r#"{"command": "GIT PUSH origin"}"#),
    );
    assert_eq!(risk, RiskLevel::High);
}

// ── SafetyPolicy: file path escalation ──

#[test]
fn write_file_user_docs_is_medium() {
    let risk = SafetyPolicy::assess(
        "write_file",
        RiskLevel::Medium,
        &json_str(r#"{"path": "C:\\Users\\user\\Documents\\abc.txt"}"#),
    );
    assert_eq!(risk, RiskLevel::Medium);
}

#[test]
fn write_file_windows_system32_is_high() {
    let risk = SafetyPolicy::assess(
        "write_file",
        RiskLevel::Medium,
        &json_str(r#"{"path": "C:\\Windows\\System32\\abc.txt"}"#),
    );
    assert_eq!(risk, RiskLevel::High);
}

#[test]
fn write_file_etc_is_high() {
    let risk = SafetyPolicy::assess(
        "write_file",
        RiskLevel::Medium,
        &json_str(r#"{"path": "/etc/hosts"}"#),
    );
    assert_eq!(risk, RiskLevel::High);
}

// ── SafetyPolicy: process risk ──

#[test]
fn process_kill_is_high() {
    let risk = SafetyPolicy::assess(
        "process",
        RiskLevel::High,
        &json_str(r#"{"action": "kill", "pid": 1234}"#),
    );
    assert_eq!(risk, RiskLevel::High);
}

#[test]
fn process_list_stays_high() {
    // process default is High; list also stays High per tool default.
    let risk = SafetyPolicy::assess(
        "process",
        RiskLevel::High,
        &json_str(r#"{"action": "list"}"#),
    );
    assert_eq!(risk, RiskLevel::High);
}

// ── ApprovalStore: lifecycle ──

use super::approval::{ApprovalError, ApprovalStatus, ApprovalStore, PendingApproval};

fn make_store() -> ApprovalStore {
    ApprovalStore::new()
}

fn create_pending(store: &ApprovalStore) -> PendingApproval {
    store.create(
        "conv-1".to_string(),
        "call-1".to_string(),
        "bash".to_string(),
        json_str(r#"{"command": "git push origin main"}"#),
        RiskLevel::High,
        "高风险操作".to_string(),
    )
}

#[test]
fn high_tool_creates_pending() {
    let store = make_store();
    let a = create_pending(&store);
    assert_eq!(a.status, ApprovalStatus::Pending);
    assert!(!a.approval_id.is_empty());
    // Original tool call is preserved verbatim.
    assert_eq!(a.tool_name, "bash");
    assert_eq!(a.arguments["command"], "git push origin main");
}

#[test]
fn approve_executes_once_and_second_is_conflict() {
    let store = make_store();
    let a = create_pending(&store);
    let conv = a.conversation_id.clone();
    let id = a.approval_id.clone();

    // First approve succeeds and returns the original payload.
    let approved = store
        .consume_for_approval(&id, &conv)
        .expect("first approve should succeed");
    assert_eq!(approved.status, ApprovalStatus::Approved);
    assert_eq!(approved.tool_call_id, "call-1");

    // Second approve must conflict.
    match store.consume_for_approval(&id, &conv) {
        Err(ApprovalError::AlreadyProcessed) => {}
        other => panic!("expected AlreadyProcessed, got {:?}", other),
    }
}

#[test]
fn reject_never_executes() {
    let store = make_store();
    let a = create_pending(&store);
    let conv = a.conversation_id.clone();
    let id = a.approval_id.clone();

    let rejected = store
        .consume_for_rejection(&id, &conv)
        .expect("reject should succeed");
    assert_eq!(rejected.status, ApprovalStatus::Rejected);

    // Approve after reject is blocked.
    match store.consume_for_approval(&id, &conv) {
        Err(ApprovalError::AlreadyProcessed) => {}
        other => panic!("expected AlreadyProcessed, got {:?}", other),
    }
}

#[test]
fn cancel_blocks_approve() {
    let store = make_store();
    let a = create_pending(&store);
    let conv = a.conversation_id.clone();
    let id = a.approval_id.clone();

    let cancelled = store.cancel(&id, &conv).expect("cancel should succeed");
    assert_eq!(cancelled.status, ApprovalStatus::Cancelled);

    match store.consume_for_approval(&id, &conv) {
        Err(ApprovalError::AlreadyProcessed) => {}
        other => panic!("expected AlreadyProcessed, got {:?}", other),
    }
}

#[test]
fn conversation_mismatch_is_blocked() {
    let store = make_store();
    let a = create_pending(&store);
    let id = a.approval_id.clone();

    match store.consume_for_approval(&id, "other-conv") {
        Err(ApprovalError::ConversationMismatch) => {}
        other => panic!("expected ConversationMismatch, got {:?}", other),
    }
}

#[test]
fn unknown_approval_not_found() {
    let store = make_store();
    match store.consume_for_approval("nope", "conv-1") {
        Err(ApprovalError::NotFound) => {}
        other => panic!("expected NotFound, got {:?}", other),
    }
}

#[test]
fn expired_approval_is_blocked_and_purged() {
    let store = make_store();
    let a = create_pending(&store);
    let id = a.approval_id.clone();
    let conv = a.conversation_id.clone();

    // Rewrite the stored approval to have an already-passed expiry.
    let mut expired = a.clone();
    expired.expires_at = chrono::Utc::now() - chrono::Duration::seconds(1);
    store.insert_for_test(expired);

    match store.consume_for_approval(&id, &conv) {
        Err(ApprovalError::Expired) => {}
        other => panic!("expected Expired, got {:?}", other),
    }

    // It is now marked Expired and visible until purged.
    assert_eq!(store.get(&id).unwrap().status, ApprovalStatus::Expired);
    assert_eq!(store.purge_expired(), 1);
    assert!(store.get(&id).is_none());
}

#[test]
fn pending_for_returns_only_active() {
    let store = make_store();
    let a = create_pending(&store);
    assert_eq!(
        store.pending_for("conv-1").unwrap().approval_id,
        a.approval_id
    );

    // After processing, no active pending remains.
    let _ = store.consume_for_approval(&a.approval_id, &a.conversation_id);
    assert!(store.pending_for("conv-1").is_none());
}

#[test]
fn concurrent_approve_executes_exactly_once() {
    // Proves the approve-once guarantee: even with many concurrent consumers,
    // exactly one transitions Pending → Approved (the tool runs once).
    use std::sync::Arc;
    let store = Arc::new(make_store());
    let a = create_pending(&store);
    let id = a.approval_id.clone();
    let conv = a.conversation_id.clone();

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let store = store.clone();
            let id = id.clone();
            let conv = conv.clone();
            std::thread::spawn(move || store.consume_for_approval(&id, &conv).is_ok())
        })
        .collect();

    let successes = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .filter(|ok| *ok)
        .count();
    assert_eq!(successes, 1);
}
