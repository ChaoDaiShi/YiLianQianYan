// ============================================================
// SafetyPolicy — assess the risk of a tool call from its
// tool name, default risk, and arguments.
//
// Fail-closed: pattern detection is only a first guardrail.
// ============================================================

use crate::tools::trait_def::RiskLevel;

/// Commands that elevate a shell command to HIGH risk.
const HIGH_RISK_SHELL_PATTERNS: &[&str] = &[
    "rm -rf",
    "remove-item",
    "del /f",
    "format ",
    "shutdown",
    "restart-computer",
    "git push",
    "git reset --hard",
    "git clean -fd",
    "taskkill",
    "stop-process",
    "reg delete",
];

/// Commands that elevate a shell command to CRITICAL risk.
const CRITICAL_SHELL_PATTERNS: &[&str] = &["diskpart", "bcdedit", "cipher /w"];

/// Directories considered sensitive for file writes/edits.
/// Matches on the lowercase path prefix.
const SENSITIVE_DIRS: &[&str] = &[
    "c:\\windows\\",
    "c:\\program files\\",
    "c:\\programdata\\",
    "system32",
    "/etc/",
    "/usr/",
    "/bin/",
    "/sbin/",
    "/system/",
];

pub struct SafetyPolicy;

impl SafetyPolicy {
    /// Compute the final risk level for a tool call:
    /// `max(default_risk, argument_risk)`.
    pub fn assess(tool_name: &str, default_risk: RiskLevel, args: &serde_json::Value) -> RiskLevel {
        let dynamic_risk = Self::assess_arguments(tool_name, args);
        std::cmp::max(default_risk, dynamic_risk)
    }

    /// Argument-based risk assessment, dispatched by tool name.
    fn assess_arguments(tool_name: &str, args: &serde_json::Value) -> RiskLevel {
        match tool_name {
            "bash" => Self::assess_shell(args),
            "process" => Self::assess_process(args),
            "write_file" | "edit_file" => Self::assess_file_write(args),
            _ => RiskLevel::Low,
        }
    }

    /// Shell command danger detection (substring, case-insensitive).
    fn assess_shell(args: &serde_json::Value) -> RiskLevel {
        let command = args["command"].as_str().unwrap_or("");
        let lowered = command.to_lowercase();

        if CRITICAL_SHELL_PATTERNS
            .iter()
            .any(|pat| lowered.contains(pat))
        {
            return RiskLevel::Critical;
        }

        if HIGH_RISK_SHELL_PATTERNS
            .iter()
            .any(|pat| lowered.contains(pat))
        {
            return RiskLevel::High;
        }

        RiskLevel::Low
    }

    /// Process tool risk: `kill` is destructive → HIGH.
    fn assess_process(args: &serde_json::Value) -> RiskLevel {
        let action = args["action"].as_str().unwrap_or("list");
        match action {
            "kill" => RiskLevel::High,
            _ => RiskLevel::Low,
        }
    }

    /// File write/edit risk: writing into sensitive system directories → HIGH.
    fn assess_file_write(args: &serde_json::Value) -> RiskLevel {
        let path = args["path"].as_str().unwrap_or("");
        if path.is_empty() {
            return RiskLevel::Low;
        }

        let lowered = path.to_lowercase();
        if SENSITIVE_DIRS
            .iter()
            .any(|dir| lowered.starts_with(dir) || lowered.contains(dir))
        {
            return RiskLevel::High;
        }

        RiskLevel::Low
    }
}
