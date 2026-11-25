use serde_json::json;
use std::{path::Path, sync::Arc};

use crate::{
    app::state::AppServer,
    safety::{
        execution_gateway::SecurityExecutionOutcome, PolicyDecision, SecurityExecutionRequest,
        SecuritySubject,
    },
    tools::RiskLevel,
};

use super::WatchSource;

pub(super) struct Sample {
    pub(super) fingerprint: Option<String>,
    pub(super) http_status: Option<u16>,
    pub(super) source_at: Option<i64>,
    pub(super) preview: Option<String>,
}

pub(super) enum SourceFailure {
    Blocked(String),
    Unavailable(String),
}

impl WatchSource {
    pub(super) fn same_location(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::File { path: a }, Self::File { path: b }) => a == b,
            (Self::Http { url: a, .. }, Self::Http { url: b, .. }) => a == b,
            _ => false,
        }
    }

    pub(super) fn description(&self) -> (&'static str, String) {
        match self {
            Self::File { path } => ("file", path.clone()),
            Self::Http { url, .. } => {
                let display = url::Url::parse(url).map(|mut url| {
                    url.set_query(None);
                    url.set_fragment(None);
                    url.to_string()
                });
                ("http", display.unwrap_or_default())
            }
        }
    }
}

pub(super) async fn collect(
    server: &AppServer,
    source: &WatchSource,
    root: &str,
    conversation: &str,
    check: &str,
) -> Result<Sample, SourceFailure> {
    let gateway = crate::safety::production_gateway::build(
        server,
        root,
        Arc::clone(&server.tool_registry),
        None,
    );
    let request = |tool: &str, arguments| SecurityExecutionRequest {
        conversation_id: conversation.into(),
        tool_call_id: check.into(),
        tool_name: tool.into(),
        arguments,
        subject: SecuritySubject::local_user(),
    };
    match source {
        WatchSource::File { path } => {
            match gateway.evaluate(&request("read_file", json!({"path":path})), RiskLevel::Low) {
                Ok(PolicyDecision::Allow(_)) => {}
                _ => {
                    return Err(SourceFailure::Blocked(
                        "当前权限未允许自动读取此文件".into(),
                    ))
                }
            }
            let observed = super::super::file_events::sample(Path::new(root), path)
                .map_err(SourceFailure::Unavailable)?;
            Ok(Sample {
                fingerprint: Some(observed.fingerprint),
                http_status: None,
                source_at: observed.modified_at,
                preview: None,
            })
        }
        WatchSource::Http { url, send_preview } => {
            let request = request(
                "http_request",
                json!({"url":url,"method":"GET","timeout_ms":10_000,
                    "observation_metadata":true,"observation_preview":send_preview}),
            );
            let result = match gateway.execute(&request, RiskLevel::Medium).await {
                Ok(SecurityExecutionOutcome::Executed { tool_result, .. }) if tool_result.ok => {
                    tool_result
                }
                Ok(SecurityExecutionOutcome::Executed { .. }) => {
                    return Err(SourceFailure::Unavailable("网络来源无法读取".into()))
                }
                _ => {
                    return Err(SourceFailure::Blocked(
                        "当前权限未允许自动读取此网址".into(),
                    ))
                }
            };
            let Some(metadata) = result.structured else {
                return Err(SourceFailure::Unavailable(
                    "网络来源未返回可核对的响应".into(),
                ));
            };
            let status = metadata["status"]
                .as_u64()
                .and_then(|value| u16::try_from(value).ok())
                .ok_or_else(|| SourceFailure::Unavailable("网络响应缺少状态码".into()))?;
            if !(200..300).contains(&status) {
                return Ok(Sample {
                    http_status: Some(status),
                    fingerprint: None,
                    source_at: None,
                    preview: None,
                });
            }
            let fingerprint = metadata["fingerprint"]
                .as_str()
                .ok_or_else(|| SourceFailure::Unavailable("网络响应缺少内容指纹".into()))?;
            let source_at = metadata["last_modified"]
                .as_str()
                .and_then(|raw| chrono::DateTime::parse_from_rfc2822(raw).ok())
                .map(|time| time.timestamp_millis());
            Ok(Sample {
                fingerprint: Some(fingerprint.into()),
                http_status: Some(status),
                source_at,
                preview: if *send_preview {
                    metadata["preview"].as_str().map(str::to_owned)
                } else {
                    None
                },
            })
        }
    }
}
