use super::ProofError;

impl super::DraftService {
    pub(super) fn verify_confirmations(
        &self,
        candidate: Option<&crate::modules::task::proofloop::contracts::CandidatePlan>,
        confirmations: &[crate::modules::task::proofloop::contracts::BindingConfirmation],
        now: i64,
    ) -> Result<Vec<crate::modules::task::proofloop::contracts::BindingConfirmation>, ProofError>
    {
        let mut verified = Vec::new();
        for confirmation in confirmations {
            let source_node = candidate
                .and_then(|plan| {
                    plan.nodes
                        .iter()
                        .find(|node| node.id == confirmation.source_node_id)
                })
                .ok_or_else(|| {
                    ProofError::new("invalid_binding_confirmation", "确认的来源节点不存在")
                })?;
            if source_node.capability_id != confirmation.source_capability_id {
                return Err(ProofError::new(
                    "invalid_binding_confirmation",
                    "来源能力已改变，请重新选择真实输出",
                ));
            }
            let id = crate::modules::task::NodeExecutionId::new(&confirmation.source_execution_id)
                .map_err(|error| {
                    ProofError::new("invalid_binding_confirmation", error.to_string())
                })?;
            let execution = self
                .db
                .load_node_execution(&id)
                .map_err(|error| ProofError::new("db_error", error.to_string()))?
                .ok_or_else(|| {
                    ProofError::new("invalid_binding_confirmation", "确认引用的执行不存在")
                })?;
            if execution.status != crate::modules::task::NodeExecutionStatus::Succeeded
                || !execution.validation.as_ref().is_some_and(|validation| {
                    validation.status == crate::modules::task::execution::ValidationStatus::Accepted
                })
                || execution
                    .executor_ref
                    .as_ref()
                    .map(|reference| reference.as_str())
                    != Some(format!("capability://{}", confirmation.source_capability_id).as_str())
            {
                return Err(ProofError::new(
                    "invalid_binding_confirmation",
                    "来源执行没有通过验证或能力身份不一致",
                ));
            }
            let mut value = execution.output.as_ref().ok_or_else(|| {
                ProofError::new("invalid_binding_confirmation", "执行没有结构化结果")
            })?;
            for segment in &confirmation.source_path {
                value = value
                    .as_object()
                    .and_then(|object| object.get(segment))
                    .ok_or_else(|| {
                        ProofError::new("invalid_binding_confirmation", "真实输出中不存在该路径")
                    })?;
            }
            let mut item = confirmation.clone();
            item.observed_type = match value {
                serde_json::Value::Null => "null",
                serde_json::Value::Bool(_) => "boolean",
                serde_json::Value::String(_) => "string",
                serde_json::Value::Array(_) => "array",
                serde_json::Value::Object(_) => "object",
                serde_json::Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
                _ => "number",
            }
            .into();
            item.confirmed_at_ms = now;
            verified.push(item);
        }
        Ok(verified)
    }
}
