use crate::integrations::llm::{client::LlmError, types::ChatCompletionResponse};
use serde::de::DeserializeOwned;

/// Accept two declared representations of one complete JSON object. Never
/// extract an object from prose, join blocks, or repair invalid/truncated JSON.
fn json_object_text(content: &str) -> Result<&str, &'static str> {
    let text = content.trim();
    if text.is_empty() {
        return Err("模型返回空正文");
    }
    if text.starts_with("```") {
        let (opening, rest) = text.split_once('\n').ok_or("JSON围栏不完整")?;
        if !matches!(opening.trim_end(), "```json" | "```") {
            return Err("只接受单一JSON代码围栏");
        }
        let (body, closing) = rest.rsplit_once('\n').ok_or("JSON围栏不完整")?;
        if closing.trim() != "```" {
            return Err("JSON围栏必须完整且不能夹杂说明文字");
        }
        return Ok(body.trim());
    }
    Ok(text)
}

pub(super) fn parse<T: DeserializeOwned>(
    response: ChatCompletionResponse,
    phase: &str,
    requested_tokens: u32,
) -> Result<T, String> {
    let completion_tokens = response
        .usage
        .as_ref()
        .map(|usage| usage.completion_tokens.to_string())
        .unwrap_or_else(|| "未提供".into());
    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| format!("{phase}：模型未返回候选响应"))?;
    let finish = match choice.finish_reason.as_deref() {
        Some("length" | "max_tokens") => "length",
        Some("content_filter" | "safety") => "content_filter",
        Some("stop") => "stop",
        Some("tool_calls" | "function_call") => "tool_calls",
        Some(_) => "other",
        None => "unspecified",
    };
    let context = format!(
        "结束原因={finish}，返回{completion_tokens} token，请求输出上限{requested_tokens} token"
    );
    if finish == "length" {
        return Err(format!(
            "{phase}：模型输出被截断（{context}），未采用不完整结果；请检查模型输出和上下文预算"
        ));
    }
    if finish == "content_filter" {
        return Err(format!("{phase}：模型服务过滤了本次输出（{context}）"));
    }
    if choice
        .message
        .tool_calls
        .as_ref()
        .is_some_and(|calls| !calls.is_empty())
        || finish == "tool_calls"
    {
        return Err(format!("{phase}：知识构建响应不允许工具调用（{context}）"));
    }
    let content = choice.message.content.as_deref().unwrap_or("");
    let text =
        json_object_text(content).map_err(|reason| format!("{phase}：{reason}（{context}）"))?;
    let value: serde_json::Value = serde_json::from_str(text).map_err(|error| {
        let kind = if error.is_eof() {
            "JSON未完整结束"
        } else {
            "JSON语法无效"
        };
        // Metadata only: model response text and user-source excerpts are never logged.
        tracing::warn!(
            phase,
            finish,
            response_bytes = content.len(),
            line = error.line(),
            column = error.column(),
            "Knowledge JSON syntax rejected"
        );
        format!(
            "{phase}：{kind}，位置{}:{}（{context}）；未自动修补或重试",
            error.line(),
            error.column()
        )
    })?;
    if !value.is_object() {
        return Err(format!("{phase}：约定响应必须为单一JSON对象（{context}）"));
    }
    serde_json::from_str(text).map_err(|error| {
        let description = error.to_string();
        let detail = if let Some(field) = description
            .strip_prefix("missing field `")
            .and_then(|tail| tail.split('`').next())
        {
            if field.len() <= 64
                && field
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
            {
                format!("缺少必填字段 {field}")
            } else {
                "缺少必填字段".into()
            }
        } else if description.starts_with("unknown field") {
            // Serde's expected field list comes from our Rust type. Do not echo
            // the unknown field name, which is untrusted model/source content.
            let fields = description
                .rsplit_once(", expected ")
                .map(|(_, expected)| {
                    expected
                        .split('`')
                        .enumerate()
                        .filter(|(index, _)| index % 2 == 1)
                        .map(|(_, field)| field)
                        .filter(|field| {
                            !field.is_empty()
                                && field.len() <= 64
                                && field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                        })
                        .collect::<Vec<_>>()
                        .join("、")
                })
                .unwrap_or_default();
            if fields.is_empty() {
                "包含未约定字段".into()
            } else {
                format!("包含未约定字段；此对象仅允许字段 {fields}")
            }
        } else if description.starts_with("unknown variant") {
            "字段枚举值不符合约定".into()
        } else {
            "字段类型不符合约定".into()
        };
        tracing::warn!(
            phase,
            finish,
            response_bytes = content.len(),
            line = error.line(),
            column = error.column(),
            "Knowledge JSON schema rejected"
        );
        format!(
            "{phase}：JSON结构不符合约定：{detail}，位置{}:{}（{context}）",
            error.line(),
            error.column()
        )
    })
}

pub(super) fn invocation_error(error: LlmError, phase: &str) -> String {
    let detail = match error {
        LlmError::NoApiKey => "模型未配置API凭据".into(),
        LlmError::SecretUnavailable => "模型凭据存储不可用".into(),
        LlmError::Timeout => "模型请求超时".into(),
        LlmError::Http(error) if error.is_timeout() => "模型请求超时".into(),
        LlmError::Http(_) => "模型HTTP连接或响应读取失败".into(),
        LlmError::Provider { status, .. } => {
            format!("模型服务返回HTTP {status}；请检查模型配置及JSON Output支持")
        }
        LlmError::Parse(_) => "模型服务响应不符合对话API格式".into(),
        _ => "已配置模型服务未完成请求，请查看模型用量或路由记录".into(),
    };
    format!("{phase}：{detail}")
}

#[cfg(test)]
mod tests;
