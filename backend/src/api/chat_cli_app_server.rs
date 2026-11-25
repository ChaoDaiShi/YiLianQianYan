use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::mpsc::Sender,
};

use super::{AgentEvent, CliResult};

pub(super) fn home() -> Option<PathBuf> {
    std::env::var_os("YILIAN_CODEX_APP_SERVER_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_dir() && !path.join("config.toml").exists())
}

pub(super) fn configure(command: &mut Command) -> Result<(), String> {
    let home = home().ok_or("Codex App Server 需要独立凭据目录")?;
    command.env("CODEX_HOME", home);
    for key in ["CODEX_API_KEY", "OPENAI_API_KEY"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command.args([
        "app-server",
        "--stdio",
        "-c",
        "web_search=\"disabled\"",
        "-c",
        "mcp_servers={}",
        "-c",
        "plugins={}",
    ]);
    super::disable_codex_execution_features(command);
    Ok(())
}

struct Connection {
    input: ChildStdin,
    output: Lines<BufReader<ChildStdout>>,
}

impl Connection {
    async fn send(&mut self, event: Value) -> Result<(), String> {
        let line = format!("{event}\n");
        self.input
            .write_all(line.as_bytes())
            .await
            .map_err(|_| "无法发送 Codex App Server 请求".into())
    }

    async fn next(&mut self) -> Result<Value, String> {
        loop {
            let line = self
                .output
                .next_line()
                .await
                .map_err(|_| "读取 Codex App Server 事件失败")?
                .ok_or("Codex App Server 提前退出")?;
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if let (Some(id), Some(method)) = (value.get("id"), value["method"].as_str()) {
                match method {
                    "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
                        self.send(json!({"id":id,"result":{"decision":"decline"}}))
                            .await?;
                    }
                    _ => {
                        self.send(
                            json!({"id":id,"error":{"code":-32601,"message":"Text-only session"}}),
                        )
                        .await?;
                    }
                }
                continue;
            }
            return Ok(value);
        }
    }

    async fn response(&mut self, id: u64) -> Result<Value, String> {
        loop {
            let event = self.next().await?;
            if event["id"].as_u64() != Some(id) {
                continue;
            }
            return event
                .get("result")
                .cloned()
                .ok_or_else(|| format!("Codex App Server 请求失败（步骤 {id}）"));
        }
    }
}

pub(super) async fn run(
    mut child: Child,
    directory: &Path,
    payload: String,
    conversation: &str,
    tx: &Sender<AgentEvent>,
    result: &mut CliResult,
) -> Result<(), String> {
    let mut rpc = Connection {
        input: child.stdin.take().ok_or("Codex App Server stdin 不可用")?,
        output: BufReader::new(
            child
                .stdout
                .take()
                .ok_or("Codex App Server stdout 不可用")?,
        )
        .lines(),
    };
    rpc.send(json!({"id":1,"method":"initialize","params":{
        "clientInfo":{"name":"yilianqianyan","title":"YiLianQianYan","version":env!("CARGO_PKG_VERSION")}
    }})).await?;
    rpc.response(1).await?;
    rpc.send(json!({"method":"initialized","params":{}}))
        .await?;
    rpc.send(json!({"id":2,"method":"thread/start","params":{
        "cwd":directory,
        "approvalPolicy":"never",
        "sandbox":"read-only",
        "ephemeral":true,
        "serviceName":"yilianqianyan"
    }}))
    .await?;
    let thread = rpc.response(2).await?["thread"]["id"]
        .as_str()
        .ok_or("Codex App Server 未返回线程标识")?
        .to_owned();
    result.session_id = Some(thread.clone());
    rpc.send(json!({"id":3,"method":"turn/start","params":{
        "threadId":thread,
        "input":[{"type":"text","text":payload}]
    }}))
    .await?;
    let mut tool_events = std::collections::HashSet::new();
    loop {
        let event = rpc.next().await?;
        if event["id"] == 3 && event.get("error").is_some() {
            return Err("Codex App Server 无法启动当前轮次".into());
        }
        let params = &event["params"];
        if params["threadId"].as_str() != Some(thread.as_str()) {
            continue;
        }
        match event["method"].as_str() {
            Some("item/agentMessage/delta") => {
                if let Some(token) = params["delta"].as_str() {
                    result.text.push_str(token);
                    let _ = tx
                        .send(AgentEvent {
                            event_type: "token".into(),
                            token: Some(token.to_owned()),
                            error: None,
                            ..AgentEvent::error_event(conversation, String::new())
                        })
                        .await;
                }
            }
            Some("item/started" | "item/completed") => {
                let start = event["method"] == "item/started";
                if let (Some(id), Some(kind)) = (
                    params["item"]["id"].as_str(),
                    params["item"]["type"].as_str(),
                ) {
                    if matches!(
                        kind,
                        "commandExecution" | "fileChange" | "mcpToolCall" | "webSearch"
                    ) && (start || tool_events.remove(id))
                    {
                        if start {
                            tool_events.insert(id.to_owned());
                        }
                        let status = params["item"]["status"].as_str().unwrap_or("observed");
                        let _ = tx
                            .send(AgentEvent {
                                event_type: if start { "tool_start" } else { "tool_end" }.into(),
                                tool_call_id: Some(format!("codex:{thread}:{id}")),
                                tool_name: Some(kind.to_owned()),
                                status: Some(status.to_owned()),
                                error: None,
                                ..AgentEvent::error_event(conversation, String::new())
                            })
                            .await;
                    }
                }
            }
            Some("thread/tokenUsage/updated") => {
                let usage = &params["tokenUsage"]["total"];
                result.usage = super::token_counts(&json!({
                    "input_tokens":usage["inputTokens"],
                    "cached_input_tokens":usage["cachedInputTokens"],
                    "output_tokens":usage["outputTokens"],
                    "total_tokens":usage["totalTokens"]
                }));
            }
            Some("turn/completed") => {
                result.completed = params["turn"]["status"] == "completed";
                break;
            }
            _ => {}
        }
    }
    if result.completed && !result.text.is_empty() {
        Ok(())
    } else {
        Err("Codex App Server 未完成回复，请检查独立登录和模型配置".into())
    }
}
