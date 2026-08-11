use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use serde_json::Value;
use thiserror::Error;

use crate::{
    agent::verifier::{DefaultVerifier, VerificationResult, Verifier},
    config::types::SandboxConfig,
    tools::{trait_def::RiskLevel, ToolRegistry, ToolResult},
};

use super::{
    describe_builtin_tool, AuditError, AuditEventInput, AuditEventType, AuditRecorder, BuiltInRole,
    DecisionContext, DescriptorError, PermissionId, PolicyDecision, PolicyEngine,
    ResourceDescriptor, ResourceScope, SafetyPolicy, ToolSecurityDescriptor, POLICY_VERSION,
};

#[derive(Debug)]
pub struct SecurityExecutionRequest {
    pub conversation_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: Value,
}

#[derive(Debug)]
pub enum SecurityExecutionOutcome {
    Executed {
        tool_result: ToolResult,
        verification: VerificationResult,
    },
    RequiresApproval,
    Denied {
        reason: String,
    },
}

#[derive(Debug, Error)]
pub enum SecurityGatewayError {
    #[error(transparent)]
    Descriptor(#[from] DescriptorError),
    #[error(transparent)]
    Audit(#[from] AuditError),
    #[error("tool not found in registry: {0}")]
    ToolNotFound(String),
}

pub struct SecurityExecutionGateway {
    policy_engine: PolicyEngine,
    sandbox_config: SandboxConfig,
    workspace_root: PathBuf,
    tool_registry: Arc<ToolRegistry>,
    verifier: Arc<dyn Verifier>,
    audit_recorder: Option<Arc<AuditRecorder>>,
}

impl SecurityExecutionGateway {
    pub fn new() -> Self {
        let sandbox_config = SandboxConfig::default();
        let workspace_root = sandbox_config.workspace_root();

        Self::with_sandbox(sandbox_config, workspace_root)
    }

    pub fn with_sandbox(sandbox_config: SandboxConfig, workspace_root: impl Into<PathBuf>) -> Self {
        let workspace_root = workspace_root.into();
        let tool_registry = Arc::new(ToolRegistry::with_defaults(
            workspace_root.to_string_lossy().as_ref(),
        ));
        let verifier = Arc::new(DefaultVerifier::new(
            workspace_root.to_string_lossy().as_ref(),
        ));

        Self::with_sandbox_registry_and_verifier(
            sandbox_config,
            workspace_root,
            tool_registry,
            verifier,
        )
    }

    pub fn with_sandbox_and_registry(
        sandbox_config: SandboxConfig,
        workspace_root: impl Into<PathBuf>,
        tool_registry: Arc<ToolRegistry>,
    ) -> Self {
        let workspace_root = workspace_root.into();
        let verifier = Arc::new(DefaultVerifier::new(
            workspace_root.to_string_lossy().as_ref(),
        ));

        Self::with_sandbox_registry_and_verifier(
            sandbox_config,
            workspace_root,
            tool_registry,
            verifier,
        )
    }

    pub fn with_sandbox_registry_and_verifier(
        sandbox_config: SandboxConfig,
        workspace_root: impl Into<PathBuf>,
        tool_registry: Arc<ToolRegistry>,
        verifier: Arc<dyn Verifier>,
    ) -> Self {
        Self::with_sandbox_registry_verifier_and_optional_audit(
            sandbox_config,
            workspace_root,
            tool_registry,
            verifier,
            None,
        )
    }

    pub fn with_sandbox_registry_verifier_and_audit(
        sandbox_config: SandboxConfig,
        workspace_root: impl Into<PathBuf>,
        tool_registry: Arc<ToolRegistry>,
        verifier: Arc<dyn Verifier>,
        audit_recorder: Arc<AuditRecorder>,
    ) -> Self {
        Self::with_sandbox_registry_verifier_and_optional_audit(
            sandbox_config,
            workspace_root,
            tool_registry,
            verifier,
            Some(audit_recorder),
        )
    }

    fn with_sandbox_registry_verifier_and_optional_audit(
        sandbox_config: SandboxConfig,
        workspace_root: impl Into<PathBuf>,
        tool_registry: Arc<ToolRegistry>,
        verifier: Arc<dyn Verifier>,
        audit_recorder: Option<Arc<AuditRecorder>>,
    ) -> Self {
        Self {
            policy_engine: PolicyEngine,
            sandbox_config,
            workspace_root: workspace_root.into(),
            tool_registry,
            verifier,
            audit_recorder,
        }
    }

    pub async fn execute(
        &self,
        request: &SecurityExecutionRequest,
        role: BuiltInRole,
        final_risk: RiskLevel,
    ) -> Result<SecurityExecutionOutcome, SecurityGatewayError> {
        match self.evaluate(request, role, final_risk)? {
            PolicyDecision::Allow(context) => {
                self.record_execution_started(request, &context)?;
                let tool_result = match self
                    .tool_registry
                    .execute(&request.tool_name, request.arguments.clone())
                    .await
                {
                    Some(tool_result) => tool_result,
                    None => {
                        self.record_execution_finished(request, &context, false)?;
                        return Err(SecurityGatewayError::ToolNotFound(
                            request.tool_name.clone(),
                        ));
                    }
                };
                self.record_execution_finished(request, &context, tool_result.ok)?;
                let verification = self
                    .verifier
                    .verify(&request.tool_name, &request.arguments, &tool_result)
                    .await;
                self.record_verification_finished(request, &context, &verification)?;

                Ok(SecurityExecutionOutcome::Executed {
                    tool_result,
                    verification,
                })
            }
            PolicyDecision::RequireApproval(_) => Ok(SecurityExecutionOutcome::RequiresApproval),
            PolicyDecision::Deny(context) => Ok(SecurityExecutionOutcome::Denied {
                reason: context.reason,
            }),
        }
    }

    fn record_policy_decided(
        &self,
        request: &SecurityExecutionRequest,
        decision: &PolicyDecision,
    ) -> Result<(), SecurityGatewayError> {
        let Some(recorder) = &self.audit_recorder else {
            return Ok(());
        };

        let context = decision.context();
        let decision_status = match decision {
            PolicyDecision::Allow(_) => "allow",
            PolicyDecision::RequireApproval(_) => "require_approval",
            PolicyDecision::Deny(_) => "deny",
        };

        recorder.record(AuditEventInput {
            event_type: AuditEventType::PolicyDecided,
            correlation_id: request.tool_call_id.clone(),
            request_id: request.tool_call_id.clone(),
            subject_id: "local-user".to_string(),
            role_key: context.role.as_str().to_string(),
            conversation_id: Some(request.conversation_id.clone()),
            tool_call_id: Some(request.tool_call_id.clone()),
            tool_name: Some(request.tool_name.clone()),
            capabilities: context
                .requested_permissions
                .iter()
                .map(|permission| permission.as_str().to_string())
                .collect(),
            actions: context
                .requested_permissions
                .iter()
                .map(|permission| permission.as_str().to_string())
                .collect(),
            resources: serde_json::json!(context.resource_scopes),
            policy_version: Some(context.policy_version.clone()),
            risk_level: Some(context.risk_level.to_string()),
            decision_status: Some(decision_status.to_string()),
            request: None,
            result: Some(serde_json::json!({ "decision": decision_status })),
            details: serde_json::json!({
                "phase": AuditEventType::PolicyDecided.as_str(),
                "reason": crate::utils::text::truncate_chars(&context.reason, 200),
            }),
            ..Default::default()
        })?;

        Ok(())
    }

    fn record_approval_requested(
        &self,
        request: &SecurityExecutionRequest,
        context: &DecisionContext,
    ) -> Result<(), SecurityGatewayError> {
        let Some(recorder) = &self.audit_recorder else {
            return Ok(());
        };

        recorder.record(AuditEventInput {
            event_type: AuditEventType::ApprovalRequested,
            correlation_id: request.tool_call_id.clone(),
            request_id: request.tool_call_id.clone(),
            subject_id: "local-user".to_string(),
            role_key: context.role.as_str().to_string(),
            conversation_id: Some(request.conversation_id.clone()),
            tool_call_id: Some(request.tool_call_id.clone()),
            tool_name: Some(request.tool_name.clone()),
            capabilities: context
                .requested_permissions
                .iter()
                .map(|permission| permission.as_str().to_string())
                .collect(),
            actions: context
                .requested_permissions
                .iter()
                .map(|permission| permission.as_str().to_string())
                .collect(),
            resources: serde_json::json!(context.resource_scopes),
            policy_version: Some(context.policy_version.clone()),
            risk_level: Some(context.risk_level.to_string()),
            decision_status: Some("require_approval".to_string()),
            request: None,
            result: None,
            details: serde_json::json!({
                "phase": AuditEventType::ApprovalRequested.as_str(),
                "reason": crate::utils::text::truncate_chars(&context.reason, 200),
            }),
            ..Default::default()
        })?;

        Ok(())
    }

    fn record_execution_started(
        &self,
        request: &SecurityExecutionRequest,
        context: &DecisionContext,
    ) -> Result<(), SecurityGatewayError> {
        self.record_execution_event(
            request,
            context,
            AuditEventType::ExecutionStarted,
            None,
            "started",
        )
    }

    fn record_execution_finished(
        &self,
        request: &SecurityExecutionRequest,
        context: &DecisionContext,
        ok: bool,
    ) -> Result<(), SecurityGatewayError> {
        self.record_execution_event(
            request,
            context,
            AuditEventType::ExecutionFinished,
            Some(ok),
            if ok { "succeeded" } else { "failed" },
        )
    }

    fn record_execution_event(
        &self,
        request: &SecurityExecutionRequest,
        context: &DecisionContext,
        event_type: AuditEventType,
        result_ok: Option<bool>,
        status: &str,
    ) -> Result<(), SecurityGatewayError> {
        let Some(recorder) = &self.audit_recorder else {
            return Ok(());
        };

        let capabilities = context
            .requested_permissions
            .iter()
            .map(|permission| permission.as_str().to_string())
            .collect();
        let actions = context
            .requested_permissions
            .iter()
            .map(|permission| permission.as_str().to_string())
            .collect();

        recorder.record(AuditEventInput {
            event_type,
            correlation_id: request.tool_call_id.clone(),
            request_id: request.tool_call_id.clone(),
            subject_id: "local-user".to_string(),
            role_key: context.role.as_str().to_string(),
            conversation_id: Some(request.conversation_id.clone()),
            tool_call_id: Some(request.tool_call_id.clone()),
            tool_name: Some(request.tool_name.clone()),
            capabilities,
            actions,
            resources: serde_json::json!(context.resource_scopes),
            policy_version: Some(context.policy_version.clone()),
            risk_level: Some(context.risk_level.to_string()),
            decision_status: Some(status.to_string()),
            request: None,
            result: result_ok.map(|ok| serde_json::json!({ "ok": ok })),
            details: serde_json::json!({ "phase": event_type.as_str() }),
            ..Default::default()
        })?;

        Ok(())
    }

    fn record_verification_finished(
        &self,
        request: &SecurityExecutionRequest,
        context: &DecisionContext,
        verification: &VerificationResult,
    ) -> Result<(), SecurityGatewayError> {
        let Some(recorder) = &self.audit_recorder else {
            return Ok(());
        };

        let capabilities = context
            .requested_permissions
            .iter()
            .map(|permission| permission.as_str().to_string())
            .collect();
        let actions = context
            .requested_permissions
            .iter()
            .map(|permission| permission.as_str().to_string())
            .collect();

        recorder.record(AuditEventInput {
            event_type: AuditEventType::VerificationFinished,
            correlation_id: request.tool_call_id.clone(),
            request_id: request.tool_call_id.clone(),
            subject_id: "local-user".to_string(),
            role_key: context.role.as_str().to_string(),
            conversation_id: Some(request.conversation_id.clone()),
            tool_call_id: Some(request.tool_call_id.clone()),
            tool_name: Some(request.tool_name.clone()),
            capabilities,
            actions,
            resources: serde_json::json!(context.resource_scopes),
            policy_version: Some(context.policy_version.clone()),
            risk_level: Some(context.risk_level.to_string()),
            decision_status: Some(
                if verification.success {
                    "verified"
                } else {
                    "verification_failed"
                }
                .to_string(),
            ),
            request: None,
            result: Some(serde_json::json!({ "success": verification.success })),
            details: serde_json::json!({
                "phase": AuditEventType::VerificationFinished.as_str(),
                "reason": crate::utils::text::truncate_chars(&verification.reason, 200),
            }),
            ..Default::default()
        })?;

        Ok(())
    }

    pub fn resolve_descriptor(
        &self,
        request: &SecurityExecutionRequest,
    ) -> Result<ToolSecurityDescriptor, DescriptorError> {
        let descriptor = describe_builtin_tool(&request.tool_name, &request.arguments)?;
        descriptor.validate_for_tool(&request.tool_name)?;
        Ok(descriptor)
    }

    fn resolve_resource_scopes(
        &self,
        request: &SecurityExecutionRequest,
        descriptor: &ToolSecurityDescriptor,
    ) -> Result<Vec<ResourceScope>, DescriptorError> {
        descriptor.validate_for_tool(&request.tool_name)?;

        let actual_descriptor = describe_builtin_tool(&request.tool_name, &request.arguments)?;
        if actual_descriptor.resources != descriptor.resources {
            return Err(DescriptorError::InvalidDescriptor(
                "descriptor resources do not match the current tool request".to_string(),
            ));
        }

        let declared_scopes = descriptor
            .requested_permissions
            .iter()
            .map(|requested| requested.scope)
            .collect::<Vec<_>>();
        let mut scopes = Vec::new();

        for resource in &descriptor.resources {
            let scope = match resource {
                ResourceDescriptor::File { path } if !path.trim().is_empty() => {
                    ResourceScope::Workspace
                }
                ResourceDescriptor::Shell { command, .. } if !command.trim().is_empty() => {
                    ResourceScope::ShellCommand
                }
                ResourceDescriptor::Process { action, .. } if !action.trim().is_empty() => {
                    ResourceScope::Process
                }
                ResourceDescriptor::Network { url, method }
                    if !url.trim().is_empty() && !method.trim().is_empty() =>
                {
                    ResourceScope::NetworkTarget
                }
                ResourceDescriptor::NetworkFromResponse {
                    source,
                    target_template,
                    method,
                } if !source.trim().is_empty()
                    && !target_template.trim().is_empty()
                    && !method.trim().is_empty() =>
                {
                    ResourceScope::NetworkTarget
                }
                ResourceDescriptor::Desktop { action, target }
                    if !action.trim().is_empty()
                        && target
                            .as_deref()
                            .map_or(true, |value| !value.trim().is_empty()) =>
                {
                    ResourceScope::DesktopTarget
                }
                ResourceDescriptor::Skill { name } if !name.trim().is_empty() => {
                    ResourceScope::DiscoveredSkill
                }
                ResourceDescriptor::Agent { action } if !action.trim().is_empty() => {
                    ResourceScope::AgentInternal
                }
                _ => {
                    return Err(DescriptorError::InvalidDescriptor(
                        "tool resource cannot be resolved to a security scope".to_string(),
                    ));
                }
            };

            if !declared_scopes.contains(&scope) {
                return Err(DescriptorError::InvalidDescriptor(
                    "resolved resource scope is not declared by the tool descriptor".to_string(),
                ));
            }
            if !scopes.contains(&scope) {
                scopes.push(scope);
            }
        }

        if scopes.is_empty() {
            return Err(DescriptorError::InvalidDescriptor(
                "tool request contains no resolvable resources".to_string(),
            ));
        }

        Ok(scopes)
    }

    fn build_decision_context(
        &self,
        request: &SecurityExecutionRequest,
        descriptor: &ToolSecurityDescriptor,
        role: BuiltInRole,
        resource_scopes: Vec<ResourceScope>,
        risk_level: RiskLevel,
    ) -> Result<DecisionContext, DescriptorError> {
        descriptor.validate_for_tool(&request.tool_name)?;

        let requested_permissions = descriptor
            .requested_permissions
            .iter()
            .map(|requested| requested.permission)
            .collect::<Vec<_>>();
        let declared_scopes = descriptor
            .requested_permissions
            .iter()
            .map(|requested| requested.scope)
            .collect::<Vec<_>>();

        if requested_permissions.is_empty()
            || resource_scopes.is_empty()
            || resource_scopes
                .iter()
                .any(|scope| !declared_scopes.contains(scope))
        {
            return Err(DescriptorError::InvalidDescriptor(
                "security descriptor must declare permissions and resource scopes".to_string(),
            ));
        }

        Ok(DecisionContext {
            role,
            risk_level,
            policy_version: POLICY_VERSION.to_string(),
            requested_permissions,
            resource_scopes,
            reason: format!("tool {} requested security evaluation", request.tool_name),
        })
    }

    fn sandbox_allows_file_write(
        &self,
        descriptor: &ToolSecurityDescriptor,
    ) -> Result<Option<bool>, DescriptorError> {
        if !descriptor
            .requested_permissions
            .iter()
            .any(|requested| requested.permission == PermissionId::FilesystemWrite)
        {
            return Ok(None);
        }

        let file_paths = descriptor
            .resources
            .iter()
            .filter_map(|resource| match resource {
                ResourceDescriptor::File { path } if !path.trim().is_empty() => Some(path),
                _ => None,
            })
            .collect::<Vec<_>>();

        if file_paths.is_empty() {
            return Err(DescriptorError::InvalidDescriptor(
                "filesystem write permission requires a non-empty file resource".to_string(),
            ));
        }

        file_paths
            .into_iter()
            .map(|path| {
                crate::safety::can_write(
                    &self.sandbox_config,
                    &self.workspace_root,
                    Path::new(path),
                )
                .map_err(|error| DescriptorError::InvalidDescriptor(error.to_string()))
            })
            .try_fold(true, |allowed, write_allowed| {
                write_allowed.map(|write_allowed| allowed && write_allowed)
            })
            .map(Some)
    }

    pub fn evaluate(
        &self,
        request: &SecurityExecutionRequest,
        role: BuiltInRole,
        final_risk: RiskLevel,
    ) -> Result<PolicyDecision, SecurityGatewayError> {
        let descriptor = self.resolve_descriptor(request)?;
        let resource_scopes = self.resolve_resource_scopes(request, &descriptor)?;
        let sandbox_allows_file_write = self.sandbox_allows_file_write(&descriptor)?;
        let assessed_risk = SafetyPolicy::assess(
            &request.tool_name,
            descriptor.default_risk,
            &request.arguments,
        );
        let mut context = self.build_decision_context(
            request,
            &descriptor,
            role,
            resource_scopes,
            assessed_risk.max(final_risk),
        )?;

        let decision = if sandbox_allows_file_write == Some(false) {
            context.reason = format!(
                "sandbox denied tool {} file write request",
                request.tool_name
            );
            PolicyDecision::Deny(context)
        } else {
            // PolicyEngine currently accepts raw role/descriptor/risk inputs and
            // rebuilds its own DecisionContext. Until that API accepts a context
            // directly, this gateway validates the canonical context first and
            // forwards its role and risk through the existing policy boundary.
            // PolicyEngine currently exposes a static evaluation API; retain it as
            // the gateway's explicit policy dependency while forwarding to that API.
            let _policy_engine = &self.policy_engine;
            PolicyEngine::evaluate(
                context.role,
                &request.tool_name,
                &descriptor,
                context.risk_level,
            )
        };

        self.record_policy_decided(request, &decision)?;
        if let PolicyDecision::RequireApproval(context) = &decision {
            self.record_approval_requested(request, context)?;
        }
        Ok(decision)
    }
}

#[cfg(test)]
mod tests {
    use super::{SecurityExecutionGateway, SecurityExecutionOutcome, SecurityExecutionRequest};
    use crate::agent::verifier::{DefaultVerifier, VerificationResult, Verifier};
    use crate::config::types::{SandboxConfig, SandboxProfile};
    use crate::db::{Database, SecurityAuditQuery};
    use crate::safety::{
        AuditRecorder, BuiltInRole, DescriptorError, PermissionId, PolicyDecision,
        ResourceDescriptor, ResourceScope, ToolSecurityDescriptor,
    };
    use crate::tools::trait_def::RiskLevel;
    use crate::tools::{Tool, ToolRegistry, ToolResult};
    use async_trait::async_trait;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    struct CountingTool {
        name: &'static str,
        executions: Arc<AtomicUsize>,
    }

    struct FailingTool;

    struct CountingVerifier {
        verifications: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Verifier for CountingVerifier {
        async fn verify(
            &self,
            _tool_name: &str,
            _args: &serde_json::Value,
            _tool_result: &ToolResult,
        ) -> VerificationResult {
            self.verifications.fetch_add(1, Ordering::SeqCst);
            VerificationResult::success("counted", None)
        }
    }

    #[async_trait]
    impl Tool for CountingTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "counts executions for gateway tests"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }

        async fn execute(&self, _args: serde_json::Value) -> ToolResult {
            self.executions.fetch_add(1, Ordering::SeqCst);
            ToolResult::success("executed")
        }
    }

    #[async_trait]
    impl Tool for FailingTool {
        fn name(&self) -> &str {
            "read_file"
        }

        fn description(&self) -> &str {
            "returns a failed result for gateway audit tests"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }

        async fn execute(&self, _args: serde_json::Value) -> ToolResult {
            ToolResult::error("simulated tool failure")
        }
    }

    fn registry_with_counting_tool(name: &'static str) -> (Arc<ToolRegistry>, Arc<AtomicUsize>) {
        let executions = Arc::new(AtomicUsize::new(0));
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(CountingTool {
            name,
            executions: Arc::clone(&executions),
        }));
        (Arc::new(registry), executions)
    }

    fn registry_with_failing_tool() -> Arc<ToolRegistry> {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(FailingTool));
        Arc::new(registry)
    }

    fn counting_verifier() -> (Arc<dyn Verifier>, Arc<AtomicUsize>) {
        let verifications = Arc::new(AtomicUsize::new(0));
        (
            Arc::new(CountingVerifier {
                verifications: Arc::clone(&verifications),
            }),
            verifications,
        )
    }

    fn request(tool_name: &str, arguments: serde_json::Value) -> SecurityExecutionRequest {
        SecurityExecutionRequest {
            conversation_id: "conversation-1".to_string(),
            tool_call_id: "call-1".to_string(),
            tool_name: tool_name.to_string(),
            arguments,
        }
    }

    fn audit_recorder(label: &str) -> (Arc<AuditRecorder>, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "yilian-gateway-audit-{label}-{}.db",
            uuid::Uuid::new_v4()
        ));
        let db = Database::new(&path).unwrap();
        (Arc::new(AuditRecorder::new(db.clone_connection())), path)
    }

    fn policy_event(recorder: &AuditRecorder, tool_call_id: &str) -> crate::db::SecurityAuditEvent {
        let events = recorder
            .query(&SecurityAuditQuery {
                correlation_id: Some(tool_call_id.to_string()),
                event_type: Some("policy_decided".to_string()),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(events.len(), 1);
        events.into_iter().next().unwrap()
    }

    fn approval_events(
        recorder: &AuditRecorder,
        tool_call_id: &str,
    ) -> Vec<crate::db::SecurityAuditEvent> {
        recorder
            .query(&SecurityAuditQuery {
                correlation_id: Some(tool_call_id.to_string()),
                event_type: Some("approval_requested".to_string()),
                ..Default::default()
            })
            .unwrap()
    }

    fn sandbox_config(
        profile: SandboxProfile,
        writable_paths: &[&str],
        denied_write_paths: &[&str],
    ) -> SandboxConfig {
        SandboxConfig {
            profile,
            writable_paths: writable_paths
                .iter()
                .map(|path| (*path).to_string())
                .collect(),
            denied_write_paths: denied_write_paths
                .iter()
                .map(|path| (*path).to_string())
                .collect(),
        }
    }

    #[test]
    fn gateway_can_be_constructed() {
        let _gateway = SecurityExecutionGateway::new();
    }

    #[test]
    fn gateway_resolves_descriptor_for_known_tool() {
        let gateway = SecurityExecutionGateway::new();
        let request = request("read_file", serde_json::json!({"path": "README.md"}));

        let descriptor = gateway.resolve_descriptor(&request).unwrap();

        assert_eq!(descriptor.tool_name, "read_file");
    }

    #[test]
    fn gateway_rejects_unknown_tool_descriptor() {
        let gateway = SecurityExecutionGateway::new();
        let request = request("definitely_not_a_tool", serde_json::json!({}));

        let result = gateway.resolve_descriptor(&request);

        assert!(matches!(
            result,
            Err(DescriptorError::UnknownTool(tool)) if tool == "definitely_not_a_tool"
        ));
    }

    #[test]
    fn gateway_resolves_file_resource_scope_from_request() {
        let gateway = SecurityExecutionGateway::new();
        let request = request("read_file", serde_json::json!({"path": "./README.md"}));
        let descriptor = gateway.resolve_descriptor(&request).unwrap();

        let scopes = gateway
            .resolve_resource_scopes(&request, &descriptor)
            .unwrap();

        assert_eq!(scopes, vec![ResourceScope::Workspace]);
    }

    #[test]
    fn gateway_rejects_missing_resource_argument() {
        let gateway = SecurityExecutionGateway::new();
        let descriptor_request = request("read_file", serde_json::json!({"path": "./README.md"}));
        let descriptor = gateway.resolve_descriptor(&descriptor_request).unwrap();
        let request_without_path = request("read_file", serde_json::json!({}));

        let result = gateway.resolve_resource_scopes(&request_without_path, &descriptor);

        assert!(matches!(
            result,
            Err(DescriptorError::MissingArgument { tool, argument })
                if tool == "read_file" && argument == "path"
        ));
    }

    #[test]
    fn gateway_rejects_resource_type_mismatch() {
        let gateway = SecurityExecutionGateway::new();
        let request = request("read_file", serde_json::json!({"path": "./README.md"}));
        let descriptor = ToolSecurityDescriptor {
            tool_name: "read_file".to_string(),
            requested_permissions: vec![
                PermissionId::FilesystemRead.in_scope(ResourceScope::Workspace)
            ],
            resources: vec![ResourceDescriptor::Shell {
                command: "pwd".to_string(),
                working_directory: None,
            }],
            default_risk: RiskLevel::Low,
            side_effects: vec![],
        };

        let result = gateway.resolve_resource_scopes(&request, &descriptor);

        assert!(matches!(result, Err(DescriptorError::InvalidDescriptor(_))));
    }

    #[test]
    fn gateway_builds_decision_context_from_descriptor() {
        let gateway = SecurityExecutionGateway::new();
        let request = request("read_file", serde_json::json!({"path": "README.md"}));
        let descriptor = gateway.resolve_descriptor(&request).unwrap();
        let resource_scopes = gateway
            .resolve_resource_scopes(&request, &descriptor)
            .unwrap();

        let context = gateway
            .build_decision_context(
                &request,
                &descriptor,
                BuiltInRole::Standard,
                resource_scopes,
                RiskLevel::Low,
            )
            .unwrap();

        assert_eq!(context.role, BuiltInRole::Standard);
        assert_eq!(context.risk_level, RiskLevel::Low);
        assert_eq!(
            context.requested_permissions,
            vec![PermissionId::FilesystemRead]
        );
        assert_eq!(context.resource_scopes, vec![ResourceScope::Workspace]);
        assert!(context.reason.contains("read_file"));
    }

    #[test]
    fn gateway_rejects_descriptor_missing_security_information() {
        let gateway = SecurityExecutionGateway::new();
        let request = request("read_file", serde_json::json!({"path": "README.md"}));
        let descriptor = ToolSecurityDescriptor {
            tool_name: "read_file".to_string(),
            requested_permissions: vec![],
            resources: vec![],
            default_risk: RiskLevel::Low,
            side_effects: vec![],
        };

        let result = gateway.build_decision_context(
            &request,
            &descriptor,
            BuiltInRole::Standard,
            vec![],
            RiskLevel::Low,
        );

        assert!(matches!(result, Err(DescriptorError::InvalidDescriptor(_))));
    }

    #[test]
    fn gateway_keeps_dynamic_risk_low_for_ordinary_read() {
        let gateway = SecurityExecutionGateway::new();
        let request = request("read_file", serde_json::json!({"path": "README.md"}));

        let decision = gateway
            .evaluate(&request, BuiltInRole::Standard, RiskLevel::Low)
            .unwrap();

        assert_eq!(decision.context().risk_level, RiskLevel::Low);
    }

    #[test]
    fn gateway_promotes_sensitive_write_to_high_via_safety_policy() {
        let gateway = SecurityExecutionGateway::new();
        let request = request(
            "write_file",
            serde_json::json!({
                "path": "C:\\Windows\\System32\\hosts",
                "content": "blocked"
            }),
        );

        let decision = gateway
            .evaluate(&request, BuiltInRole::Standard, RiskLevel::Medium)
            .unwrap();

        assert_eq!(decision.context().risk_level, RiskLevel::High);
    }

    #[test]
    fn gateway_forwards_allow_decision_from_policy_engine() {
        let gateway = SecurityExecutionGateway::new();
        let request = request(
            "write_file",
            serde_json::json!({"path": "notes.txt", "content": "hello"}),
        );
        let decision = gateway
            .evaluate(&request, BuiltInRole::Standard, RiskLevel::Medium)
            .unwrap();

        assert!(matches!(decision, PolicyDecision::Allow(_)));
    }

    #[test]
    fn gateway_forwards_approval_decision_from_policy_engine() {
        let gateway = SecurityExecutionGateway::new();
        let request = request(
            "bash",
            serde_json::json!({"command": "git push origin develop"}),
        );
        let decision = gateway
            .evaluate(&request, BuiltInRole::Owner, RiskLevel::High)
            .unwrap();

        assert!(matches!(decision, PolicyDecision::RequireApproval(_)));
    }

    #[test]
    fn gateway_forwards_deny_decision_from_policy_engine() {
        let gateway = SecurityExecutionGateway::new();
        let request = request(
            "write_file",
            serde_json::json!({"path": "notes.txt", "content": "hello"}),
        );
        let decision = gateway
            .evaluate(&request, BuiltInRole::Restricted, RiskLevel::Medium)
            .unwrap();

        assert!(matches!(decision, PolicyDecision::Deny(_)));
    }

    #[test]
    fn evaluate_allow_records_policy_decided() {
        let (recorder, db_path) = audit_recorder("policy-allow");
        let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
            Arc::new(ToolRegistry::new()),
            Arc::new(DefaultVerifier::new("workspace")),
            Arc::clone(&recorder),
        );
        let request = request("read_file", serde_json::json!({"path": "README.md"}));

        let decision = gateway
            .evaluate(&request, BuiltInRole::Standard, RiskLevel::Low)
            .unwrap();

        assert!(matches!(decision, PolicyDecision::Allow(_)));
        let event = policy_event(&recorder, &request.tool_call_id);
        assert_eq!(event.decision_status.as_deref(), Some("allow"));
        assert_eq!(event.risk_level.as_deref(), Some("low"));
        assert_eq!(event.conversation_id.as_deref(), Some("conversation-1"));
        assert_eq!(event.tool_name.as_deref(), Some("read_file"));
        assert!(!serde_json::to_string(&event).unwrap().contains("README.md"));
        assert!(approval_events(&recorder, &request.tool_call_id).is_empty());
        drop(gateway);
        drop(recorder);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn evaluate_approval_records_policy_and_approval_requested() {
        let (recorder, db_path) = audit_recorder("policy-approval");
        let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
            Arc::new(ToolRegistry::new()),
            Arc::new(DefaultVerifier::new("workspace")),
            Arc::clone(&recorder),
        );
        let request = request(
            "bash",
            serde_json::json!({"command": "git push origin develop"}),
        );

        let decision = gateway
            .evaluate(&request, BuiltInRole::Owner, RiskLevel::High)
            .unwrap();

        assert!(matches!(decision, PolicyDecision::RequireApproval(_)));
        assert_eq!(
            policy_event(&recorder, &request.tool_call_id)
                .decision_status
                .as_deref(),
            Some("require_approval")
        );
        let approval_events = approval_events(&recorder, &request.tool_call_id);
        assert_eq!(approval_events.len(), 1);
        let approval_event = &approval_events[0];
        assert_eq!(
            approval_event.conversation_id.as_deref(),
            Some("conversation-1")
        );
        assert_eq!(approval_event.tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(approval_event.tool_name.as_deref(), Some("bash"));
        assert_eq!(approval_event.risk_level.as_deref(), Some("high"));
        drop(gateway);
        drop(recorder);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn evaluate_policy_deny_records_policy_decided() {
        let (recorder, db_path) = audit_recorder("policy-deny");
        let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            sandbox_config(SandboxProfile::WorkspaceWrite, &[], &[]),
            "workspace",
            Arc::new(ToolRegistry::new()),
            Arc::new(DefaultVerifier::new("workspace")),
            Arc::clone(&recorder),
        );
        let request = request(
            "write_file",
            serde_json::json!({"path": "notes.txt", "content": "secret"}),
        );

        let decision = gateway
            .evaluate(&request, BuiltInRole::Restricted, RiskLevel::Medium)
            .unwrap();

        assert!(matches!(decision, PolicyDecision::Deny(_)));
        assert_eq!(
            policy_event(&recorder, &request.tool_call_id)
                .decision_status
                .as_deref(),
            Some("deny")
        );
        assert!(approval_events(&recorder, &request.tool_call_id).is_empty());
        drop(gateway);
        drop(recorder);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn evaluate_sandbox_deny_records_policy_decided() {
        let (recorder, db_path) = audit_recorder("sandbox-deny");
        let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
            Arc::new(ToolRegistry::new()),
            Arc::new(DefaultVerifier::new("workspace")),
            Arc::clone(&recorder),
        );
        let request = request(
            "write_file",
            serde_json::json!({"path": "notes.txt", "content": "secret"}),
        );

        let decision = gateway
            .evaluate(&request, BuiltInRole::Standard, RiskLevel::Medium)
            .unwrap();

        assert!(matches!(decision, PolicyDecision::Deny(_)));
        assert!(decision.context().reason.contains("sandbox denied"));
        assert_eq!(
            policy_event(&recorder, &request.tool_call_id)
                .decision_status
                .as_deref(),
            Some("deny")
        );
        assert!(approval_events(&recorder, &request.tool_call_id).is_empty());
        drop(gateway);
        drop(recorder);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn gateway_sandbox_workspace_write_allows_workspace_write_file() {
        let gateway = SecurityExecutionGateway::with_sandbox(
            sandbox_config(SandboxProfile::WorkspaceWrite, &[], &[]),
            "workspace",
        );
        let request = request(
            "write_file",
            serde_json::json!({"path": "notes.txt", "content": "hello"}),
        );

        let decision = gateway
            .evaluate(&request, BuiltInRole::Standard, RiskLevel::Medium)
            .unwrap();

        assert!(matches!(decision, PolicyDecision::Allow(_)));
    }

    #[test]
    fn gateway_sandbox_workspace_write_denies_outside_write_file() {
        let gateway = SecurityExecutionGateway::with_sandbox(
            sandbox_config(SandboxProfile::WorkspaceWrite, &[], &[]),
            "workspace",
        );
        let outside_path = std::env::temp_dir()
            .join("yilian-gateway-sandbox-outside")
            .join("notes.txt");
        let request = request(
            "write_file",
            serde_json::json!({"path": outside_path, "content": "hello"}),
        );

        let decision = gateway
            .evaluate(&request, BuiltInRole::Standard, RiskLevel::Medium)
            .unwrap();

        assert!(matches!(decision, PolicyDecision::Deny(_)));
    }

    #[test]
    fn gateway_sandbox_read_only_denies_write_file() {
        let gateway = SecurityExecutionGateway::with_sandbox(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
        );
        let request = request(
            "write_file",
            serde_json::json!({"path": "notes.txt", "content": "hello"}),
        );

        let decision = gateway
            .evaluate(&request, BuiltInRole::Standard, RiskLevel::Medium)
            .unwrap();

        assert!(matches!(decision, PolicyDecision::Deny(_)));
    }

    #[test]
    fn gateway_sandbox_read_only_allows_read_file() {
        let gateway = SecurityExecutionGateway::with_sandbox(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
        );
        let request = request("read_file", serde_json::json!({"path": "README.md"}));

        let decision = gateway
            .evaluate(&request, BuiltInRole::Standard, RiskLevel::Low)
            .unwrap();

        assert!(matches!(decision, PolicyDecision::Allow(_)));
    }

    #[test]
    fn gateway_sandbox_deny_preserves_final_risk_and_reason() {
        let gateway = SecurityExecutionGateway::with_sandbox(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
        );
        let request = request(
            "write_file",
            serde_json::json!({"path": "notes.txt", "content": "hello"}),
        );

        let decision = gateway
            .evaluate(&request, BuiltInRole::Standard, RiskLevel::Critical)
            .unwrap();

        assert!(matches!(decision, PolicyDecision::Deny(_)));
        assert_eq!(decision.context().risk_level, RiskLevel::Critical);
        assert!(decision.context().reason.contains("sandbox denied"));
        assert!(decision.context().reason.contains("write_file"));
    }

    #[test]
    fn gateway_sandbox_custom_allows_edit_file_in_writable_src() {
        let gateway = SecurityExecutionGateway::with_sandbox(
            sandbox_config(SandboxProfile::Custom, &["src"], &[]),
            "workspace",
        );
        let request = request(
            "edit_file",
            serde_json::json!({"path": "src/main.rs", "content": "fn main() {}"}),
        );

        let decision = gateway
            .evaluate(&request, BuiltInRole::Standard, RiskLevel::Medium)
            .unwrap();

        assert!(matches!(decision, PolicyDecision::Allow(_)));
    }

    #[test]
    fn gateway_sandbox_custom_denied_path_overrides_writable_src() {
        let gateway = SecurityExecutionGateway::with_sandbox(
            sandbox_config(SandboxProfile::Custom, &["src"], &["src/private"]),
            "workspace",
        );
        let request = request(
            "write_file",
            serde_json::json!({"path": "src/private/secret.txt", "content": "secret"}),
        );

        let decision = gateway
            .evaluate(&request, BuiltInRole::Standard, RiskLevel::Medium)
            .unwrap();

        assert!(matches!(decision, PolicyDecision::Deny(_)));
    }

    #[tokio::test]
    async fn execute_allow_runs_tool_once() {
        let (registry, executions) = registry_with_counting_tool("read_file");
        let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
            registry,
        );
        let request = request("read_file", serde_json::json!({"path": "README.md"}));

        let outcome = gateway
            .execute(&request, BuiltInRole::Standard, RiskLevel::Low)
            .await
            .unwrap();

        assert!(matches!(outcome, SecurityExecutionOutcome::Executed { .. }));
        assert_eq!(executions.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn execute_approval_does_not_run_tool() {
        let (registry, executions) = registry_with_counting_tool("bash");
        let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
            registry,
        );
        let request = request(
            "bash",
            serde_json::json!({"command": "git push origin develop"}),
        );

        let outcome = gateway
            .execute(&request, BuiltInRole::Owner, RiskLevel::High)
            .await
            .unwrap();

        assert!(matches!(
            outcome,
            SecurityExecutionOutcome::RequiresApproval
        ));
        assert_eq!(executions.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn execute_deny_does_not_run_tool() {
        let (registry, executions) = registry_with_counting_tool("write_file");
        let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
            registry,
        );
        let request = request(
            "write_file",
            serde_json::json!({"path": "notes.txt", "content": "hello"}),
        );

        let outcome = gateway
            .execute(&request, BuiltInRole::Standard, RiskLevel::Medium)
            .await
            .unwrap();

        assert!(matches!(outcome, SecurityExecutionOutcome::Denied { .. }));
        assert_eq!(executions.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn execute_allow_records_execution_and_verification_audit() {
        let (registry, executions) = registry_with_counting_tool("read_file");
        let (verifier, verifications) = counting_verifier();
        let (recorder, db_path) = audit_recorder("allow");
        let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
            registry,
            verifier,
            Arc::clone(&recorder),
        );
        let request = request("read_file", serde_json::json!({"path": "README.md"}));

        let outcome = gateway
            .execute(&request, BuiltInRole::Standard, RiskLevel::Low)
            .await
            .unwrap();

        assert!(matches!(outcome, SecurityExecutionOutcome::Executed { .. }));
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        assert_eq!(verifications.load(Ordering::SeqCst), 1);

        let events = recorder
            .query(&SecurityAuditQuery {
                correlation_id: Some(request.tool_call_id.clone()),
                ..Default::default()
            })
            .unwrap();
        let event_types = events
            .iter()
            .map(|event| event.event_type.as_str())
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 4);
        assert!(event_types.contains(&"policy_decided"));
        assert!(event_types.contains(&"execution_started"));
        assert!(event_types.contains(&"execution_finished"));
        assert!(event_types.contains(&"verification_finished"));
        let verification = events
            .iter()
            .find(|event| event.event_type == "verification_finished")
            .expect("verification audit event");
        assert_eq!(verification.details["result"]["success"], true);
        assert!(serde_json::to_string(&events)
            .unwrap()
            .find("README.md")
            .is_none());
        drop(gateway);
        drop(recorder);
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn execute_failed_tool_still_records_finished_audit() {
        let (verifier, verifications) = counting_verifier();
        let (recorder, db_path) = audit_recorder("tool-failure");
        let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
            registry_with_failing_tool(),
            verifier,
            Arc::clone(&recorder),
        );
        let request = request("read_file", serde_json::json!({"path": "README.md"}));

        let outcome = gateway
            .execute(&request, BuiltInRole::Standard, RiskLevel::Low)
            .await
            .unwrap();

        match outcome {
            SecurityExecutionOutcome::Executed { tool_result, .. } => assert!(!tool_result.ok),
            _ => panic!("expected executed outcome"),
        }
        assert_eq!(verifications.load(Ordering::SeqCst), 1);

        let events = recorder
            .query(&SecurityAuditQuery {
                correlation_id: Some(request.tool_call_id),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(events.len(), 4);
        let finished = events
            .iter()
            .find(|event| event.event_type == "execution_finished")
            .expect("finished audit event");
        assert_eq!(finished.details["result"]["ok"], false);
        let verification = events
            .iter()
            .find(|event| event.event_type == "verification_finished")
            .expect("verification audit event");
        assert_eq!(verification.details["result"]["success"], true);
        drop(gateway);
        drop(recorder);
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn execute_approval_does_not_record_execution_audit() {
        let (registry, executions) = registry_with_counting_tool("bash");
        let (recorder, db_path) = audit_recorder("approval");
        let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
            registry,
            Arc::new(DefaultVerifier::new("workspace")),
            Arc::clone(&recorder),
        );
        let request = request(
            "bash",
            serde_json::json!({"command": "git push origin develop"}),
        );

        let outcome = gateway
            .execute(&request, BuiltInRole::Owner, RiskLevel::High)
            .await
            .unwrap();

        assert!(matches!(
            outcome,
            SecurityExecutionOutcome::RequiresApproval
        ));
        assert_eq!(executions.load(Ordering::SeqCst), 0);
        let events = recorder
            .query(&SecurityAuditQuery {
                correlation_id: Some(request.tool_call_id),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(events.len(), 2);
        assert!(events
            .iter()
            .any(|event| event.event_type == "policy_decided"));
        assert!(events
            .iter()
            .any(|event| event.event_type == "approval_requested"));
        drop(gateway);
        drop(recorder);
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn execute_deny_does_not_record_execution_audit() {
        let (registry, executions) = registry_with_counting_tool("write_file");
        let (recorder, db_path) = audit_recorder("deny");
        let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
            registry,
            Arc::new(DefaultVerifier::new("workspace")),
            Arc::clone(&recorder),
        );
        let request = request(
            "write_file",
            serde_json::json!({"path": "notes.txt", "content": "secret"}),
        );

        let outcome = gateway
            .execute(&request, BuiltInRole::Standard, RiskLevel::Medium)
            .await
            .unwrap();

        assert!(matches!(outcome, SecurityExecutionOutcome::Denied { .. }));
        assert_eq!(executions.load(Ordering::SeqCst), 0);
        let events = recorder
            .query(&SecurityAuditQuery {
                correlation_id: Some(request.tool_call_id),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "policy_decided");
        drop(gateway);
        drop(recorder);
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn execute_allow_runs_verifier() {
        let (registry, _executions) = registry_with_counting_tool("read_file");
        let gateway = SecurityExecutionGateway::with_sandbox_registry_and_verifier(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
            registry,
            Arc::new(DefaultVerifier::new("workspace")),
        );
        let request = request("read_file", serde_json::json!({"path": "README.md"}));

        let outcome = gateway
            .execute(&request, BuiltInRole::Standard, RiskLevel::Low)
            .await
            .unwrap();

        match outcome {
            SecurityExecutionOutcome::Executed {
                tool_result,
                verification,
            } => {
                assert!(tool_result.ok);
                assert!(verification.success);
            }
            _ => panic!("expected executed outcome"),
        }
    }

    #[tokio::test]
    async fn execute_allow_calls_injected_verifier_once() {
        let (registry, executions) = registry_with_counting_tool("read_file");
        let (verifier, verifications) = counting_verifier();
        let gateway = SecurityExecutionGateway::with_sandbox_registry_and_verifier(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
            registry,
            verifier,
        );
        let request = request("read_file", serde_json::json!({"path": "README.md"}));

        let outcome = gateway
            .execute(&request, BuiltInRole::Standard, RiskLevel::Low)
            .await
            .unwrap();

        assert!(matches!(outcome, SecurityExecutionOutcome::Executed { .. }));
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        assert_eq!(verifications.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn execute_verification_failure_keeps_tool_success() {
        let (registry, _executions) = registry_with_counting_tool("write_file");
        let (recorder, db_path) = audit_recorder("verification-failure");
        let workspace_root =
            std::env::temp_dir().join(format!("yilian-gateway-verifier-{}", uuid::Uuid::new_v4()));
        let missing_path = format!("missing-{}.txt", uuid::Uuid::new_v4());
        let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            sandbox_config(SandboxProfile::WorkspaceWrite, &[], &[]),
            workspace_root.clone(),
            registry,
            Arc::new(DefaultVerifier::new(
                workspace_root.to_string_lossy().as_ref(),
            )),
            Arc::clone(&recorder),
        );
        let request = request(
            "write_file",
            serde_json::json!({"path": missing_path, "content": "expected"}),
        );

        let outcome = gateway
            .execute(&request, BuiltInRole::Standard, RiskLevel::Medium)
            .await
            .unwrap();

        match outcome {
            SecurityExecutionOutcome::Executed {
                tool_result,
                verification,
            } => {
                assert!(tool_result.ok);
                assert!(!verification.success);
            }
            _ => panic!("expected executed outcome"),
        }

        let events = recorder
            .query(&SecurityAuditQuery {
                correlation_id: Some(request.tool_call_id),
                ..Default::default()
            })
            .unwrap();
        let verification = events
            .iter()
            .find(|event| event.event_type == "verification_finished")
            .expect("verification audit event");
        assert_eq!(verification.details["result"]["success"], false);
        drop(gateway);
        drop(recorder);
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn execute_approval_skips_verifier() {
        let (registry, executions) = registry_with_counting_tool("bash");
        let (verifier, verifications) = counting_verifier();
        let gateway = SecurityExecutionGateway::with_sandbox_registry_and_verifier(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
            registry,
            verifier,
        );
        let request = request(
            "bash",
            serde_json::json!({"command": "git push origin develop"}),
        );

        let outcome = gateway
            .execute(&request, BuiltInRole::Owner, RiskLevel::High)
            .await
            .unwrap();

        assert!(matches!(
            outcome,
            SecurityExecutionOutcome::RequiresApproval
        ));
        assert_eq!(executions.load(Ordering::SeqCst), 0);
        assert_eq!(verifications.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn execute_deny_skips_verifier() {
        let (registry, executions) = registry_with_counting_tool("write_file");
        let (verifier, verifications) = counting_verifier();
        let gateway = SecurityExecutionGateway::with_sandbox_registry_and_verifier(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
            registry,
            verifier,
        );
        let request = request(
            "write_file",
            serde_json::json!({"path": "notes.txt", "content": "hello"}),
        );

        let outcome = gateway
            .execute(&request, BuiltInRole::Standard, RiskLevel::Medium)
            .await
            .unwrap();

        assert!(matches!(outcome, SecurityExecutionOutcome::Denied { .. }));
        assert_eq!(executions.load(Ordering::SeqCst), 0);
        assert_eq!(verifications.load(Ordering::SeqCst), 0);
    }
}
