use super::{runtime::ChatRuntime, ChatApiError};
use crate::{
    agent::engine::AgentEvent,
    db::conversation_modes::WorkspaceMode,
    execution::context_manifest::ContextStatus,
    integrations::llm::client::LlmClient,
    interaction::chat_understanding::{self, Action, Domain, Exchange, Scope, Understanding},
};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

const ROUTE_KIND: &str = "interaction_route";

pub(super) async fn refine(
    understanding: &mut Understanding,
    mode: WorkspaceMode,
    client: &LlmClient,
    runtime: &ChatRuntime,
    text: &str,
    cancel: &CancellationToken,
) -> Result<(), ChatApiError> {
    if mode != WorkspaceMode::Auto
        || runtime.model_pool_id.is_some()
        || !understanding.semantic_candidate(text)
        || crate::safety::contains_sensitive_content(text)
    {
        return Ok(());
    }
    let messages = chat_understanding::semantic_messages(text);
    let result = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(super::preparation_cancelled()),
        result = tokio::time::timeout(Duration::from_secs(3), client.invoke_with_max_tokens(&messages, &[], 250)) => result,
    };
    let labels = result.ok().and_then(Result::ok).and_then(|reply| {
        let body = reply.choices.first()?.message.content.as_deref()?.trim();
        let body = body
            .strip_prefix("```json")
            .and_then(|body| body.strip_suffix("```"))
            .unwrap_or(body);
        serde_json::from_str(body).ok()
    });
    if let Some(labels) = labels {
        understanding.apply_semantic(labels);
    } else {
        understanding.reasons.push("semantic_unavailable");
    }
    Ok(())
}

fn apply_scope(runtime: &mut ChatRuntime, scope: Scope, version: Option<&str>) {
    runtime.tools = Arc::new(runtime.tools.filtered(|name| {
        if matches!(name, "save_memory" | "update_memory") {
            return matches!(version, Some("memory" | "memory/v2"));
        }
        match scope {
            Scope::Conversation => name == "ask_user",
            Scope::Read => chat_understanding::allows_read_tool(name),
            Scope::WechatDraft => {
                matches!(name, "software_wechat_draft" | "ask_user")
                    // Earlier turns may already have a pending session lookup approval.
                    || name == "software_sessions" && matches!(version, Some("standard" | "memory"))
            }
            Scope::SoftwareRead | Scope::SoftwareControl => {
                matches!(name, "ask_user" | "software_sessions" | "software_observe")
                    || scope == Scope::SoftwareControl && name == "software_control"
            }
            Scope::Work => true,
        }
    }));
}

fn guidance(understanding: &Understanding) -> &'static str {
    if matches!(
        understanding.scope,
        Scope::SoftwareRead | Scope::SoftwareControl
    ) {
        return "用户点名软件时，第一步直接用 software_observe，app 填用户说的软件名。仅指当前窗口时可省略 app。工具返回找不到目标或目标不唯一时，才用 software_sessions。列表中的窗口均可观察控件，allow_content 只决定正文。用户要读当前微信会话时用 software_observe 的 read_chat=true，允许微信保持最小化。按可见文字、控件层级和状态理解界面。如果用户要读画面但结构化控件未提供所需文字，且允许窗口文字识别，用 software_observe 的 read_pixels=true 读取该窗口。要求只看时直接据实回答；要求操作时，从最新观察选 element_id 和 observation_id 调用 software_control。不要让用户提供句柄或控件编号。只执行请求的操作，保留草稿、禁止保存或发送等限制。切换界面后重新观察，只有核对到目标状态才报告完成。";
    }
    if understanding.scope == Scope::WechatDraft {
        return "直接调用 software_wechat_draft，省略 target，由本机选择已连接且已获许可的微信窗口。群名与内容取自用户请求，接续修改时沿用明确的群名。要求补成、改成或指定最终草稿用 replace=true；要求追加用 false。只有工具回读成功才能报告已填写。窗口或许可不足时简短转述工具给出的下一步。";
    }
    if understanding.uncertain_reference && understanding.scope == Scope::Conversation {
        return "如果无法从已有上下文唯一确定用户指的对象，向用户简短核实对象。";
    }
    if understanding.exchange == Exchange::Mixed {
        return "先自然回应用户的感受或近况，再回应实际请求。以当前允许的操作范围为准。";
    }
    if understanding.domains.contains(&Domain::Research)
        || understanding.domains.contains(&Domain::Web)
    {
        return "研究时说明依据和来源状态，区分已核实的事实与推断。";
    }
    if understanding.domains.contains(&Domain::Development)
        && understanding.actions.contains(&Action::Execute)
    {
        return "先了解实际工程和约束，完成操作后用真实结果验证。";
    }
    "回答当前请求，说明真正完成的结果和仍需确认的事项。"
}

pub(super) fn apply(runtime: &mut ChatRuntime, understanding: &Understanding) {
    let version = match (
        understanding.scope == Scope::WechatDraft,
        understanding.memory_change,
    ) {
        (true, true) => "memory/v2",
        (true, false) => "standard/v2",
        (false, true) => "memory",
        (false, false) => "standard",
    };
    apply_scope(runtime, understanding.scope, Some(version));
    let scope = match understanding.scope {
        Scope::Conversation => "conversation",
        Scope::Read => "read",
        Scope::WechatDraft => "wechat_draft",
        Scope::SoftwareRead => "software_read",
        Scope::SoftwareControl => "software_control",
        Scope::Work => "work",
    };
    let prompt = guidance(understanding);
    runtime.prompt.push_str("\n\n");
    runtime.prompt.push_str(prompt);
    runtime.context_manifest.record(
        ROUTE_KIND,
        scope,
        Some(version.into()),
        ContextStatus::Resolved,
        0,
        Some(prompt),
    );
    runtime.context_manifest.record(
        "interaction_basis",
        scope,
        None,
        ContextStatus::Resolved,
        0,
        Some(&understanding.reasons.join(", ")),
    );
    if let Some(agent) = &runtime.agent_id {
        runtime.context_manifest.record(
            "interaction_agent",
            agent,
            None,
            ContextStatus::Resolved,
            0,
            None,
        );
    }
}

pub(super) fn restore(
    runtime: &mut ChatRuntime,
    entry: &crate::execution::context_manifest::ContextEntry,
) -> Result<(), String> {
    let scope = match entry.source_id.as_str() {
        "conversation" => Scope::Conversation,
        "read" => Scope::Read,
        "wechat_draft" => Scope::WechatDraft,
        "software_read" => Scope::SoftwareRead,
        "software_control" => Scope::SoftwareControl,
        "work" => Scope::Work,
        _ => return Err("无法恢复此轮的交互范围".into()),
    };
    apply_scope(runtime, scope, entry.version.as_deref());
    if let Some(reason) = &entry.reason {
        runtime.prompt.push_str("\n\n");
        runtime.prompt.push_str(reason);
    }
    runtime.context_manifest.entries.push(entry.clone());
    Ok(())
}

pub(super) fn route_entry(
    manifest: &crate::execution::context_manifest::ContextManifest,
) -> Option<&crate::execution::context_manifest::ContextEntry> {
    manifest
        .entries
        .iter()
        .find(|entry| entry.kind == ROUTE_KIND)
}

pub(super) fn event(id: &str, understanding: &Understanding, agent_id: Option<&str>) -> AgentEvent {
    let mut event = AgentEvent::error_event(id, String::new());
    event.event_type = "interaction_route".into();
    event.error = None;
    let mut details = serde_json::json!(understanding);
    details["agent_id"] = serde_json::json!(agent_id);
    event.args = Some(details);
    event
}
