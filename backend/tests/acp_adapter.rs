#![cfg(windows)]

use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use yilian_backend::integrations::acp::{
    AcpAgentTool, AcpError, AcpLaunchConfig, AcpPermissionDecision, AcpPermissionHandler,
    AcpPermissionRequest,
};
use yilian_backend::tools::Tool;

struct FixturePermissionHandler {
    calls: AtomicUsize,
}

#[async_trait]
impl AcpPermissionHandler for FixturePermissionHandler {
    async fn request_permission(
        &self,
        request: AcpPermissionRequest,
        _cancel: CancellationToken,
    ) -> Result<AcpPermissionDecision, AcpError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(request.session_id, "fixture-session");
        assert_eq!(request.tool_call["toolCallId"], "fixture-tool-call");
        assert!(request
            .options
            .iter()
            .any(|option| option["optionId"] == "allow-once"));
        Ok(AcpPermissionDecision::Selected("allow-once".to_string()))
    }
}

#[tokio::test]
async fn acp_tool_runs_stdio_session_permission_hook_and_verifies_expected_file() {
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    let powershell = PathBuf::from(system_root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    assert!(
        powershell.is_file(),
        "Windows PowerShell is required for this protocol fixture test"
    );

    let task_root = std::env::temp_dir().join(format!("yilian-acp-adapter-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&task_root).await.unwrap();
    let fixture = task_root.join("acp-fixture.ps1");
    tokio::fs::write(&fixture, powershell_fixture())
        .await
        .unwrap();

    let config = AcpLaunchConfig {
        id: "fixture-agent".to_string(),
        executable: powershell,
        args: vec![
            "-NoLogo".to_string(),
            "-NoProfile".to_string(),
            "-NonInteractive".to_string(),
            "-ExecutionPolicy".to_string(),
            "Bypass".to_string(),
            "-File".to_string(),
            fixture.to_string_lossy().into_owned(),
        ],
        cwd: task_root.clone(),
        env_allowlist: Vec::new(),
        timeout_seconds: 20,
    };
    let permission_handler = Arc::new(FixturePermissionHandler {
        calls: AtomicUsize::new(0),
    });
    let tool = AcpAgentTool::new(config, Some(permission_handler.clone())).unwrap();
    let result = tool
        .execute(json!({
            "task":"Create the expected fixture file.",
            "expected_outputs":["fixture-output.txt"]
        }))
        .await;

    assert!(result.ok, "{}", result.content);
    let structured = result.structured.expect("ACP result is structured");
    assert_eq!(structured["state"], "completed");
    assert_eq!(structured["stop_reason"], "end_turn");
    assert_eq!(
        structured["response_text"],
        "Fixture agent completed the task."
    );
    assert_eq!(structured["artifact_paths"].as_array().unwrap().len(), 1);
    assert_eq!(structured["model_calls"], Value::Null);
    assert_eq!(structured["total_tokens"], Value::Null);
    assert_eq!(structured["context_usage"]["context_used"], 12);
    assert_eq!(permission_handler.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        tokio::fs::read_to_string(task_root.join("fixture-output.txt"))
            .await
            .unwrap(),
        "ACP fixture artifact"
    );

    tokio::fs::remove_dir_all(task_root).await.unwrap();
}

fn powershell_fixture() -> &'static str {
    r#"
$sessionId = 'fixture-session'
while (($line = [Console]::In.ReadLine()) -ne $null) {
    $request = $line | ConvertFrom-Json
    $result = $null
    switch ($request.method) {
        'initialize' { $result = @{ protocolVersion = 1; agentCapabilities = @{}; agentInfo = @{ name = 'ACP fixture'; version = '1' } } }
        'session/new' { $result = @{ sessionId = $sessionId } }
        'session/prompt' {
            $permission = @{ jsonrpc = '2.0'; id = 77; method = 'session/request_permission'; params = @{ sessionId = $sessionId; toolCall = @{ toolCallId = 'fixture-tool-call'; title = 'Write expected fixture file' }; options = @(@{ optionId = 'allow-once'; name = 'Allow once'; kind = 'allow_once' }, @{ optionId = 'reject-once'; name = 'Reject'; kind = 'reject_once' }) } } | ConvertTo-Json -Compress -Depth 20
            [Console]::Out.WriteLine($permission)
            $permissionResponse = [Console]::In.ReadLine() | ConvertFrom-Json
            if ($permissionResponse.result.outcome.optionId -eq 'allow-once') {
                [System.IO.File]::WriteAllText((Join-Path (Get-Location) 'fixture-output.txt'), 'ACP fixture artifact', [System.Text.UTF8Encoding]::new($false))
            }
            [Console]::Out.WriteLine((@{ jsonrpc = '2.0'; method = 'session/update'; params = @{ sessionId = $sessionId; update = @{ sessionUpdate = 'agent_message_chunk'; messageId = 'message-1'; content = @{ type = 'text'; text = 'Fixture agent completed the task.' } } } } | ConvertTo-Json -Compress -Depth 20))
            [Console]::Out.WriteLine((@{ jsonrpc = '2.0'; method = 'session/update'; params = @{ sessionId = $sessionId; update = @{ sessionUpdate = 'usage_update'; used = 12; size = 100 } } } | ConvertTo-Json -Compress -Depth 20))
            $result = @{ stopReason = 'end_turn' }
        }
        'session/cancel' { $result = @{} }
        default { $result = @{} }
    }
    [Console]::Out.WriteLine((@{ jsonrpc = '2.0'; id = $request.id; result = $result } | ConvertTo-Json -Compress -Depth 20))
}
"#
}
