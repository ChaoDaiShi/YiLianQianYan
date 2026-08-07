// ============================================================
// Safety module unit tests
// ============================================================

use crate::tools::trait_def::RiskLevel;

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
