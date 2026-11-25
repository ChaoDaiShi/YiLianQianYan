//! Configuration and authority readiness checks without executing a turn.
use super::files::{checked_root, lexical_relative, local_grants};
use super::{AppServer, Automation};
use crate::{
    config::types::ModelConfig,
    safety::{
        grant::{GrantDecision, GrantEvaluator},
        PermissionId, ResourceDescriptor,
    },
};
use serde_json::{json, Value};
use std::path::Path;

pub(super) fn issue(
    issues: &mut Vec<Value>,
    code: &str,
    message: impl Into<String>,
    severity: &str,
) {
    issues.push(json!({"code":code,"message":message.into(),"severity":severity}));
}

fn model_configs(server: &AppServer, conversation: &str) -> Result<Vec<ModelConfig>, String> {
    use crate::api::chat::runtime::{resolve_model_source, ChatModelSource};
    let options = server.db.load_chat_options(conversation)?;
    if let Some(id) = options.model_profile_id {
        let row = server
            .db
            .get_llm_model(&id)?
            .ok_or("当前对话选定的模型档案已删除")?;
        if row.api_format != "openai" {
            return Err("当前对话模型不支持对话协议".into());
        }
        return Ok(vec![row.to_model_config()]);
    }
    match resolve_model_source(server, Some(conversation))? {
        ChatModelSource::Fixed { config, row } => {
            if row.is_some_and(|row| row.api_format != "openai") {
                return Err("当前模型不支持对话协议".into());
            }
            Ok(vec![config])
        }
        ChatModelSource::Pool(pool) => {
            if !pool.enabled
                || pool.service_kind != "chat"
                || pool.attempt_timeout_ms == 0
                || pool.total_timeout_ms < pool.attempt_timeout_ms
            {
                return Err("当前对话模型池已停用、用途不匹配或超时配置无效".into());
            }
            let mut configs = Vec::new();
            for tier in &pool.tiers {
                if tier.profile_ids.is_empty() {
                    return Err("当前模型池包含空候选层".into());
                }
                for id in &tier.profile_ids {
                    let row = server
                        .db
                        .get_llm_model(id)?
                        .ok_or("当前模型池候选档案已删除")?;
                    if row.api_format != "openai" {
                        return Err("模型池候选不支持对话协议".into());
                    }
                    configs.push(row.to_model_config());
                }
            }
            if configs.is_empty() {
                return Err("当前模型池没有候选档案".into());
            }
            Ok(configs)
        }
    }
}

pub(super) fn readiness(
    server: &AppServer,
    automation: &Automation,
    conversation: &str,
    root: &str,
    discovery: bool,
    writes: bool,
    source_error: Option<&str>,
) -> Vec<Value> {
    let mut issues = Vec::new();
    match super::super::super::workspace(server, &automation.workspace_id) {
        Err(error) => issue(&mut issues, "workspace", error, "error"),
        Ok(workspace) => {
            let current = Path::new(
                workspace
                    .root_path
                    .as_deref()
                    .unwrap_or(&server.workspace_root),
            );
            if lexical_relative(Path::new(root), current)
                .is_none_or(|relative| !relative.as_os_str().is_empty())
            {
                issue(&mut issues,"workspace_directory_changed","工作空间目录已更改；本委托仍按已保存目录执行。如需更换操作范围，请先停止并编辑核对新目录","warning");
            }
        }
    }
    let directory_valid = match checked_root(root) {
        Ok(_) => true,
        Err(error) => {
            issue(&mut issues, "directory", error, "error");
            false
        }
    };
    if let Some(error) = source_error {
        issue(&mut issues, "source", error, "error");
    }
    match model_configs(server, conversation) {
        Err(error) => issue(&mut issues, "model", error, "error"),
        Ok(configs) => {
            let mut credential_unchecked = false;
            for config in configs {
                if config.provider.trim().is_empty()
                    || config.name.trim().is_empty()
                    || !crate::modules::settings::domain::policy::valid_provider_url(
                        &config.base_url,
                    )
                    || config.invoke_timeout_ms == 0
                {
                    issue(
                        &mut issues,
                        "model",
                        "当前模型的名称、服务地址或超时配置不完整，请先检查模型设置",
                        "error",
                    );
                    break;
                }
                if let Some(reference) = &config.api_key_ref {
                    if reference.validate().is_err() {
                        issue(
                            &mut issues,
                            "model_credential",
                            "当前模型凭据引用无效，请检查模型设置",
                            "error",
                        );
                    } else {
                        credential_unchecked = true;
                    }
                } else if config.api_key.is_empty()
                    && (config.api_key_env.is_empty()
                        || std::env::var_os(&config.api_key_env)
                            .is_none_or(|value| value.is_empty()))
                {
                    issue(
                        &mut issues,
                        "model_credential",
                        "当前模型尚未配置可用的凭据来源，请检查模型设置",
                        "error",
                    );
                }
            }
            if credential_unchecked {
                issue(
                    &mut issues,
                    "credential_unchecked",
                    "这里只检查模型配置元数据；凭据会在运行时校验，未访问系统凭据或调用模型服务",
                    "warning",
                );
            }
        }
    }
    if discovery && directory_valid {
        if let Err(error) = super::super::search_authority::check(server, Path::new(root)) {
            issue(&mut issues, "discovery_permission", error, "error");
        }
    }
    if !directory_valid {
        return issues;
    }
    match local_grants(server) {
        Err(error) => issue(
            &mut issues,
            "permissions",
            format!("无法读取当前权限：{error}"),
            "error",
        ),
        Ok(grants) => {
            let evaluator = GrantEvaluator::new(grants, &server.workspace_root);
            let target = ResourceDescriptor::File { path: root.into() };
            for (permission, code, label) in [
                (PermissionId::FilesystemRead, "read_permission", "读取"),
                (
                    PermissionId::FilesystemWrite,
                    "write_permission",
                    "文件创建",
                ),
            ] {
                if permission == PermissionId::FilesystemWrite && !writes {
                    continue;
                }
                match evaluator.evaluate(permission, &target, chrono::Utc::now().timestamp_millis()) {
                    GrantDecision::Deny { .. } => issue(&mut issues, code,
                        format!("授权目录的{label}被当前权限明确拒绝，请先检查权限设置"), "error"),
                    GrantDecision::RequireApproval { .. } => issue(&mut issues, code,
                        format!("{label}可能需要逐项审批；委托勾选不会自动增加权限，具体路径仍由执行网关检查"), "warning"),
                    GrantDecision::Allow => {}
                }
            }
        }
    }
    issues
}
