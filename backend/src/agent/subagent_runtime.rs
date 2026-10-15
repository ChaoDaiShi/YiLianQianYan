// ============================================================
// Subagent Runtime Execution Foundation — isolated Child Agent.
//
// A subagent delegation runs as a brand-new Child AgentState with its own
// restricted tool registry, security gateway, approval store, and ReAct loop.
//
// Hard runtime constraints this round:
//   - Child registry is a strict whitelist of `allowed_tools` copied from the
//     parent source registry; no implicit builtins.
//   - `subagent_*` tools are always filtered out (single-layer delegation).
//   - Nested approvals fail closed (child uses an independent ApprovalStore).
//   - workdir override is not supported yet (fail closed).
//   - Only the model *name* may be overridden; provider config is inherited.
//   - The child never inherits parent conversation history / memory / workflow.
//
// Production wiring (Step 6D) will construct this executor from AppServer.
// Until then the executor and its helpers are exercised only by tests, so
// dead-code warnings are suppressed here on purpose.
// ============================================================

#![allow(dead_code)]

use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::engine;
use crate::agent::state::AgentState;
use crate::agent::verifier::DefaultVerifier;
use crate::app::state::LogBuffer;
use crate::config::types::AppConfig;
use crate::db::Database;
use crate::integrations::llm::client::LlmClient;
use crate::integrations::secret::SecretResolver;
use crate::modules::task::artifact::ArtifactService;
use crate::modules::task::model::ArtifactId;
use crate::safety::approval::ApprovalStore;
use crate::safety::AuditRecorder;
use crate::safety::SecurityExecutionGateway;
use crate::tools::registry::ToolRegistry;
use crate::tools::subagent::{SubagentExecutionSpec, SubagentExecutor};
use crate::tools::trait_def::ToolResult;

/// Maximum characters of the final child result returned to the parent.
pub(crate) const MAX_SUBAGENT_RESULT_CHARS: usize = 16_000;

/// Runtime executor that runs a local Child Agent using the shared ReAct engine.
pub(crate) struct LocalSubagentExecutor {
    config: AppConfig,
    workspace_root: String,
    source_tool_registry: Arc<ToolRegistry>,
    db: Database,
    audit_recorder: AuditRecorder,
    log_buffer: LogBuffer,
    resolver: Arc<SecretResolver>,
    slots: Arc<tokio::sync::Semaphore>,
}

impl LocalSubagentExecutor {
    pub(crate) fn new(
        config: AppConfig,
        workspace_root: String,
        source_tool_registry: Arc<ToolRegistry>,
        db: Database,
        audit_recorder: AuditRecorder,
        log_buffer: LogBuffer,
        resolver: Arc<SecretResolver>,
    ) -> Self {
        Self {
            config,
            workspace_root,
            source_tool_registry,
            db,
            audit_recorder,
            log_buffer,
            resolver,
            slots: Arc::new(tokio::sync::Semaphore::new(2)),
        }
    }

    /// Share the app-lifetime child limit across capability registry rebuilds.
    pub(crate) fn with_slots(mut self, slots: Arc<tokio::sync::Semaphore>) -> Self {
        self.slots = slots;
        self
    }
}

#[async_trait]
impl SubagentExecutor for LocalSubagentExecutor {
    async fn execute_subagent(
        &self,
        spec: &SubagentExecutionSpec,
        task: &str,
        handoff: Option<&crate::agent::delegation::DelegationRequest>,
    ) -> ToolResult {
        if let Err(reason) = validate_runtime_spec(spec) {
            return ToolResult::error(reason);
        }
        let inherited = crate::agent::delegation::current_scope();
        if let Some(request) = handoff {
            if let Err(reason) = validate_handoff_scope(request, inherited.as_ref()) {
                return ToolResult::error(reason);
            }
        }
        let started = std::time::Instant::now();
        let timeout_ms = handoff
            .and_then(|request| request.timeout_ms)
            .unwrap_or(600_000);
        let monotonic_start = tokio::time::Instant::now();
        let requested_deadline = monotonic_start + std::time::Duration::from_millis(timeout_ms);
        let deadline = inherited
            .as_ref()
            .and_then(|scope| scope.deadline)
            .map(|parent_deadline| parent_deadline.min(requested_deadline))
            .unwrap_or(requested_deadline);
        if deadline <= monotonic_start {
            return ToolResult::error("父任务剩余时间不足");
        }
        let cancel_token = inherited
            .as_ref()
            .map(|scope| scope.cancel.child_token())
            .unwrap_or_default();
        let budget = inherited
            .as_ref()
            .and_then(|scope| scope.budget.clone())
            .unwrap_or_else(|| {
                crate::modules::task::resource_budget::SharedExecutionBudget::new(6, 24_000)
            });
        let (calls, tokens) = match budget.available() {
            Ok(v) => v,
            Err(error) => return ToolResult::error(error),
        };
        let reserved_calls = calls.saturating_sub(1).min(3);
        let reserved_tokens = tokens.saturating_sub(1_000).min(12_000);
        if reserved_calls == 0 || reserved_tokens == 0 {
            return ToolResult::error("proofloop_budget_exhausted");
        }
        let handoff_id = format!("handoff-{}", uuid::Uuid::new_v4());
        let reservation =
            match budget.reserve(crate::modules::task::resource_budget::BudgetReservation {
                handoff_id: handoff_id.clone(),
                calls: reserved_calls,
                tokens: reserved_tokens,
            }) {
                Ok(value) => value,
                Err(error) => return ToolResult::error(error),
            };
        let _permit = tokio::select! {
            biased;
            _=cancel_token.cancelled()=>return ToolResult::error("子任务已取消"),
            result=tokio::time::timeout_at(deadline,Arc::clone(&self.slots).acquire_owned())=>match result {
                Ok(Ok(permit))=>permit,
                Ok(Err(_))=>return ToolResult::error("子任务执行槽已关闭"),
                Err(_)=>return ToolResult::error("子任务执行超时"),
            }
        };
        let child_config = build_child_config(&self.config, spec.model.as_deref());
        let child_tools = match effective_child_tools(spec, handoff, &self.source_tool_registry) {
            Ok(tools) => tools,
            Err(error) => return ToolResult::error(error),
        };
        let child_registry = build_child_tool_registry(&self.source_tool_registry, &child_tools);

        let system_prompt = build_child_system_prompt(&spec.name, &spec.instructions, handoff);
        let mut state = AgentState::new(system_prompt);
        state.add_user_message(build_child_task(task, handoff));

        let conversation_id = format!("subagent:{}:{}", spec.name, uuid::Uuid::new_v4());

        let client = LlmClient::new(&child_config.model, Arc::clone(&self.resolver));
        let security_gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            child_config.sandbox.clone(),
            self.workspace_root.clone(),
            Arc::clone(&child_registry),
            Arc::new(DefaultVerifier::new(&self.workspace_root)),
            Arc::new(self.audit_recorder.clone()),
        )
        .with_db(Arc::new(self.db.clone_connection()))
        .with_grant_enforcement();

        // Independent approval store: a child pause must never leak into the
        // parent approval chain.
        let child_approval_store = ApprovalStore::new();
        let (tx, _rx) = tokio::sync::mpsc::channel(256);
        let scope = crate::agent::delegation::ExecutionScope {
            cancel: cancel_token.clone(),
            budget: Some(Arc::clone(&budget)),
            handoff_id: Some(handoff_id.clone()),
            goal_id: inherited.as_ref().and_then(|value| value.goal_id.clone()),
            goal_version: inherited.as_ref().and_then(|value| value.goal_version),
            evidence_ids: handoff
                .map(|request| request.input_evidence_ids.clone())
                .unwrap_or_default(),
            deadline: Some(deadline),
        };
        let execution = engine::run_react_loop_with_channel(
            &mut state,
            &client,
            &child_registry,
            &child_approval_store,
            &security_gateway,
            &child_config,
            &conversation_id,
            &cancel_token,
            &tx,
            &self.log_buffer,
        );
        let result = crate::agent::delegation::EXECUTION_SCOPE
            .scope(scope, async {
                tokio::select! {
                    biased;
                    _=cancel_token.cancelled()=>Err("子任务已取消".into()),
                    result=tokio::time::timeout_at(deadline,execution)=>match result {
                        Ok(value)=>value,
                        Err(_)=>{cancel_token.cancel();Err("子任务执行超时".into())},
                    }
                }
            })
            .await;
        let child_usage = budget.handoff_usage(&handoff_id).ok();
        drop(reservation);

        match result {
            Ok(engine::RunOutcome::Done { output }) => {
                match build_delegation_result(
                    &output,
                    handoff,
                    &self.db,
                    &self.workspace_root,
                    &handoff_id,
                    inherited.as_ref(),
                    child_usage,
                    started.elapsed(),
                ) {
                    Ok((content, result)) => bounded_subagent_success(&content).with_structured(
                        Some(serde_json::to_value(result).unwrap_or(serde_json::Value::Null)),
                    ),
                    Err(error) => ToolResult::error(error),
                }
            }
            Ok(engine::RunOutcome::Paused { .. }) => ToolResult::error(
                "Subagent requested a nested approval; nested approvals are not supported",
            ),
            Err(error) => ToolResult::error(format!(
                "Subagent execution failed: {}",
                crate::utils::text::truncate_chars(&error, 1000)
            )),
        }
    }
}

/// Fail-closed validation before any LLM call. Currently only the workdir
/// override is rejected; model-name override is allowed.
fn validate_runtime_spec(spec: &SubagentExecutionSpec) -> Result<(), String> {
    if let Some(workdir) = &spec.workdir {
        if !workdir.trim().is_empty() {
            return Err("Subagent workdir override is not supported yet".to_string());
        }
    }
    Ok(())
}

fn validate_handoff_scope(
    request: &crate::agent::delegation::DelegationRequest,
    scope: Option<&crate::agent::delegation::ExecutionScope>,
) -> Result<(), String> {
    request.validate()?;
    if request.goal_id.is_some()
        && scope.and_then(|value| value.goal_id.as_ref()) != request.goal_id.as_ref()
    {
        return Err(
            "ProofLoop goal context is unavailable or does not match this execution".into(),
        );
    }
    if request.goal_version.is_some()
        && scope.and_then(|value| value.goal_version) != request.goal_version
    {
        return Err("ProofLoop goal version does not match this execution".into());
    }
    let available_evidence = scope
        .map(|value| value.evidence_ids.as_slice())
        .unwrap_or_default();
    if request
        .input_evidence_ids
        .iter()
        .any(|id| !available_evidence.contains(id))
    {
        return Err("Handoff evidence ids must come from the active execution evidence".into());
    }
    Ok(())
}

fn effective_child_tools(
    spec: &SubagentExecutionSpec,
    handoff: Option<&crate::agent::delegation::DelegationRequest>,
    source: &ToolRegistry,
) -> Result<Vec<String>, String> {
    let requested = handoff
        .map(|request| request.allowed_tools.as_slice())
        .unwrap_or(&spec.allowed_tools);
    let mut tools = Vec::with_capacity(requested.len());
    for name in requested {
        if !spec.allowed_tools.iter().any(|allowed| allowed == name) {
            return Err(format!(
                "Handoff requested tool outside the configured allowlist: {name}"
            ));
        }
        if name.starts_with("subagent_") {
            return Err("Nested subagent tools are not supported".into());
        }
        if source.get(name).is_none() {
            return Err(format!("Handoff requested unavailable tool: {name}"));
        }
        tools.push(name.clone());
    }
    Ok(tools)
}

fn build_child_task(
    task: &str,
    handoff: Option<&crate::agent::delegation::DelegationRequest>,
) -> String {
    let Some(request) = handoff else {
        return task.to_string();
    };
    format!(
        "{task}\n\n## Explicit handoff context\nrole: {:?}\ninput evidence ids: {}\nrequired artifact names: {}",
        request.role,
        serde_json::to_string(&request.input_evidence_ids).unwrap_or_else(|_|"[]".into()),
        serde_json::to_string(&request.required_artifact_names).unwrap_or_else(|_|"[]".into()),
    )
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ChildHandoffOutput {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    artifact_ids: Vec<String>,
    #[serde(default)]
    artifacts: Vec<ChildArtifactClaim>,
    #[serde(default)]
    evidence_ids: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ChildArtifactClaim {
    name: String,
    path: String,
    #[serde(default)]
    artifact_id: Option<String>,
}

fn resolve_handoff_artifact_path(path: &str, workspace_root: &str) -> Result<String, String> {
    if path.trim().is_empty()
        || path != path.trim()
        || path.len() > 4096
        || path.chars().any(char::is_control)
    {
        return Err("Child handback contains an invalid artifact path".into());
    }
    let root = std::path::Path::new(workspace_root)
        .canonicalize()
        .map_err(|_| "Workspace root is unavailable")?;
    let reported = std::path::Path::new(path);
    let candidate = if reported.is_absolute() {
        reported.to_path_buf()
    } else {
        root.join(reported)
    };
    ArtifactService::validate_path(&candidate.to_string_lossy(), Some(&root.to_string_lossy()))
        .map_err(|_| "Child artifact path escapes the active workspace")?;
    let canonical = candidate
        .canonicalize()
        .map_err(|_| "Child artifact path does not exist")?;
    if canonical == root || !canonical.starts_with(&root) || !canonical.is_file() {
        return Err("Child artifact path is not a file inside the active workspace".into());
    }
    Ok(canonical.to_string_lossy().into_owned())
}

fn build_delegation_result(
    output: &str,
    handoff: Option<&crate::agent::delegation::DelegationRequest>,
    db: &Database,
    workspace_root: &str,
    handoff_id: &str,
    scope: Option<&crate::agent::delegation::ExecutionScope>,
    usage: Option<(u32, Option<u64>)>,
    elapsed: std::time::Duration,
) -> Result<(String, crate::agent::delegation::DelegationResult), String> {
    let parsed = if handoff.is_some() {
        serde_json::from_str::<ChildHandoffOutput>(output).ok()
    } else {
        None
    };
    if handoff.is_some_and(|request| !request.required_artifact_names.is_empty())
        && parsed.is_none()
    {
        return Err(
            "required_artifact_missing: child did not return a structured artifact handback".into(),
        );
    }
    let (summary, claimed_artifacts, claimed_evidence, child_artifacts) = match parsed {
        Some(value) => (
            value.summary,
            value.artifact_ids,
            value.evidence_ids,
            value.artifacts,
        ),
        None => (output.to_string(), Vec::new(), Vec::new(), Vec::new()),
    };
    if claimed_artifacts.len() > 16 || claimed_evidence.len() > 32 || child_artifacts.len() > 16 {
        return Err("Child artifact or evidence handback exceeds bounded limits".into());
    }
    let artifact_service = ArtifactService::new(db.clone_connection());
    let mut verified_artifacts = Vec::with_capacity(claimed_artifacts.len());
    let mut verified_paths = Vec::new();
    let mut verified_names = std::collections::HashSet::new();
    let mut seen_ids = std::collections::HashSet::new();
    for raw_id in claimed_artifacts {
        if !seen_ids.insert(raw_id.clone()) {
            return Err("Child handback contains a duplicate artifact id".into());
        }
        let id = ArtifactId::new(raw_id)
            .map_err(|_| "Child handback contains an invalid artifact id")?;
        let (artifact, _bytes) = artifact_service
            .read_bytes(&id, workspace_root)
            .map_err(|_| "Child handback references an artifact that could not be verified")?;
        verified_names.insert(artifact.name.clone());
        verified_artifacts.push(artifact.id.as_str().to_string());
        let path = artifact
            .path
            .as_deref()
            .ok_or("Verified artifact has no stored path")?;
        let canonical = resolve_handoff_artifact_path(path, workspace_root)?;
        if !verified_paths.contains(&canonical) {
            verified_paths.push(canonical);
        }
    }
    for claim in &child_artifacts {
        if claim.name.trim().is_empty()
            || claim.name.as_str() != claim.name.trim()
            || claim.name.chars().count() > 200
        {
            return Err("Child handback contains an invalid artifact name".into());
        }
        let canonical = resolve_handoff_artifact_path(&claim.path, workspace_root)?;
        if let Some(raw_id) = &claim.artifact_id {
            let id = ArtifactId::new(raw_id.clone())
                .map_err(|_| "Child handback contains an invalid artifact id")?;
            let (artifact, _bytes) = artifact_service
                .read_bytes(&id, workspace_root)
                .map_err(|_| "Child handback references an artifact that could not be verified")?;
            let stored = artifact
                .path
                .as_deref()
                .ok_or("Verified artifact has no stored path")?;
            let stored = resolve_handoff_artifact_path(stored, workspace_root)?;
            if artifact.name != claim.name || stored != canonical {
                return Err("Child artifact claim does not match its registered artifact".into());
            }
            verified_names.insert(artifact.name.clone());
            if seen_ids.insert(raw_id.clone()) {
                verified_artifacts.push(artifact.id.as_str().to_string());
            }
        }
        verified_names.insert(claim.name.clone());
        if !verified_paths.contains(&canonical) {
            verified_paths.push(canonical);
        }
    }
    if let Some(request) = handoff {
        for required in &request.required_artifact_names {
            if !verified_names.contains(required) {
                return Err(format!("required_artifact_missing: {}", required));
            }
        }
        let supplied = request
            .input_evidence_ids
            .iter()
            .collect::<std::collections::HashSet<_>>();
        if claimed_evidence.iter().any(|id| !supplied.contains(id)) {
            return Err("Child returned an evidence id that was not explicitly supplied".into());
        }
    }
    let goal_id = scope.and_then(|value| value.goal_id.clone());
    let goal_version = scope.and_then(|value| value.goal_version);
    let (model_calls, total_tokens) = usage.unwrap_or((0, None));
    let result = crate::agent::delegation::DelegationResult {
        handoff_id: handoff_id.to_string(),
        goal_id,
        goal_version,
        status: "completed_unverified".into(),
        artifact_ids: verified_artifacts.clone(),
        artifact_paths: verified_paths.clone(),
        evidence_ids: claimed_evidence,
        model_calls,
        total_tokens,
        elapsed_ms: elapsed.as_millis().min(u64::MAX as u128) as u64,
    };
    let content = if verified_artifacts.is_empty() && verified_paths.is_empty() {
        summary
    } else {
        format!(
            "{summary}\n\n已核验的产物路径：{}\n已核验的 artifact_id：{}",
            verified_paths.join(", "),
            verified_artifacts.join(", ")
        )
    };
    Ok((content, result))
}

/// Copy the parent config but allow only the model *name* to be overridden by
/// the subagent definition. Provider / base_url / keys / sandbox are inherited.
fn build_child_config(parent: &AppConfig, model: Option<&str>) -> AppConfig {
    let mut child = parent.clone();
    if let Some(model) = model {
        let model = model.trim();
        if !model.is_empty() {
            child.model.name = model.to_string();
        }
    }
    child
}

/// Build a strictly whitelisted child tool registry.
///
/// Starts empty and only exact-copies tools whose names appear in
/// `allowed_tools`. `subagent_*` names are always filtered out (single-layer
/// delegation). Missing tools are skipped.
fn build_child_tool_registry(source: &ToolRegistry, allowed_tools: &[String]) -> Arc<ToolRegistry> {
    let mut child = ToolRegistry::new();
    for allowed_name in allowed_tools {
        if allowed_name.starts_with("subagent_") {
            continue;
        }
        if let Some(tool) = source.get(allowed_name) {
            child.register(Arc::clone(tool));
        }
    }
    Arc::new(child)
}

/// Independent child system prompt. The instructions are a trusted local
/// definition, used as the child's system instructions.
fn build_child_system_prompt(
    name: &str,
    instructions: &str,
    handoff: Option<&crate::agent::delegation::DelegationRequest>,
) -> String {
    let mut prompt = format!(
        "You are the subagent `{name}`.\n\n\
        Runtime constraints:\n\
        - Complete only the delegated task.\n\
        - You may use only tools exposed by the child runtime registry.\n\
        - You must not delegate to another subagent.\n\
        - Nested approvals are not supported.\n\
        - Role labels are descriptive and do not grant capabilities.\n\
        - Do not claim access to tools that are not present.\n\n\
        ## Subagent Instructions\n\n{instructions}"
    );
    if handoff.is_some() {
        prompt.push_str("\n\nFor handoff completion, return one JSON object with `summary`, `artifact_ids`, `artifacts`, and `evidence_ids`. Each `artifacts` item has a required `name` and actual workspace-relative `path`, plus optional `artifact_id` only when a tool returned a registered artifact id. Never invent an id or path. Evidence ids may only be selected from the explicit handoff context. Keep the completion unverified and do not claim the parent acceptance passed.");
    }
    if handoff.is_some_and(|request| !request.required_artifact_names.is_empty()) {
        prompt.push_str(" A required artifact is complete only after an authorized tool has produced it and you can report its registered artifact id.");
    }
    prompt
}

/// Bound a successful child output into a `ToolResult` (UTF-8 safe).
fn bounded_subagent_success(output: &str) -> ToolResult {
    let bounded = crate::utils::text::truncate_chars(output, MAX_SUBAGENT_RESULT_CHARS);
    ToolResult::success(bounded)
}

#[cfg(test)]
mod tests;
