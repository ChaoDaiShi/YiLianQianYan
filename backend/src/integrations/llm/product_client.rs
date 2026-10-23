//! Model source resolution for product generation outside the chat runtime.
use super::{
    client::{LlmClient, UsageRecorder},
    model_pool::load_model_pool_runtime,
    usage::DatabaseUsageRecorder,
};
use crate::{config::types::ModelConfig, db::Database, integrations::secret::SecretResolver};
use std::sync::Arc;

/// An explicit default pool owns model selection. Invalid bindings are errors;
/// they never cause a switch to the active profile or legacy model settings.
pub fn configured_product_client(
    db: &Database,
    legacy: &ModelConfig,
    resolver: Arc<SecretResolver>,
    source: &str,
    operation_id: &str,
) -> Result<LlmClient, String> {
    if let Some(pool_id) = db.get_model_pool_binding("default")? {
        let pool = db.get_model_pool(&pool_id)?.ok_or("默认模型池已删除")?;
        if !pool.enabled || pool.service_kind != "chat" {
            return Err("默认模型池已停用或不是对话用途".into());
        }
        let mut first_config = None;
        for tier in &pool.tiers {
            if tier.profile_ids.is_empty() {
                return Err("默认模型池包含空候选层".into());
            }
            for id in &tier.profile_ids {
                let row = db.get_llm_model(id)?.ok_or("默认模型池候选档案已删除")?;
                if row.api_format != "openai" {
                    return Err("默认模型池候选不支持对话协议".into());
                }
                if first_config.is_none() {
                    first_config = Some(row.to_model_config());
                }
            }
        }
        let config = first_config.ok_or("默认模型池没有候选档案")?;
        let runtime = load_model_pool_runtime(
            db,
            Arc::clone(&resolver),
            &pool_id,
            &format!("{source}:{operation_id}"),
            Arc::new(|_| {}),
        )?;
        // The shared pool loader installs a usage recorder on every candidate;
        // do not count the same response again on this outer client.
        return Ok(LlmClient::new(&config, resolver).with_model_pool(runtime));
    }

    if let Some(active) = db.get_active_llm_model()? {
        if active.api_format != "openai" {
            return Err("当前模型档案不支持对话协议".into());
        }
        let config = active.to_model_config();
        let recorder: Arc<dyn UsageRecorder> = Arc::new(DatabaseUsageRecorder::new(
            db.clone_connection(),
            active.id,
            active.provider,
            active.model,
            source,
        ));
        return Ok(LlmClient::new_with_usage_recorder(
            &config,
            resolver,
            Some(recorder),
        ));
    }

    Ok(LlmClient::new(legacy, resolver))
}

#[cfg(test)]
mod tests;
