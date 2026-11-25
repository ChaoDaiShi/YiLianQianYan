use super::*;
use crate::db::access_rules::{resolve_access_rule, AccessMode, AccessRule};
use crate::safety::{grant::GrantResource, GrantMode, RolePolicy};

impl SecurityExecutionGateway {
    pub(super) fn evaluate_delegated(
        &self,
        request: &SecurityExecutionRequest,
        descriptor: &ToolSecurityDescriptor,
        mut context: DecisionContext,
        delegation: &DelegatedSubject,
    ) -> Result<PolicyDecision, SecurityGatewayError> {
        if request.subject != delegation.subject {
            return Err(SecurityGatewayError::DelegatedSubjectMismatch);
        }
        if self.tool_registry.get(&request.tool_name).is_none() {
            context.reason = "tool is outside the delegated tool list".into();
            return Ok(PolicyDecision::Deny(context));
        }

        let db = self
            .db
            .as_ref()
            .ok_or(SecurityGatewayError::RoleDatabaseUnavailable)?;
        let parent = &delegation.parent;
        let parent_role = self.resolve_bound_role(&parent.subject)?;
        let parent_policy = PolicyEngine::evaluate(
            parent_role,
            &request.tool_name,
            descriptor,
            context.risk_level,
        );
        if RolePolicy::grant(parent_role, PermissionId::AgentDelegate) == GrantMode::Deny
            || matches!(parent_policy, PolicyDecision::Deny(_))
        {
            context.reason =
                "delegating subject no longer has the required role permissions".into();
            return Ok(PolicyDecision::Deny(context));
        }

        let rules = db
            .list_access_rules()
            .map_err(SecurityGatewayError::AccessRules)?;
        let child_id = Some(delegation.subject.subject_id.as_str());
        let child_rules: Vec<_> = rules
            .iter()
            .filter(|rule| {
                rule.subject_id.as_deref() == child_id
                    && matches!(rule.scope.as_str(), "agent" | "resource")
            })
            .cloned()
            .collect();
        let resolve =
            |rules: &[AccessRule], permission, resource: &ResourceDescriptor, agent, tool: &str| {
                resolve_access_rule(
                    rules,
                    permission,
                    Some(resource),
                    agent,
                    tool,
                    &self.workspace_root,
                    self.managed_process_registry.clone(),
                )
            };
        let delegated_resource = ResourceDescriptor::Subagent {
            name: delegation.child_name.clone(),
        };
        if resolve(
            &rules,
            PermissionId::AgentDelegate,
            &delegated_resource,
            parent.agent_id.as_deref(),
            &parent.tool_name,
        )
        .is_some_and(|rule| rule.mode == AccessMode::Deny)
        {
            context.reason = "delegation permission was revoked".into();
            return Ok(PolicyDecision::Deny(context));
        }

        let evaluator = |subject: &SecuritySubject| {
            db.list_grants(&subject.subject_id)
                .map(|grants| {
                    GrantEvaluator::new(grants, &self.workspace_root)
                        .with_registry(self.managed_process_registry.clone())
                })
                .map_err(|error| SecurityGatewayError::AccessRules(error.to_string()))
        };
        let parent_grants = evaluator(&parent.subject)?;
        let child_grants = evaluator(&delegation.subject)?;
        let now = chrono::Utc::now().timestamp_millis();
        if matches!(
            parent_grants.evaluate(PermissionId::AgentDelegate, &delegated_resource, now),
            GrantDecision::Deny { .. }
        ) {
            context.reason = "delegation permission was revoked".into();
            return Ok(PolicyDecision::Deny(context));
        }
        let mut needs_approval = false;

        for requested in &descriptor.requested_permissions {
            let permission = requested.permission;
            for resource in descriptor
                .resources
                .iter()
                .filter(|resource| resource_scope(resource) == requested.scope)
            {
                let child_rule =
                    resolve(&rules, permission, resource, child_id, &request.tool_name);
                let parent_rule = resolve(
                    &rules,
                    permission,
                    resource,
                    parent.agent_id.as_deref(),
                    &request.tool_name,
                );
                let parent_grant = parent_grants.evaluate_with_evidence(permission, resource, now);
                let child_grant = child_grants.evaluate_with_evidence(permission, resource, now);
                if [child_rule.as_ref(), parent_rule.as_ref()]
                    .into_iter()
                    .flatten()
                    .any(|rule| rule.mode == AccessMode::Deny)
                    || [&child_grant, &parent_grant]
                        .into_iter()
                        .any(|grant| matches!(grant.decision, GrantDecision::Deny { .. }))
                {
                    context.reason =
                        "delegated operation denied by current access rules or grants".into();
                    return Ok(PolicyDecision::Deny(context));
                }

                let mut evidence = child_grant;
                let scoped_rule = resolve(
                    &child_rules,
                    permission,
                    resource,
                    child_id,
                    &request.tool_name,
                );
                if let Some(rule) = &scoped_rule {
                    if rule.mode == AccessMode::Full && evidence.matched_grant_id.is_none() {
                        evidence = GrantEvaluation {
                            decision: GrantDecision::Allow,
                            matched_grant_id: Some(rule.effective_rule_id.clone()),
                            matched_resource: child_rules
                                .iter()
                                .find(|candidate| candidate.id == rule.effective_rule_id)
                                .and_then(|candidate| candidate.resource.clone())
                                .or(Some(GrantResource::Any)),
                        };
                    }
                }
                let explicit = evidence.matched_grant_id.is_some()
                    || scoped_rule.is_some_and(|rule| rule.mode == AccessMode::Approve);
                let default_read = matches!((permission, resource), (PermissionId::FilesystemRead, ResourceDescriptor::File { path }) if delegated_workspace_contains(&delegation.default_read_root, path));
                if !explicit && !default_read {
                    context.reason = "delegated operation has no matching child authority".into();
                    return Ok(PolicyDecision::Deny(context));
                }

                needs_approval |= [child_rule.as_ref(), parent_rule.as_ref()]
                    .into_iter()
                    .flatten()
                    .any(|rule| rule.mode == AccessMode::Approve)
                    || [&evidence, &parent_grant].into_iter().any(|grant| {
                        grant.matched_grant_id.is_some()
                            && matches!(grant.decision, GrantDecision::RequireApproval { .. })
                    });
                if let Some(resource) =
                    authorized_resource_from_evaluation(resource, &evidence, true)
                {
                    context.authorized_resources.push(resource);
                }
            }
        }

        let mode = self
            .permissions_config
            .as_ref()
            .map(|config| config.read().permissions.mode.clone());
        needs_approval |= match mode {
            Some(PermissionMode::FullAccess) => false,
            Some(PermissionMode::FullApproval) => true,
            _ => matches!(parent_policy, PolicyDecision::RequireApproval(_)),
        };
        if needs_approval {
            context.reason =
                "delegated operation requires approval under the current policy".into();
            Ok(PolicyDecision::RequireApproval(context))
        } else {
            context.reason = "delegated operation is covered by the child scope".into();
            Ok(PolicyDecision::Allow(context))
        }
    }
}

fn delegated_workspace_contains(root: &Path, path: &str) -> bool {
    let Ok(root) = root.canonicalize() else {
        return false;
    };
    crate::safety::resolve_write_target(&root, Path::new(path))
        .is_ok_and(|target| crate::safety::is_within_root(&root, &target))
}

fn resource_scope(resource: &ResourceDescriptor) -> ResourceScope {
    match resource {
        ResourceDescriptor::File { .. } => ResourceScope::Workspace,
        ResourceDescriptor::Shell { .. } => ResourceScope::ShellCommand,
        ResourceDescriptor::Process { .. } => ResourceScope::Process,
        ResourceDescriptor::Network { .. } | ResourceDescriptor::NetworkFromResponse { .. } => {
            ResourceScope::NetworkTarget
        }
        ResourceDescriptor::Desktop { .. } => ResourceScope::DesktopTarget,
        ResourceDescriptor::Skill { .. } => ResourceScope::DiscoveredSkill,
        ResourceDescriptor::Agent { .. } => ResourceScope::AgentInternal,
        ResourceDescriptor::Mcp { .. } => ResourceScope::McpServer,
        ResourceDescriptor::Subagent { .. } => ResourceScope::Subagent,
        ResourceDescriptor::ChannelMessage { .. } => ResourceScope::ChannelTarget,
    }
}
