use crate::{
    db::Database,
    safety::{DescriptorError, ToolSecurityDescriptor},
    tools::{RiskLevel, Tool, ToolExecutionContext, ToolResult},
};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{path::Path, process::Stdio};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LspRequest {
    pub workspace_id: String,
    pub server_path: String,
    #[serde(default)]
    pub server_args: Vec<String>,
    pub language_id: String,
    pub path: String,
    pub action: String,
    pub line: u32,
    pub character: u32,
}
pub struct WorkspaceLspTool {
    db: Database,
}
impl WorkspaceLspTool {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
fn validate(input: &LspRequest) -> Result<(), String> {
    if !Path::new(&input.server_path).is_absolute() || !Path::new(&input.server_path).is_file() {
        return Err("请指定已安装语言服务器的完整可执行文件路径".into());
    }
    if !["definition", "references", "diagnostics"].contains(&input.action.as_str()) {
        return Err("不支持的语言服务操作".into());
    }
    if input.language_id.trim().is_empty() {
        return Err("请填写语言标识".into());
    }
    Ok(())
}
async fn send(writer: &mut tokio::process::ChildStdin, value: Value) -> Result<(), String> {
    let data = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
    writer
        .write_all(format!("Content-Length: {}\r\n\r\n", data.len()).as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    writer.write_all(&data).await.map_err(|e| e.to_string())?;
    writer.flush().await.map_err(|e| e.to_string())
}
async fn receive(reader: &mut BufReader<tokio::process::ChildStdout>) -> Result<Value, String> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader
            .read_line(&mut line)
            .await
            .map_err(|e| e.to_string())?
            == 0
        {
            return Err("语言服务器关闭了输出".into());
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("Content-Length") {
                length = Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .map_err(|_| "语言服务器消息长度无效")?,
                );
            }
        }
    }
    let length = length.ok_or("语言服务器响应缺少 Content-Length")?;
    if length > 8 * 1024 * 1024 {
        return Err("语言服务器响应超过 8 MiB".into());
    }
    let mut body = vec![0; length];
    reader
        .read_exact(&mut body)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&body).map_err(|e| e.to_string())
}
async fn server_request(
    writer: &mut tokio::process::ChildStdin,
    value: &Value,
) -> Result<(), String> {
    let id = value["id"].clone();
    let result = match value["method"].as_str() {
        Some("workspace/configuration") => Some(json!(value["params"]["items"]
            .as_array()
            .map(|items| vec![Value::Null; items.len()])
            .unwrap_or_default())),
        Some("client/registerCapability" | "window/workDoneProgress/create") => Some(Value::Null),
        _ => None,
    };
    send(writer, if let Some(result) = result { json!({"jsonrpc":"2.0","id":id,"result":result}) } else { json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Client method not supported"}}) }).await
}
async fn response(
    reader: &mut BufReader<tokio::process::ChildStdout>,
    writer: &mut tokio::process::ChildStdin,
    id: u32,
) -> Result<Value, String> {
    loop {
        let value = receive(reader).await?;
        if value.get("method").is_some() && value.get("id").is_some() {
            server_request(writer, &value).await?;
            continue;
        }
        if value["id"] == id {
            if let Some(error) = value.get("error") {
                return Err(format!("语言服务器错误：{error}"));
            }
            return Ok(value["result"].clone());
        }
    }
}
async fn execute(db: &Database, input: LspRequest) -> Result<Value, String> {
    validate(&input)?;
    let root = super::workspace_root(db, &input.workspace_id)?;
    let path = super::relative_path(&root, &input.path)?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !path.starts_with(&root) {
        return Err("源码文件不在所选工作空间内".into());
    }
    let source = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| e.to_string())?;
    let root_uri = reqwest::Url::from_directory_path(&root)
        .map_err(|_| "工作空间路径无法转换为URI")?
        .to_string();
    let uri = reqwest::Url::from_file_path(&path)
        .map_err(|_| "源码路径无法转换为URI")?
        .to_string();
    let mut command = tokio::process::Command::new(&input.server_path);
    command
        .args(&input.server_args)
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("启动语言服务器失败：{e}"))?;
    let mut writer = child.stdin.take().ok_or("语言服务器标准输入不可用")?;
    let mut reader = BufReader::new(child.stdout.take().ok_or("语言服务器标准输出不可用")?);
    send(&mut writer, json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":std::process::id(),"rootUri":root_uri,"capabilities":{"textDocument":{"publishDiagnostics":{},"diagnostic":{}},"workspace":{"configuration":true}},"workspaceFolders":[{"uri":root_uri,"name":input.workspace_id}]}})).await?;
    let initialized = response(&mut reader, &mut writer, 1).await?;
    send(
        &mut writer,
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    )
    .await?;
    send(&mut writer, json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":input.language_id,"version":1,"text":source}}})).await?;
    let result = if input.action == "diagnostics"
        && initialized["capabilities"]
            .get("diagnosticProvider")
            .is_none()
    {
        loop {
            let value = receive(&mut reader).await?;
            if value.get("method").is_some() && value.get("id").is_some() {
                server_request(&mut writer, &value).await?;
            } else if value["method"] == "textDocument/publishDiagnostics"
                && value["params"]["uri"] == uri
            {
                break value["params"]["diagnostics"].clone();
            }
        }
    } else {
        let method = match input.action.as_str() {
            "definition" => "textDocument/definition",
            "references" => "textDocument/references",
            _ => "textDocument/diagnostic",
        };
        let params = if input.action == "diagnostics" {
            json!({"textDocument":{"uri":uri}})
        } else {
            json!({"textDocument":{"uri":uri},"position":{"line":input.line,"character":input.character},"context":{"includeDeclaration":true}})
        };
        send(
            &mut writer,
            json!({"jsonrpc":"2.0","id":2,"method":method,"params":params}),
        )
        .await?;
        response(&mut reader, &mut writer, 2).await?
    };
    send(
        &mut writer,
        json!({"jsonrpc":"2.0","id":3,"method":"shutdown","params":null}),
    )
    .await?;
    response(&mut reader, &mut writer, 3).await?;
    send(&mut writer, json!({"jsonrpc":"2.0","method":"exit"})).await?;
    child.wait().await.map_err(|e| e.to_string())?;
    Ok(json!({"action":input.action,"file":input.path,"result":result}))
}
#[async_trait]
impl Tool for WorkspaceLspTool {
    fn name(&self) -> &str {
        "workspace_lsp"
    }
    fn description(&self) -> &str {
        "使用用户明确指定的已安装语言服务器，读取工作空间源码定义、引用或诊断。"
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::High
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{"workspace_id":{"type":"string"},"server_path":{"type":"string"},"server_args":{"type":"array","items":{"type":"string"}},"language_id":{"type":"string"},"path":{"type":"string"},"action":{"enum":["definition","references","diagnostics"]},"line":{"type":"integer","minimum":0},"character":{"type":"integer","minimum":0}},"required":["workspace_id","server_path","language_id","path","action","line","character"],"additionalProperties":false})
    }
    fn security_descriptor(&self, args: &Value) -> Result<ToolSecurityDescriptor, DescriptorError> {
        let input: LspRequest = serde_json::from_value(args.clone())
            .map_err(|e| DescriptorError::InvalidDescriptor(e.to_string()))?;
        validate(&input).map_err(DescriptorError::InvalidDescriptor)?;
        let root = super::workspace_root(&self.db, &input.workspace_id)
            .map_err(DescriptorError::InvalidDescriptor)?;
        super::descriptor(
            self.name(),
            &root,
            format!(
                "{} {} · LSP {} {}",
                input.server_path,
                json!(input.server_args),
                input.action,
                input.path
            ),
        )
    }
    async fn execute(&self, _args: Value) -> ToolResult {
        ToolResult::error("语言服务器启动必须通过安全执行网关")
    }
    async fn execute_with_context(
        &self,
        args: Value,
        _context: &ToolExecutionContext,
    ) -> ToolResult {
        let input = match serde_json::from_value(args) {
            Ok(input) => input,
            Err(error) => return ToolResult::error(error.to_string()),
        };
        match tokio::time::timeout(std::time::Duration::from_secs(60), execute(&self.db, input))
            .await
        {
            Ok(Ok(value)) => ToolResult::success_with_structured(value.to_string(), Some(value)),
            Ok(Err(error)) => ToolResult::error(error),
            Err(_) => ToolResult::error("语言服务器60秒内未完成请求，进程已停止"),
        }
    }
}
