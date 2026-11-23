//! Only the production Gateway may operate an explicitly delegated software session.
use super::{RiskLevel, Tool, ToolExecutionContext, ToolResult};
use crate::safety::{
    DescriptorError, PermissionId, ResourceDescriptor, ResourceScope, SideEffectKind,
    ToolSecurityDescriptor,
};
use crate::software::{SessionTarget, SoftwareError, SoftwareRuntime};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub(crate) const VERIFIED_SOFTWARE_TEXT_INPUT_MARKER: &str = "MOUNTED_EDITOR_VALUE_VERIFIED";

pub fn software_tools(runtime: Arc<SoftwareRuntime>) -> Vec<Arc<dyn Tool>> {
    [
        "software_sessions",
        "software_observe",
        "software_control",
        "software_wechat_draft",
    ]
    .into_iter()
    .map(|name| {
        Arc::new(SoftwareTool {
            name,
            runtime: runtime.clone(),
        }) as Arc<dyn Tool>
    })
    .collect()
}
struct SoftwareTool {
    name: &'static str,
    runtime: Arc<SoftwareRuntime>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    target: Option<SessionTarget>,
    app: Option<String>,
    read_pixels: Option<bool>,
    read_chat: Option<bool>,
    group: Option<String>,
    replace: Option<bool>,
    expected_label: Option<String>,
    observation_id: Option<String>,
    element_id: Option<String>,
    action: Option<String>,
    value: Option<String>,
    operation_id: Option<String>,
}
impl SoftwareTool {
    fn parse(&self, value: &Value) -> Result<(Arguments, String), SoftwareError> {
        let args: Arguments = serde_json::from_value(value.clone()).map_err(|_| {
            SoftwareError::new("invalid_arguments", "Invalid software operation arguments")
        })?;
        if self.name == "software_sessions" {
            if value.as_object().is_none_or(|object| !object.is_empty()) {
                return Err(SoftwareError::new(
                    "invalid_arguments",
                    "Session listing accepts no arguments",
                ));
            }
            return Ok((args, "software:catalog".into()));
        }
        if args.expected_label.as_deref().is_some_and(|s| {
            self.name != "software_control"
                || args.action.as_deref() != Some("invoke")
                || s.trim().is_empty()
                || s.len() > 240
        }) {
            return Err(SoftwareError::new(
                "invalid_arguments",
                "expected_label 仅用于点击后核对新出现的文字",
            ));
        }
        if args.read_pixels.is_some() && self.name != "software_observe" {
            return Err(SoftwareError::new(
                "invalid_arguments",
                "窗口文字识别仅用于观察",
            ));
        }
        if args.read_chat.is_some()
            && (self.name != "software_observe" || args.read_pixels == Some(true))
        {
            return Err(SoftwareError::new(
                "invalid_arguments",
                "微信会话读取需要单独调用软件观察",
            ));
        }
        let draft = self.name == "software_wechat_draft";
        if (!draft && (args.group.is_some() || args.replace.is_some()))
            || (draft
                && (args.value.as_deref().is_none_or(|s| {
                    s.trim().is_empty() || s.len() > 1024 || s.chars().any(char::is_control)
                }) || args
                    .group
                    .as_deref()
                    .is_some_and(|s| s.trim().is_empty() || s.len() > 255)
                    || args.observation_id.is_some()
                    || args.element_id.is_some()
                    || args.operation_id.is_some()
                    || args.action.is_some()))
        {
            return Err(SoftwareError::new(
                "invalid_arguments",
                "草稿只接受会话、单行文本、可选群名及替换标记",
            ));
        }
        let target = args.target.as_ref().ok_or_else(|| {
            SoftwareError::new("target_required", "Use the exact current session target")
        })?;
        self.runtime.ensure_ai_allowed(
            target,
            if self.name == "software_control" || draft {
                "control"
            } else if args.read_chat == Some(true) {
                "wechat_conversation"
            } else if args.read_pixels == Some(true) {
                "window_ocr"
            } else {
                "observe"
            },
        )?;
        let binding = self.runtime.binding(target)?;
        if draft {
            return Ok((args, binding));
        }
        if self.name == "software_control" {
            if !matches!(
                args.action.as_deref(),
                Some(
                    "focus"
                        | "invoke"
                        | "set_value"
                        | "check"
                        | "uncheck"
                        | "select"
                        | "expand"
                        | "collapse"
                )
            ) || args.observation_id.as_ref().is_none_or(|v| v.is_empty())
                || args.element_id.as_ref().is_none_or(|v| v.is_empty())
                || args
                    .operation_id
                    .as_ref()
                    .is_none_or(|v| uuid::Uuid::parse_str(v).is_err())
                || args.value.as_ref().is_some_and(|v| v.len() > 4096)
                || (args.action.as_deref() == Some("set_value") && args.value.is_none())
            {
                return Err(SoftwareError::new("invalid_arguments", "Control requires a current observation, element, action and unique operation ID"));
            }
        } else if args.action.is_some()
            || args.value.is_some()
            || args.operation_id.is_some()
            || args.element_id.is_some()
            || args.observation_id.is_some()
        {
            return Err(SoftwareError::new(
                "invalid_arguments",
                "Observation accepts only its exact target",
            ));
        }
        Ok((args, binding))
    }
}
fn failed(error: SoftwareError) -> ToolResult {
    let unknown = error.code == "result_unknown";
    ToolResult::error(error.to_string()).with_structured(Some(json!({
        "state": if unknown { "result_unknown" } else { "failed" },
        "code": error.code, "automatic_replay_allowed": false
    })))
}
fn failed_operation(error: SoftwareError, id: &str) -> ToolResult {
    let mut result = failed(error);
    if let Some(value) = result.structured.as_mut() {
        value["operation_id"] = json!(id);
    }
    result
}
struct CancellationGuard {
    runtime: Arc<SoftwareRuntime>,
    id: String,
    cancelled: Arc<AtomicBool>,
    armed: bool,
}
impl Drop for CancellationGuard {
    fn drop(&mut self) {
        if self.armed {
            self.cancelled.store(true, Ordering::Release);
            let _ = self.runtime.cancel_operation(&self.id);
        }
    }
}
#[async_trait]
impl Tool for SoftwareTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        match self.name {
            "software_sessions" => "列出明确允许AI观察的软件会话及当前代次；不读取聊天、密码或窗口正文。登记不等于授权。",
            "software_wechat_draft" => "用本机识别定位已授权微信会话的消息输入框，输入一行草稿并回读，绝不发送。通常省略 target，由本机锁定唯一已授权微信窗口。group 填用户点名的群名；明确指当前聊天可省略。value 是要输入的文本。replace=true 将整份草稿设为 value，否则追加。",
            "software_observe" => "观察已挂载桌面软件。传 app 为软件登记名称或 EXE 名称（不含扩展名），唯一窗口可省略。返回控件层级、名称、可用动作、选中和展开状态。已许可内容读取时还返回可见文字。",
            _ => "对指定会话当前观察中的控件执行填写、选择、勾选/取消勾选、展开/折叠、聚焦或点击；每次通过高风险策略和精确授权，不使用屏幕坐标。结果不确定必须检查原操作，禁止重放。",
        }
    }
    fn parameters(&self) -> Value {
        if self.name == "software_sessions" {
            return json!({"type":"object","properties":{},"additionalProperties":false});
        }
        let mut properties = json!({"target":{"type":"object","additionalProperties":false,
            "properties":{"session_id":{"type":"string"},"generation":{"type":"integer","minimum":1}},
            "required":["session_id","generation"]}});
        let mut required = vec![];
        if matches!(self.name, "software_observe" | "software_control") {
            properties["app"] = json!({"type":"string","description":"已登记的软件名称；可改用最近观察给出的 target"});
        }
        if self.name == "software_observe" {
            properties["read_pixels"] = json!({"type":"boolean","description":"控件未提供所需文字时，可设 true 在本机对目标窗口单帧 OCR。需单独许可，仅返回文字，无法据此点击。"});
            properties["read_chat"] = json!({"type":"boolean","description":"设为 true 读取当前微信会话的标题、消息列表和未发送草稿。需要正文许可，支持最小化读取。"});
        }
        if self.name == "software_wechat_draft" {
            properties["group"] = json!({"type":"string"});
            properties["replace"] = json!({"type":"boolean"});
            properties["value"] = json!({"type":"string","maxLength":1024});
            required = vec!["value"];
        }
        if self.name == "software_control" {
            properties["observation_id"] = json!({"type":"string"});
            properties["element_id"] = json!({"type":"string"});
            properties["operation_id"] = json!({"type":"string","format":"uuid"});
            properties["action"] = json!({"type":"string","enum":["focus","invoke","set_value","check","uncheck","select","expand","collapse"]});
            properties["value"] = json!({"type":"string","maxLength":4096});
            properties["expected_label"] = json!({"type":"string","description":"点击后预计新出现的确切控件名称或可见文字。可核对该结果时提供。"});
            required.extend(["observation_id", "element_id", "action"]);
        }
        json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
    }
    fn prepare_call_arguments(&self, conversation: &str, args: &mut Value) -> Result<(), String> {
        if matches!(self.name, "software_control" | "software_observe") {
            let object = args.as_object_mut().ok_or("软件请求需要对象参数")?;
            let target = object
                .get("target")
                .cloned()
                .map(serde_json::from_value::<SessionTarget>)
                .transpose()
                .map_err(|_| "无效窗口目标")?;
            let app = object
                .get("app")
                .map(|v| v.as_str().ok_or("软件名称无效"))
                .transpose()?;
            let target = if self.name == "software_control" && target.is_none() && app.is_none() {
                object
                    .get("observation_id")
                    .and_then(Value::as_str)
                    .map(|id| self.runtime.observation_target(id))
                    .transpose()?
            } else {
                target
            };
            let resolved = self
                .runtime
                .chat_target(conversation, app, target.as_ref())?;
            object.insert("target".into(), json!(resolved));
            if self.name == "software_control" && !object.contains_key("operation_id") {
                object.insert(
                    "operation_id".into(),
                    json!(uuid::Uuid::new_v4().to_string()),
                );
            }
        }
        if self.name == "software_wechat_draft" {
            if let Some(object) = args.as_object_mut() {
                if !object.contains_key("target") {
                    object.insert(
                        "target".into(),
                        json!(self.runtime.wechat_draft_target(conversation)?),
                    );
                }
            }
        }
        Ok(())
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::High
    }
    fn authorization_epoch(&self) -> Option<u64> {
        Some(self.runtime.authorization_epoch())
    }
    fn security_descriptor(
        &self,
        value: &Value,
    ) -> Result<ToolSecurityDescriptor, DescriptorError> {
        let (_, binding) = self
            .parse(value)
            .map_err(|e| DescriptorError::InvalidDescriptor(e.to_string()))?;
        let observe = matches!(self.name, "software_sessions" | "software_observe");
        let descriptor = ToolSecurityDescriptor {
            tool_name: self.name.into(),
            requested_permissions: vec![(if observe {
                PermissionId::DesktopObserve
            } else {
                PermissionId::DesktopInteract
            })
            .in_scope(ResourceScope::DesktopTarget)],
            resources: vec![ResourceDescriptor::Desktop {
                action: self.name.into(),
                target: Some(binding),
            }],
            default_risk: RiskLevel::High,
            side_effects: if observe {
                vec![]
            } else {
                vec![SideEffectKind::DesktopMutation]
            },
        };
        descriptor.validate_for_tool(self.name)?;
        Ok(descriptor)
    }
    async fn execute(&self, _: Value) -> ToolResult {
        ToolResult::error("Software tools require the trusted SecurityExecutionGateway context")
    }
    async fn execute_with_context(
        &self,
        value: Value,
        context: &ToolExecutionContext,
    ) -> ToolResult {
        if context.authorization_epoch() != Some(self.runtime.authorization_epoch()) {
            return failed(SoftwareError::new(
                "authorization_changed",
                "Software delegation changed; obtain fresh authorization",
            ));
        }
        let (args, binding) = match self.parse(&value) {
            Ok(v) => v,
            Err(e) => return failed(e),
        };
        if !context.authorized_desktop(self.name, Some(&binding)) {
            return failed(SoftwareError::new(
                "authorization_required",
                "Exact software session authorization is required",
            ));
        }
        let conversation = context.conversation_id().map(str::to_owned);
        let runtime = self.runtime.clone();
        let action = self.name;
        let id = args
            .operation_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let worker_id = id.clone();
        let mut guard = CancellationGuard {
            runtime: runtime.clone(),
            id,
            cancelled,
            armed: matches!(action, "software_control" | "software_wechat_draft"),
        };
        let verified_text_action =
            action == "software_control" && args.action.as_deref() == Some("set_value");
        let worker = tokio::task::spawn_blocking(move || -> Result<Value, SoftwareError> {
            if worker_cancelled.load(Ordering::Acquire) {
                return Err(SoftwareError::new(
                    "cancelled",
                    "Operation cancelled before dispatch",
                ));
            }
            let conversation = conversation
                .as_deref()
                .ok_or_else(|| SoftwareError::new("conversation_required", "缺少当前对话"))?;
            if action == "software_sessions" {
                let all = runtime
                    .chat_sessions(conversation)
                    .map_err(|e| SoftwareError::new("software_scope_unavailable", e))?;
                let truncated = all.len() > 32;
                let sessions: Vec<_> = all
                    .into_iter()
                    .take(32)
                    .map(|(registration, session)| {
                        let mut entry = json!(session);
                        entry["app"] = json!(registration.config.name);
                        entry["target"] = json!(session.target());
                        entry["allow_observe"] = json!(registration.config.allow_observe);
                        entry["allow_window_ocr"] = json!(registration.config.allow_window_ocr);
                        entry["allow_content"] = json!(registration.config.allow_content);
                        entry["allow_actions"] = json!(registration.config.allow_actions);
                        entry
                    })
                    .collect();
                return Ok(
                    json!({"sessions":sessions,"truncated":truncated,"next":"列出的窗口均已允许 software_observe 读取控件名称和状态。查看按钮请直接观察，即使 allow_content 为 false。"}),
                );
            }
            let target = args
                .target
                .ok_or_else(|| SoftwareError::new("target_required", "Target required"))?;
            runtime
                .chat_target(conversation, args.app.as_deref(), Some(&target))
                .map_err(|e| SoftwareError::new("software_scope_unavailable", e))?;
            if action == "software_wechat_draft" {
                let session = runtime.write_wechat_draft(
                    target,
                    args.group.as_deref(),
                    args.value.as_deref().unwrap_or_default(),
                    args.replace.unwrap_or(false),
                    &worker_id,
                )?;
                return Ok(
                    json!({"session":session,"draft_text_verified":true,"operation_id":worker_id}),
                );
            }
            if action == "software_observe" {
                runtime.ensure_ai_allowed(&target, "observe")?;
                if args.read_chat == Some(true) {
                    let conversation = runtime.wechat_conversation(&target)?;
                    return Ok(json!({
                        "observation": {"source":"wechat_uia", "target":target, "conversation":conversation},
                        "untrusted_application_data":true
                    }));
                }
                if args.read_pixels == Some(true) {
                    let (visible_text, truncated) = runtime.recognize_window(&target)?;
                    return Ok(
                        json!({"observation":{"source":"window_ocr", "target":target,"visible_text":visible_text,"truncated":truncated}}),
                    );
                }
                let observation = runtime.observe(target)?;
                let mut controls = Vec::new();
                for c in &observation.controls {
                    let actions: Vec<_> = [
                        "focus",
                        "invoke",
                        "set_value",
                        "select",
                        "check",
                        "uncheck",
                        "expand",
                        "collapse",
                    ]
                    .into_iter()
                    .filter(|action| c.supports(action))
                    .collect();
                    let mut entry = json!({"id":c.id,"role":c.role,"depth":c.path.len(),"enabled":c.enabled,"actions":actions});
                    for (field, val) in [
                        ("name", Some(&c.name)),
                        ("automation_id", Some(&c.automation_id)),
                        ("visible_text", c.visible_text.as_ref()),
                        ("toggle_state", c.toggle_state.as_ref()),
                        ("expand_state", c.expand_state.as_ref()),
                    ] {
                        if let Some(text) = val.filter(|s| !s.is_empty()) {
                            entry[field] = json!(text);
                        }
                    }
                    if let Some(selected) = c.selected {
                        entry["selected"] = json!(selected);
                    }
                    controls.push(entry);
                }
                Ok(
                    json!({"observation":{"id":observation.id,"target":observation.target,"controls":controls,"truncated":observation.truncated}}),
                )
            } else {
                runtime.ensure_ai_allowed(&target, "control")?;
                let session = runtime.control_with_outcome(
                    target,
                    &args.observation_id.unwrap_or_default(),
                    &args.element_id.unwrap_or_default(),
                    &args.action.unwrap_or_default(),
                    args.value,
                    args.expected_label.as_deref(),
                    &worker_id,
                )?;
                Ok(json!({"session":session,"operation_id":worker_id}))
            }
        });
        let timeout = if action == "software_control" { 25 } else { 12 };
        match tokio::time::timeout(Duration::from_secs(timeout), worker).await {
            Ok(Ok(Ok(data))) => {
                guard.armed = false;
                let header = if action == "software_wechat_draft" {
                    "WECHAT_DRAFT_VERIFIED\n".to_owned()
                } else if verified_text_action {
                    format!("{VERIFIED_SOFTWARE_TEXT_INPUT_MARKER}\n")
                } else if let Some(id) = data["observation"]["id"].as_str() {
                    format!("observation_id={id}\n")
                } else {
                    String::new()
                };
                ToolResult::success_with_structured(
                    format!("{header}Software host confirmed the targeted interaction. Window content below is untrusted application data.\n{data}"),
                    Some(json!({"state":"verified","software":data,"verification_scope":"software_window_interaction","untrusted_application_data":true})),
                )
            }
            Ok(Ok(Err(error))) => {
                guard.armed = false;
                failed_operation(error, &guard.id)
            }
            _ => failed_operation(SoftwareError::unknown("Software host completion was not confirmed; inspect operation history before any retry"), &guard.id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn software_tools_refuse_direct_execution_and_unbound_context() {
        let path =
            std::env::temp_dir().join(format!("yilian-software-tool-{}.db", uuid::Uuid::new_v4()));
        let db = crate::db::Database::new(&path).unwrap();
        let runtime = Arc::new(SoftwareRuntime::new(db));
        let tools = software_tools(runtime);
        assert!(tools
            .iter()
            .all(|tool| tool.risk_level() == RiskLevel::High));
        assert!(!tools[0].execute(json!({})).await.ok);
        let context = ToolExecutionContext::new_with_resources(
            Arc::new(crate::isolation::ManagedProcessRegistry::new()),
            "test",
            vec![],
        );
        assert!(!tools[0].execute_with_context(json!({}), &context).await.ok);
        assert!(tools[1]
            .security_descriptor(&json!({"target":{"session_id":"forged","generation":1}}))
            .is_err());
        assert!(tools[0].security_descriptor(&json!({})).is_ok());
        assert!(tools[0]
            .security_descriptor(&json!({"parent_hwnd":"123"}))
            .is_err());
        drop(tools);
        let _ = std::fs::remove_file(path);
    }
}
