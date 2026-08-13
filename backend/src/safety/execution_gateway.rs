use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use serde_json::Value;
use thiserror::Error;

use crate::{
    agent::verifier::{DefaultVerifier, VerificationResult, Verifier},
    config::types::SandboxConfig,
    db::Database,
    tools::{trait_def::RiskLevel, ToolRegistry, ToolResult},
};

use super::{
    describe_builtin_tool, AuditError, AuditEventInput, AuditEventType, AuditRecorder, BuiltInRole,
    DecisionContext, DescriptorError, PermissionId, PolicyDecision, PolicyEngine,
    ResourceDescriptor, ResourceScope, SafetyPolicy, SecuritySubject, ToolSecurityDescriptor,
    POLICY_VERSION,
};

#[derive(Debug)]
pub struct SecurityExecutionRequest {
    pub conversation_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: Value,
    /// The security subject that initiated this request.
    /// The gateway resolves the active role from this subject at evaluation time.
    pub subject: SecuritySubject,
}

#[derive(Debug)]
pub enum SecurityExecutionOutcome {
    Executed {
        tool_result: ToolResult,
        verification: VerificationResult,
    },
    RequiresApproval {
        risk_level: RiskLevel,
        reason: String,
    },
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
    #[error("security database unavailable for subject role resolution")]
    RoleDatabaseUnavailable,
    #[error("no active role binding for subject: {0}")]
    MissingRoleBinding(String),
    #[error("invalid role binding '{role}' for subject: {subject_id}")]
    InvalidRoleBinding { subject_id: String, role: String },
}

pub struct SecurityExecutionGateway {
    policy_engine: PolicyEngine,
    sandbox_config: SandboxConfig,
    workspace_root: PathBuf,
    tool_registry: Arc<ToolRegistry>,
    verifier: Arc<dyn Verifier>,
    audit_recorder: Option<Arc<AuditRecorder>>,
    db: Option<Arc<Database>>,
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
            db: None,
        }
    }

    /// Attach a database handle so the gateway can resolve subject roles
    /// from the `security_role_bindings` table at execution time.
    pub fn with_db(mut self, db: Arc<Database>) -> Self {
        self.db = Some(db);
        self
    }

    /// Resolve the active [`BuiltInRole`] for a security subject from the
    /// database-backed `security_role_bindings` table.
    ///
    /// **Fail-closed**: returns an error when the database is unavailable,
    /// the subject has no active binding, or the stored role key is
    /// unrecognised.  There is no fallback — callers must treat the error
    /// as a hard deny.
    pub fn resolve_subject_role(
        &self,
        subject: &SecuritySubject,
    ) -> Result<BuiltInRole, SecurityGatewayError> {
        let db = self
            .db
            .as_ref()
            .ok_or(SecurityGatewayError::RoleDatabaseUnavailable)?;
        let role_key = db
            .resolve_active_role_binding(&subject.subject_id)
            .ok_or_else(|| SecurityGatewayError::MissingRoleBinding(subject.subject_id.clone()))?;
        match role_key.as_str() {
            "owner" => Ok(BuiltInRole::Owner),
            "standard" => Ok(BuiltInRole::Standard),
            "restricted" => Ok(BuiltInRole::Restricted),
            _ => Err(SecurityGatewayError::InvalidRoleBinding {
                subject_id: subject.subject_id.clone(),
                role: role_key,
            }),
        }
    }

    /// Evaluate policy for the request, resolving the subject's role from the
    /// database-backed role bindings, then execute or gate as appropriate.
    pub async fn execute(
        &self,
        request: &SecurityExecutionRequest,
        final_risk: RiskLevel,
    ) -> Result<SecurityExecutionOutcome, SecurityGatewayError> {
        let role = self.resolve_subject_role(&request.subject)?;
        self.execute_with_role(request, role, final_risk).await
    }

    /// Same as [`execute`] but accepts a callback fired immediately before
    /// tool dispatch (for SSE signalling).
    pub async fn execute_with_on_start<F>(
        &self,
        request: &SecurityExecutionRequest,
        final_risk: RiskLevel,
        on_execution_start: F,
    ) -> Result<SecurityExecutionOutcome, SecurityGatewayError>
    where
        F: FnOnce(),
    {
        let role = self.resolve_subject_role(&request.subject)?;
        self.execute_with_role_and_on_start(request, role, final_risk, on_execution_start)
            .await
    }

    /// Internal entry-point that accepts an explicit role (for tests that
    /// inject specific role scenarios without a database).
    pub async fn execute_with_role(
        &self,
        request: &SecurityExecutionRequest,
        role: BuiltInRole,
        final_risk: RiskLevel,
    ) -> Result<SecurityExecutionOutcome, SecurityGatewayError> {
        self.execute_with_role_and_on_start(request, role, final_risk, || {})
            .await
    }

    pub async fn execute_with_role_and_on_start<F>(
        &self,
        request: &SecurityExecutionRequest,
        role: BuiltInRole,
        final_risk: RiskLevel,
        on_execution_start: F,
    ) -> Result<SecurityExecutionOutcome, SecurityGatewayError>
    where
        F: FnOnce(),
    {
        match self.evaluate_with_role(request, role, final_risk)? {
            PolicyDecision::Allow(context) => {
                self.execute_allowed(request, context, on_execution_start)
                    .await
            }
            PolicyDecision::RequireApproval(context) => {
                Ok(SecurityExecutionOutcome::RequiresApproval {
                    risk_level: context.risk_level,
                    reason: context.reason,
                })
            }
            PolicyDecision::Deny(context) => Ok(SecurityExecutionOutcome::Denied {
                reason: context.reason,
            }),
        }
    }

    /// Execute a previously-approved tool call, re-evaluating the subject's
    /// *current* role (which may have been downgraded since the approval was
    /// created).  If the current policy denies the operation the outcome will
    /// be [`SecurityExecutionOutcome::Denied`].
    pub async fn execute_approved(
        &self,
        request: &SecurityExecutionRequest,
        approved_risk: RiskLevel,
    ) -> Result<SecurityExecutionOutcome, SecurityGatewayError> {
        let role = self.resolve_subject_role(&request.subject)?;
        self.execute_approved_with_role(request, role, approved_risk)
            .await
    }

    /// Internal variant of [`execute_approved`] that accepts an explicit
    /// role — used by tests that exercise specific role scenarios.
    pub async fn execute_approved_with_role(
        &self,
        request: &SecurityExecutionRequest,
        role: BuiltInRole,
        approved_risk: RiskLevel,
    ) -> Result<SecurityExecutionOutcome, SecurityGatewayError> {
        let evaluated = self.evaluate_core(request, role, approved_risk)?;
        let decision = match evaluated {
            PolicyDecision::Deny(context) => PolicyDecision::Deny(context),
            PolicyDecision::Allow(mut context) | PolicyDecision::RequireApproval(mut context) => {
                if context.risk_level > approved_risk {
                    context.reason = format!(
                        "approved tool {} risk escalated from {} to {}",
                        request.tool_name, approved_risk, context.risk_level
                    );
                    PolicyDecision::Deny(context)
                } else {
                    context.reason = format!(
                        "tool {} execution authorized by consumed approval",
                        request.tool_name
                    );
                    PolicyDecision::Allow(context)
                }
            }
        };

        self.record_policy_decided(request, &decision)?;
        match decision {
            PolicyDecision::Allow(context) => self.execute_allowed(request, context, || {}).await,
            PolicyDecision::Deny(context) => Ok(SecurityExecutionOutcome::Denied {
                reason: context.reason,
            }),
            PolicyDecision::RequireApproval(_) => {
                unreachable!("approved execution must resolve require-approval before dispatch")
            }
        }
    }

    async fn execute_allowed<F>(
        &self,
        request: &SecurityExecutionRequest,
        context: DecisionContext,
        on_execution_start: F,
    ) -> Result<SecurityExecutionOutcome, SecurityGatewayError>
    where
        F: FnOnce(),
    {
        self.record_execution_started(request, &context)?;
        on_execution_start();
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
        if let Err(error) = self.record_execution_finished(request, &context, tool_result.ok) {
            tracing::error!(
                tool_call_id = %request.tool_call_id,
                tool_name = %request.tool_name,
                error = %error,
                "tool executed but execution-finished audit persistence failed"
            );
        }
        let verification = self
            .verifier
            .verify(&request.tool_name, &request.arguments, &tool_result)
            .await;
        if let Err(error) = self.record_verification_finished(request, &context, &verification) {
            tracing::error!(
                tool_call_id = %request.tool_call_id,
                tool_name = %request.tool_name,
                error = %error,
                "tool verified but verification-finished audit persistence failed"
            );
        }

        Ok(SecurityExecutionOutcome::Executed {
            tool_result,
            verification,
        })
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

    /// Describe a tool call, consulting the runtime registry when the tool is
    /// not a static builtin.
    ///
    /// Builtin tools keep using [`describe_builtin_tool`]. For names the static
    /// builder does not know (e.g. namespaced `mcp_*` tools), fall back to the
    /// tool's own `security_descriptor()` from the registry — so registered MCP
    /// adapters are evaluated through the same policy chain as builtins.
    fn describe_tool(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> Result<ToolSecurityDescriptor, DescriptorError> {
        match describe_builtin_tool(tool_name, args) {
            Ok(descriptor) => Ok(descriptor),
            Err(DescriptorError::UnknownTool(_)) => match self.tool_registry.get(tool_name) {
                Some(tool) => tool.security_descriptor(args),
                None => Err(DescriptorError::UnknownTool(tool_name.to_string())),
            },
            Err(e) => Err(e),
        }
    }

    pub fn resolve_descriptor(
        &self,
        request: &SecurityExecutionRequest,
    ) -> Result<ToolSecurityDescriptor, DescriptorError> {
        let descriptor = self.describe_tool(&request.tool_name, &request.arguments)?;
        descriptor.validate_for_tool(&request.tool_name)?;
        Ok(descriptor)
    }

    fn resolve_resource_scopes(
        &self,
        request: &SecurityExecutionRequest,
        descriptor: &ToolSecurityDescriptor,
    ) -> Result<Vec<ResourceScope>, DescriptorError> {
        descriptor.validate_for_tool(&request.tool_name)?;

        let actual_descriptor = self.describe_tool(&request.tool_name, &request.arguments)?;
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
                ResourceDescriptor::Mcp {
                    server_id,
                    tool_name,
                } if !server_id.trim().is_empty() && !tool_name.trim().is_empty() => {
                    ResourceScope::McpServer
                }
                ResourceDescriptor::Subagent { name } if !name.trim().is_empty() => {
                    ResourceScope::Subagent
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

    fn evaluate_core(
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

        if sandbox_allows_file_write == Some(false) {
            context.reason = format!(
                "sandbox denied tool {} file write request",
                request.tool_name
            );
            return Ok(PolicyDecision::Deny(context));
        }

        // PolicyEngine currently accepts raw role/descriptor/risk inputs and
        // rebuilds its own DecisionContext. Until that API accepts a context
        // directly, this gateway validates the canonical context first and
        // forwards its role and risk through the existing policy boundary.
        // PolicyEngine currently exposes a static evaluation API; retain it as
        // the gateway's explicit policy dependency while forwarding to that API.
        let _policy_engine = &self.policy_engine;
        Ok(PolicyEngine::evaluate(
            context.role,
            &request.tool_name,
            &descriptor,
            context.risk_level,
        ))
    }

    /// Evaluate the security policy for a request using the subject's
    /// currently-bound role.
    pub fn evaluate(
        &self,
        request: &SecurityExecutionRequest,
        final_risk: RiskLevel,
    ) -> Result<PolicyDecision, SecurityGatewayError> {
        let role = self.resolve_subject_role(&request.subject)?;
        self.evaluate_with_role(request, role, final_risk)
    }

    /// Internal variant of [`evaluate`] that accepts an explicit role
    /// — used by tests.
    pub fn evaluate_with_role(
        &self,
        request: &SecurityExecutionRequest,
        role: BuiltInRole,
        final_risk: RiskLevel,
    ) -> Result<PolicyDecision, SecurityGatewayError> {
        let decision = self.evaluate_core(request, role, final_risk)?;
        self.record_policy_decided(request, &decision)?;
        if let PolicyDecision::RequireApproval(context) = &decision {
            self.record_approval_requested(request, context)?;
        }
        Ok(decision)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SecurityExecutionGateway, SecurityExecutionOutcome, SecurityExecutionRequest,
        SecurityGatewayError,
    };
    use crate::agent::verifier::{DefaultVerifier, VerificationResult, Verifier};
    use crate::config::types::{SandboxConfig, SandboxProfile};
    use crate::db::{Database, SecurityAuditQuery};
    use crate::safety::{
        AuditRecorder, BuiltInRole, DescriptorError, PermissionId, PolicyDecision,
        ResourceDescriptor, ResourceScope, SecuritySubject, ToolSecurityDescriptor,
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

    struct AuditBreakingTool {
        database_path: std::path::PathBuf,
        executions: Arc<AtomicUsize>,
    }

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

    #[async_trait]
    impl Tool for AuditBreakingTool {
        fn name(&self) -> &str {
            "read_file"
        }

        fn description(&self) -> &str {
            "breaks audit persistence after execution for gateway tests"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }

        async fn execute(&self, _args: serde_json::Value) -> ToolResult {
            self.executions.fetch_add(1, Ordering::SeqCst);
            let connection = rusqlite::Connection::open(&self.database_path).unwrap();
            connection
                .execute("DROP TABLE security_audit_events", [])
                .unwrap();
            ToolResult::success("executed before audit persistence failed")
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
            subject: SecuritySubject::local_user(),
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
            .evaluate_with_role(&request, BuiltInRole::Standard, RiskLevel::Low)
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
            .evaluate_with_role(&request, BuiltInRole::Standard, RiskLevel::Medium)
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
            .evaluate_with_role(&request, BuiltInRole::Standard, RiskLevel::Medium)
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
            .evaluate_with_role(&request, BuiltInRole::Owner, RiskLevel::High)
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
            .evaluate_with_role(&request, BuiltInRole::Restricted, RiskLevel::Medium)
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
            .evaluate_with_role(&request, BuiltInRole::Standard, RiskLevel::Low)
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
            .evaluate_with_role(&request, BuiltInRole::Owner, RiskLevel::High)
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
            .evaluate_with_role(&request, BuiltInRole::Restricted, RiskLevel::Medium)
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
            .evaluate_with_role(&request, BuiltInRole::Standard, RiskLevel::Medium)
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
            .evaluate_with_role(&request, BuiltInRole::Standard, RiskLevel::Medium)
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
            .evaluate_with_role(&request, BuiltInRole::Standard, RiskLevel::Medium)
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
            .evaluate_with_role(&request, BuiltInRole::Standard, RiskLevel::Medium)
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
            .evaluate_with_role(&request, BuiltInRole::Standard, RiskLevel::Low)
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
            .evaluate_with_role(&request, BuiltInRole::Standard, RiskLevel::Critical)
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
            .evaluate_with_role(&request, BuiltInRole::Standard, RiskLevel::Medium)
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
            .evaluate_with_role(&request, BuiltInRole::Standard, RiskLevel::Medium)
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
            .execute_with_role(&request, BuiltInRole::Standard, RiskLevel::Low)
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
            .execute_with_role(&request, BuiltInRole::Owner, RiskLevel::High)
            .await
            .unwrap();

        assert!(matches!(
            outcome,
            SecurityExecutionOutcome::RequiresApproval { .. }
        ));
        assert_eq!(executions.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn execute_returns_approval_risk_and_reason() {
        let (registry, executions) = registry_with_counting_tool("bash");
        let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
            sandbox_config(SandboxProfile::Open, &[], &[]),
            "workspace",
            registry,
        );
        let request = request(
            "bash",
            serde_json::json!({"command": "git push origin develop"}),
        );

        match gateway
            .execute_with_role(&request, BuiltInRole::Owner, RiskLevel::Low)
            .await
            .unwrap()
        {
            SecurityExecutionOutcome::RequiresApproval { risk_level, reason } => {
                assert_eq!(risk_level, RiskLevel::High);
                assert!(!reason.is_empty());
            }
            other => panic!("expected approval, got {other:?}"),
        }
        assert_eq!(executions.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn execute_callback_runs_once_only_for_allow() {
        let (registry, executions) = registry_with_counting_tool("read_file");
        let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
            registry,
        );
        let starts = AtomicUsize::new(0);
        let request = request("read_file", serde_json::json!({"path": "README.md"}));

        let outcome = gateway
            .execute_with_role_and_on_start(&request, BuiltInRole::Owner, RiskLevel::Low, || {
                starts.fetch_add(1, Ordering::SeqCst);
            })
            .await
            .unwrap();

        assert!(matches!(outcome, SecurityExecutionOutcome::Executed { .. }));
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert_eq!(executions.load(Ordering::SeqCst), 1);
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
            .execute_with_role(&request, BuiltInRole::Standard, RiskLevel::Medium)
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
            .execute_with_role(&request, BuiltInRole::Standard, RiskLevel::Low)
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
            .execute_with_role(&request, BuiltInRole::Standard, RiskLevel::Low)
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
            .execute_with_role(&request, BuiltInRole::Owner, RiskLevel::High)
            .await
            .unwrap();

        assert!(matches!(
            outcome,
            SecurityExecutionOutcome::RequiresApproval { .. }
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
            .execute_with_role(&request, BuiltInRole::Standard, RiskLevel::Medium)
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
            .execute_with_role(&request, BuiltInRole::Standard, RiskLevel::Low)
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
            .execute_with_role(&request, BuiltInRole::Standard, RiskLevel::Low)
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
            .execute_with_role(&request, BuiltInRole::Standard, RiskLevel::Medium)
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
            .execute_with_role(&request, BuiltInRole::Owner, RiskLevel::High)
            .await
            .unwrap();

        assert!(matches!(
            outcome,
            SecurityExecutionOutcome::RequiresApproval { .. }
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
            .execute_with_role(&request, BuiltInRole::Standard, RiskLevel::Medium)
            .await
            .unwrap();

        assert!(matches!(outcome, SecurityExecutionOutcome::Denied { .. }));
        assert_eq!(executions.load(Ordering::SeqCst), 0);
        assert_eq!(verifications.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn execute_approved_runs_original_high_risk_tool_once_without_reapproval() {
        let (registry, executions) = registry_with_counting_tool("bash");
        let (verifier, verifications) = counting_verifier();
        let (recorder, db_path) = audit_recorder("approved-execution");
        let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            sandbox_config(SandboxProfile::Open, &[], &[]),
            "workspace",
            registry,
            verifier,
            Arc::clone(&recorder),
        );
        let request = request("bash", serde_json::json!({"command": "echo approved"}));

        let outcome = gateway
            .execute_approved_with_role(&request, BuiltInRole::Owner, RiskLevel::High)
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
        assert!(events
            .iter()
            .any(|event| event.event_type == "policy_decided"
                && event.decision_status.as_deref() == Some("allow")));
        assert!(events
            .iter()
            .any(|event| event.event_type == "execution_started"));
        assert!(events
            .iter()
            .any(|event| event.event_type == "execution_finished"));
        assert!(events
            .iter()
            .any(|event| event.event_type == "verification_finished"));
        assert!(!events
            .iter()
            .any(|event| event.event_type == "approval_requested"));
        drop(gateway);
        drop(recorder);
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn execute_approved_denies_sandbox_hard_deny_without_execution() {
        let (registry, executions) = registry_with_counting_tool("write_file");
        let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
            registry,
        );
        let request = request(
            "write_file",
            serde_json::json!({"path": "notes.txt", "content": "blocked"}),
        );

        let outcome = gateway
            .execute_approved_with_role(&request, BuiltInRole::Owner, RiskLevel::Medium)
            .await
            .unwrap();

        assert!(matches!(outcome, SecurityExecutionOutcome::Denied { .. }));
        assert_eq!(executions.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn execute_approved_denies_dynamic_risk_escalation_without_execution() {
        let (registry, executions) = registry_with_counting_tool("bash");
        let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
            sandbox_config(SandboxProfile::Open, &[], &[]),
            "workspace",
            registry,
        );
        let request = request("bash", serde_json::json!({"command": "diskpart"}));

        let outcome = gateway
            .execute_approved_with_role(&request, BuiltInRole::Owner, RiskLevel::High)
            .await
            .unwrap();

        match outcome {
            SecurityExecutionOutcome::Denied { reason } => {
                assert!(reason.contains("risk"));
                assert!(reason.contains("critical"));
            }
            _ => panic!("expected approved execution to be denied"),
        }
        assert_eq!(executions.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn execute_approved_preserves_result_and_verifies_after_post_execution_audit_failure() {
        let (recorder, db_path) = audit_recorder("approved-post-execution-audit-failure");
        let executions = Arc::new(AtomicUsize::new(0));
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(AuditBreakingTool {
            database_path: db_path.clone(),
            executions: Arc::clone(&executions),
        }));
        let (verifier, verifications) = counting_verifier();
        let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            sandbox_config(SandboxProfile::Open, &[], &[]),
            "workspace",
            Arc::new(registry),
            verifier,
            Arc::clone(&recorder),
        );
        let request = request("read_file", serde_json::json!({"path": "README.md"}));

        let outcome = gateway
            .execute_approved_with_role(&request, BuiltInRole::Owner, RiskLevel::Low)
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
            _ => panic!("expected executed outcome after post-execution audit failure"),
        }
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        assert_eq!(verifications.load(Ordering::SeqCst), 1);
        assert_eq!(recorder.health(), crate::safety::AuditHealth::Degraded);
        drop(gateway);
        drop(recorder);
        let _ = std::fs::remove_file(db_path);
    }

    // ── RBAC subject role resolution tests ──

    #[test]
    fn resolve_subject_role_returns_owner_from_db() {
        let db_path =
            std::env::temp_dir().join(format!("yilian-rbac-owner-{}.db", uuid::Uuid::new_v4()));
        let db = Database::new(&db_path).unwrap();
        // DB migration seeds local-user → owner
        let gateway = SecurityExecutionGateway::new().with_db(Arc::new(db.clone_connection()));

        let role = gateway
            .resolve_subject_role(&SecuritySubject::local_user())
            .unwrap();
        assert_eq!(role, BuiltInRole::Owner);

        drop(gateway);
        drop(db);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn resolve_subject_role_standard_from_db() {
        let db_path =
            std::env::temp_dir().join(format!("yilian-rbac-standard-{}.db", uuid::Uuid::new_v4()));
        let db = Database::new(&db_path).unwrap();
        // Override the default owner binding to standard
        db.conn()
            .execute(
                "UPDATE security_role_bindings SET role_key = 'standard'
                 WHERE subject_id = 'local-user' AND revoked_at IS NULL",
                [],
            )
            .unwrap();
        let gateway = SecurityExecutionGateway::new().with_db(Arc::new(db.clone_connection()));

        let role = gateway
            .resolve_subject_role(&SecuritySubject::local_user())
            .unwrap();
        assert_eq!(role, BuiltInRole::Standard);

        drop(gateway);
        drop(db);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn resolve_subject_role_restricted_from_db() {
        let db_path = std::env::temp_dir().join(format!(
            "yilian-rbac-restricted-{}.db",
            uuid::Uuid::new_v4()
        ));
        let db = Database::new(&db_path).unwrap();
        db.conn()
            .execute(
                "UPDATE security_role_bindings SET role_key = 'restricted'
                 WHERE subject_id = 'local-user' AND revoked_at IS NULL",
                [],
            )
            .unwrap();
        let gateway = SecurityExecutionGateway::new().with_db(Arc::new(db.clone_connection()));

        let role = gateway
            .resolve_subject_role(&SecuritySubject::local_user())
            .unwrap();
        assert_eq!(role, BuiltInRole::Restricted);

        drop(gateway);
        drop(db);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn resolve_subject_role_errors_when_no_db() {
        let gateway = SecurityExecutionGateway::new();

        let result = gateway.resolve_subject_role(&SecuritySubject::local_user());
        assert!(
            matches!(result, Err(SecurityGatewayError::RoleDatabaseUnavailable)),
            "expected RoleDatabaseUnavailable, got {result:?}"
        );
    }

    #[test]
    fn resolve_subject_role_errors_for_missing_binding() {
        let db_path =
            std::env::temp_dir().join(format!("yilian-rbac-missing-{}.db", uuid::Uuid::new_v4()));
        let db = Database::new(&db_path).unwrap();
        let gateway = SecurityExecutionGateway::new().with_db(Arc::new(db.clone_connection()));

        let unknown = SecuritySubject {
            subject_id: "unknown-user".to_string(),
            subject_type: crate::safety::SubjectType::LocalUser,
            provider: "test".to_string(),
            external_ref: None,
        };
        let result = gateway.resolve_subject_role(&unknown);
        assert!(
            matches!(
                &result,
                Err(SecurityGatewayError::MissingRoleBinding(id)) if id == "unknown-user"
            ),
            "expected MissingRoleBinding, got {result:?}"
        );

        drop(gateway);
        drop(db);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn resolve_subject_role_errors_for_invalid_role_key() {
        let db_path =
            std::env::temp_dir().join(format!("yilian-rbac-invalid-{}.db", uuid::Uuid::new_v4()));
        let db = Database::new(&db_path).unwrap();
        // Replace the schema-constrained table with one that accepts any role_key
        // so the defence-in-depth match in resolve_subject_role can be exercised.
        let conn = db.conn();
        conn.execute_batch(
            "DROP TABLE IF EXISTS security_role_bindings;
             CREATE TABLE security_role_bindings (
                 binding_id TEXT PRIMARY KEY,
                 subject_id TEXT NOT NULL,
                 role_key TEXT NOT NULL,
                 source TEXT NOT NULL,
                 effective_at INTEGER NOT NULL,
                 expires_at INTEGER,
                 revoked_at INTEGER
             );
             INSERT INTO security_role_bindings VALUES
                 ('test-invalid', 'local-user', 'administrator', 'test', 0, NULL, NULL);",
        )
        .unwrap();
        drop(conn);
        let gateway = SecurityExecutionGateway::new().with_db(Arc::new(db.clone_connection()));

        let result = gateway.resolve_subject_role(&SecuritySubject::local_user());
        assert!(
            matches!(
                &result,
                Err(SecurityGatewayError::InvalidRoleBinding { subject_id, role })
                    if subject_id == "local-user" && role == "administrator"
            ),
            "expected InvalidRoleBinding, got {result:?}"
        );

        drop(gateway);
        drop(db);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn evaluate_uses_subject_role_from_db() {
        let db_path =
            std::env::temp_dir().join(format!("yilian-rbac-eval-{}.db", uuid::Uuid::new_v4()));
        let db = Database::new(&db_path).unwrap();
        let gateway = SecurityExecutionGateway::new().with_db(Arc::new(db.clone_connection()));

        // local-user is owner → write_file should be allowed
        let mut request = request(
            "write_file",
            serde_json::json!({"path": "notes.txt", "content": "hello"}),
        );
        request.subject = SecuritySubject::local_user();

        let decision = gateway.evaluate(&request, RiskLevel::Medium).unwrap();
        assert!(
            matches!(decision, PolicyDecision::Allow(_)),
            "owner should be allowed to write files"
        );

        drop(gateway);
        drop(db);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn restricted_subject_write_file_is_denied_by_policy() {
        let gateway = SecurityExecutionGateway::new();
        let mut request = request(
            "write_file",
            serde_json::json!({"path": "notes.txt", "content": "hello"}),
        );
        request.subject = SecuritySubject::local_user();

        let decision = gateway
            .evaluate_with_role(&request, BuiltInRole::Restricted, RiskLevel::Medium)
            .unwrap();
        assert!(
            matches!(decision, PolicyDecision::Deny(_)),
            "restricted subject should be denied filesystem write"
        );
    }

    #[test]
    fn restricted_subject_shell_execute_is_denied_by_policy() {
        let gateway = SecurityExecutionGateway::new();
        let mut request = request("bash", serde_json::json!({"command": "echo test"}));
        request.subject = SecuritySubject::local_user();

        let decision = gateway
            .evaluate_with_role(&request, BuiltInRole::Restricted, RiskLevel::Low)
            .unwrap();
        assert!(
            matches!(decision, PolicyDecision::Deny(_)),
            "restricted subject should be denied shell execute"
        );
    }

    #[test]
    fn owner_subject_bash_requires_approval() {
        let db_path = std::env::temp_dir().join(format!(
            "yilian-rbac-owner-bash-{}.db",
            uuid::Uuid::new_v4()
        ));
        let db = Database::new(&db_path).unwrap();
        let gateway = SecurityExecutionGateway::new().with_db(Arc::new(db.clone_connection()));

        let mut request = request(
            "bash",
            serde_json::json!({"command": "git push origin develop"}),
        );
        request.subject = SecuritySubject::local_user();

        let decision = gateway.evaluate(&request, RiskLevel::High).unwrap();
        assert!(
            matches!(decision, PolicyDecision::RequireApproval(_)),
            "owner should require approval for high-risk bash"
        );

        drop(gateway);
        drop(db);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn execute_errors_when_no_db() {
        let (registry, executions) = registry_with_counting_tool("read_file");
        let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
            registry,
        );
        // No .with_db() — role resolution must fail
        let mut request = request("read_file", serde_json::json!({"path": "README.md"}));
        request.subject = SecuritySubject::local_user();

        let result = gateway.execute(&request, RiskLevel::Low).await;
        assert!(
            matches!(result, Err(SecurityGatewayError::RoleDatabaseUnavailable)),
            "expected RoleDatabaseUnavailable, got {result:?}"
        );
        assert_eq!(executions.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn execute_errors_for_missing_binding() {
        let db_path =
            std::env::temp_dir().join(format!("yilian-exec-missing-{}.db", uuid::Uuid::new_v4()));
        let db = Database::new(&db_path).unwrap();
        let (registry, executions) = registry_with_counting_tool("read_file");
        let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
            registry,
        )
        .with_db(Arc::new(db.clone_connection()));

        let mut request = request("read_file", serde_json::json!({"path": "README.md"}));
        request.subject = SecuritySubject {
            subject_id: "no-binding-user".to_string(),
            ..SecuritySubject::local_user()
        };

        let result = gateway.execute(&request, RiskLevel::Low).await;
        assert!(
            matches!(
                &result,
                Err(SecurityGatewayError::MissingRoleBinding(id)) if id == "no-binding-user"
            ),
            "expected MissingRoleBinding, got {result:?}"
        );
        assert_eq!(executions.load(Ordering::SeqCst), 0);

        drop(gateway);
        drop(db);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn execute_errors_for_invalid_role_key() {
        let db_path =
            std::env::temp_dir().join(format!("yilian-exec-invalid-{}.db", uuid::Uuid::new_v4()));
        let db = Database::new(&db_path).unwrap();
        // Bypass the CHECK constraint to exercise the defence-in-depth match
        let conn = db.conn();
        conn.execute_batch(
            "DROP TABLE IF EXISTS security_role_bindings;
             CREATE TABLE security_role_bindings (
                 binding_id TEXT PRIMARY KEY,
                 subject_id TEXT NOT NULL,
                 role_key TEXT NOT NULL,
                 source TEXT NOT NULL,
                 effective_at INTEGER NOT NULL,
                 expires_at INTEGER,
                 revoked_at INTEGER
             );
             INSERT INTO security_role_bindings VALUES
                 ('test-exec-invalid', 'local-user', 'administrator', 'test', 0, NULL, NULL);",
        )
        .unwrap();
        drop(conn);
        let (registry, executions) = registry_with_counting_tool("read_file");
        let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
            sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
            "workspace",
            registry,
        )
        .with_db(Arc::new(db.clone_connection()));

        let mut request = request("read_file", serde_json::json!({"path": "README.md"}));
        request.subject = SecuritySubject::local_user();

        let result = gateway.execute(&request, RiskLevel::Low).await;
        assert!(
            matches!(
                &result,
                Err(SecurityGatewayError::InvalidRoleBinding { subject_id, role })
                    if subject_id == "local-user" && role == "administrator"
            ),
            "expected InvalidRoleBinding, got {result:?}"
        );
        assert_eq!(executions.load(Ordering::SeqCst), 0);

        drop(gateway);
        drop(db);
        let _ = std::fs::remove_file(&db_path);
    }

    // ── Subagent delegation through the runtime dynamic-tool fallback ──

    fn subagent_gateway() -> SecurityExecutionGateway {
        let definition = crate::server::DiscoveredSubagent {
            name: "researcher".to_string(),
            description: "研究助手".to_string(),
            path: ".agents/agents/researcher/AGENT.md".to_string(),
            allowed_tools: vec!["read_file".to_string(), "grep".to_string()],
            model: Some("deepseek-v4-flash".to_string()),
            workdir: None,
            instructions: "research body".to_string(),
        };
        let adapter = crate::tools::SubagentToolAdapter::new(&definition).unwrap();
        let mut registry = ToolRegistry::new();
        registry.register(std::sync::Arc::new(adapter));
        SecurityExecutionGateway::with_sandbox_and_registry(
            sandbox_config(SandboxProfile::Open, &[], &[]),
            "workspace",
            Arc::new(registry),
        )
    }

    #[test]
    fn subagent_delegation_owner_and_standard_require_approval_restricted_denies() {
        let gateway = subagent_gateway();
        let request = request(
            "subagent_researcher",
            serde_json::json!({"task": "研究当前仓库"}),
        );

        let owner = gateway
            .evaluate_with_role(&request, BuiltInRole::Owner, RiskLevel::High)
            .unwrap();
        assert!(matches!(owner, PolicyDecision::RequireApproval(_)));

        let standard = gateway
            .evaluate_with_role(&request, BuiltInRole::Standard, RiskLevel::High)
            .unwrap();
        assert!(matches!(standard, PolicyDecision::RequireApproval(_)));

        let restricted = gateway
            .evaluate_with_role(&request, BuiltInRole::Restricted, RiskLevel::High)
            .unwrap();
        assert!(matches!(restricted, PolicyDecision::Deny(_)));
    }
}
