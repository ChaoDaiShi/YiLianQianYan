use super::*;
use crate::{
    db::access_rules::{resolve_access_rule, AccessMode},
    modules::{
        task::{
            model::TaskExecutionStatus, proofloop::budget::GoalModelReservation, NodeExecutionId,
            TaskExecutionControlState, TaskExecutionId, TaskId,
        },
        workflow::WorkflowRunId,
    },
    safety::{
        grant::{GrantDecision, GrantEvaluator},
        subject::{DelegatedSubject, SubjectType},
        BuiltInRole, GrantMode, PermissionId, ResourceDescriptor, RolePolicy,
    },
    workspace::{WorkspaceId, WorkspaceStatus},
};

pub(super) struct ModelAuthority {
    pub server_id: String,
    pub tool_name: String,
    pub grant_id: Option<String>,
    pub one_shot_approval: bool,
    pub delegation: Option<DelegatedSubject>,
}

impl ModelAuthority {
    pub(super) fn configuration_invocation<'a>(
        &'a self,
        caller: &'a ToolInvocation,
    ) -> &'a ToolInvocation {
        self.delegation
            .as_ref()
            .map_or(caller, |delegation| &delegation.parent)
    }
}

pub(super) fn still_authorized(
    server: &AppServer,
    caller: &ToolInvocation,
    authority: &ModelAuthority,
) -> Result<(), ()> {
    let permission = PermissionId::McpInvoke;
    let resource = ResourceDescriptor::Mcp {
        server_id: authority.server_id.clone(),
        tool_name: authority.tool_name.clone(),
    };
    let subject = caller.subject.subject_id.as_str();
    let permits_role = |subject: &str, permission| {
        let role = match server.db.resolve_active_role_binding(subject).as_deref() {
            Some("owner") => BuiltInRole::Owner,
            Some("standard") => BuiltInRole::Standard,
            Some("restricted") => BuiltInRole::Restricted,
            _ => return false,
        };
        RolePolicy::grant(role, permission) != GrantMode::Deny
    };
    let rules = server.db.list_access_rules().map_err(|_| ())?;
    let agent = match &authority.delegation {
        Some(delegation) => {
            let parent = &delegation.parent;
            let parent_id = parent.subject.subject_id.as_str();
            if delegation.subject != caller.subject
                || !permits_role(parent_id, PermissionId::AgentDelegate)
                || !permits_role(parent_id, PermissionId::McpInvoke)
            {
                return Err(());
            }
            let denied = |permission, resource: &ResourceDescriptor, tool: &str| {
                resolve_access_rule(
                    &rules,
                    permission,
                    Some(resource),
                    parent.agent_id.as_deref(),
                    tool,
                    std::path::Path::new(&server.workspace_root),
                    Arc::clone(&server.managed_process_registry),
                )
                .is_some_and(|rule| rule.mode == AccessMode::Deny)
            };
            let delegate = ResourceDescriptor::Subagent {
                name: delegation.child_name.clone(),
            };
            let parent_grants = GrantEvaluator::new(
                server.db.list_grants(parent_id).map_err(|_| ())?,
                &server.workspace_root,
            );
            let now = chrono::Utc::now().timestamp_millis();
            if denied(PermissionId::AgentDelegate, &delegate, &parent.tool_name)
                || denied(permission, &resource, &caller.tool_name)
                || matches!(
                    parent_grants.evaluate(permission, &resource, now),
                    GrantDecision::Deny { .. }
                )
                || matches!(
                    parent_grants.evaluate(PermissionId::AgentDelegate, &delegate, now),
                    GrantDecision::Deny { .. }
                )
            {
                return Err(());
            }
            Some(subject)
        }
        None if !permits_role(subject, permission)
            || caller.subject.subject_type == SubjectType::Subagent =>
        {
            return Err(())
        }
        None => caller.agent_id.as_deref(),
    };
    let access = (subject == "local-user" || authority.delegation.is_some())
        .then(|| {
            resolve_access_rule(
                &rules,
                permission,
                Some(&resource),
                agent,
                &caller.tool_name,
                std::path::Path::new(&server.workspace_root),
                Arc::clone(&server.managed_process_registry),
            )
        })
        .flatten();
    let grants = server.db.list_grants(subject).map_err(|_| ())?;
    let evidence = GrantEvaluator::new(grants, &server.workspace_root).evaluate_with_evidence(
        permission,
        &resource,
        chrono::Utc::now().timestamp_millis(),
    );
    if matches!(evidence.decision, GrantDecision::Deny { .. })
        || access
            .as_ref()
            .is_some_and(|rule| rule.mode == AccessMode::Deny)
    {
        return Err(());
    }
    let full_access =
        server.config.read().permissions.mode == crate::config::types::PermissionMode::FullAccess;
    if !authority.one_shot_approval && !full_access {
        return Err(());
    }
    let Some(expected) = authority.grant_id.as_deref() else {
        return (authority.delegation.is_none()
            && (authority.one_shot_approval || subject == "local-user"))
            .then_some(())
            .ok_or(());
    };
    let scoped_access = access.as_ref().is_some_and(|rule| {
        rule.mode == AccessMode::Full
            && rule.effective_rule_id == expected
            && (authority.delegation.is_none()
                || rules.iter().any(|candidate| {
                    candidate.id == expected
                        && candidate.subject_id.as_deref() == Some(subject)
                        && matches!(candidate.scope.as_str(), "agent" | "resource")
                }))
    });
    if scoped_access
        || (matches!(evidence.decision, GrantDecision::Allow)
            && evidence.matched_grant_id.as_deref() == Some(expected))
    {
        Ok(())
    } else {
        Err(())
    }
}

pub(super) fn active_workspace(
    server: &AppServer,
    owner: &ExecutionOwner,
    caller: &ToolInvocation,
    scope: Option<&ExecutionScope>,
) -> Result<Option<String>, ()> {
    if scope.is_some_and(|scope| {
        scope.cancel.is_cancelled()
            || scope
                .deadline
                .is_some_and(|deadline| deadline <= tokio::time::Instant::now())
            || (scope.is_bounded() && scope.budget.is_none())
    }) {
        return Err(());
    }
    let has_budget = scope.is_some_and(|scope| scope.budget.is_some());
    let workspace = match owner {
        ExecutionOwner::Conversation { conversation_id }
            if conversation_id == &caller.conversation_id =>
        {
            server
                .db
                .get_conversation(conversation_id)
                .map_err(|_| ())?;
            server
                .db
                .conversation_workspace(conversation_id)
                .map_err(|_| ())?
        }
        ExecutionOwner::PersonalDelegation {
            automation_id,
            occurrence_id,
            conversation_id,
        } if conversation_id == &caller.conversation_id && has_budget => {
            let occurrence = server
                .db
                .automation_runtime_occurrence(automation_id)
                .map_err(|_| ())?
                .ok_or(())?;
            if occurrence.id != *occurrence_id
                || occurrence.conversation_id.as_deref() != Some(conversation_id)
                || !matches!(
                    occurrence.status.as_str(),
                    "running" | "dispatching" | "waiting_approval" | "awaiting_approval"
                )
            {
                return Err(());
            }
            Some(occurrence.workspace_id)
        }
        ExecutionOwner::LegacyTask {
            task_id,
            execution_id,
        } => {
            let task_id = TaskId::new(task_id).map_err(|_| ())?;
            let execution_id = TaskExecutionId::new(execution_id).map_err(|_| ())?;
            let execution = server
                .db
                .get_task_execution(&execution_id)
                .map_err(|_| ())?
                .ok_or(())?;
            if execution.task_id != task_id || execution.status != TaskExecutionStatus::Running {
                return Err(());
            }
            Some(
                server
                    .db
                    .get_task(&task_id)
                    .map_err(|_| ())?
                    .ok_or(())?
                    .workspace_id
                    .to_string(),
            )
        }
        ExecutionOwner::GoalNode {
            graph_id,
            node_id,
            execution_id,
            ..
        } => {
            let id = NodeExecutionId::new(execution_id).map_err(|_| ())?;
            active_goal_node(server, &id, graph_id, node_id)?;
            let proof = server
                .db
                .confirmed_proof_draft_for_graph(graph_id)
                .map_err(|_| ())?;
            proof.map(|draft| draft.contract.workspace_id)
        }
        ExecutionOwner::Workflow {
            workflow_graph_id,
            run_id,
        } => {
            let id = WorkflowRunId::new(run_id).map_err(|_| ())?;
            let stored = server.db.get_workflow_run(&id).map_err(|_| ())?.ok_or(())?;
            if &stored.workflow_graph_id != workflow_graph_id || stored.run.status.is_terminal() {
                return Err(());
            }
            // Standalone runs have no workspace binding in the persistence contract.
            None
        }
        _ => return Err(()),
    };
    if let Some(workspace) = &workspace {
        let id = WorkspaceId::new(workspace).map_err(|_| ())?;
        let row = server.db.get_workspace(&id).map_err(|_| ())?.ok_or(())?;
        if row.status != WorkspaceStatus::Active {
            return Err(());
        }
    }
    Ok(workspace)
}

fn active_goal_node(
    server: &AppServer,
    id: &NodeExecutionId,
    graph_id: &str,
    node_id: &str,
) -> Result<CancellationToken, ()> {
    let (graph, attempt) = server.task_world.find_execution(id).ok_or(())?;
    let cancel = server
        .task_world
        .execution_cancellation_token(id)
        .ok_or(())?;
    if graph.as_str() != graph_id
        || attempt.node_id.as_str() != node_id
        || !attempt.status.is_active()
        || cancel.is_cancelled()
        || server
            .task_world
            .control_state(&graph)
            .map_err(|_| ())?
            .state
            != TaskExecutionControlState::Running
    {
        return Err(());
    }
    Ok(cancel)
}

pub(super) fn reserve_direct_goal_model(
    server: &AppServer,
    owner: &ExecutionOwner,
    inherited: Option<&ExecutionScope>,
) -> Result<Option<(GoalModelReservation, ExecutionScope)>, Error> {
    let ExecutionOwner::GoalNode {
        graph_id,
        node_id,
        execution_id,
        ..
    } = owner
    else {
        return Ok(None);
    };
    if inherited.is_some_and(|scope| scope.budget.is_some()) {
        return Ok(None);
    }
    let draft = server
        .db
        .confirmed_proof_draft_for_graph(graph_id)
        .map_err(|_| budget_unavailable())?;
    let Some(draft) = draft else {
        return Ok(None);
    };
    let id = NodeExecutionId::new(execution_id).map_err(|_| cancelled())?;
    let cancel = active_goal_node(server, &id, graph_id, node_id).map_err(|_| cancelled())?;
    let remaining = server
        .proofloop_budget
        .check(&draft.contract, chrono::Utc::now().timestamp_millis())
        .map_err(goal_budget_error)?;
    let deadline = tokio::time::Instant::now() + remaining;
    let reservation = GoalModelReservation::reserve(&server.db, &draft, "goal-node-plugin")
        .map_err(goal_budget_error)?;
    let scope = ExecutionScope {
        owner: Some(owner.clone()),
        cancel,
        budget: Some(reservation.budget()),
        handoff_id: None,
        goal_id: Some(draft.contract.goal_id),
        goal_version: Some(draft.contract.version),
        evidence_ids: vec![],
        deadline: Some(deadline),
    };
    Ok(Some((reservation, scope)))
}

fn goal_budget_error(error: crate::modules::task::proofloop::ProofError) -> Error {
    if error.code == "proofloop_budget_exhausted" {
        return problem(
            StatusCode::CONFLICT,
            "proofloop_budget_exhausted",
            "目标的模型调用、token 或耗时预算已耗尽",
        );
    }
    budget_unavailable()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::types::PermissionMode,
        safety::{
            grant::{GrantEffect, GrantResource, GrantSource, SecurityGrant},
            SecuritySubject,
        },
    };

    fn server() -> AppServer {
        let server = AppServer::new_with_control_session(
            std::path::Path::new(":memory:"),
            ".",
            crate::safety::ControlSession::new("f".repeat(64)).unwrap(),
        )
        .unwrap();
        server.config.write().permissions.mode = PermissionMode::FullAccess;
        server
    }

    fn invocation(subject: SecuritySubject, conversation_id: &str) -> ToolInvocation {
        ToolInvocation {
            subject,
            conversation_id: conversation_id.into(),
            agent_id: None,
            tool_name: "fixture".into(),
            tool_call_id: "fixture-call".into(),
        }
    }

    fn authority(grant_id: Option<&str>, delegation: Option<DelegatedSubject>) -> ModelAuthority {
        ModelAuthority {
            server_id: "managed-instance".into(),
            tool_name: "test".into(),
            grant_id: grant_id.map(str::to_string),
            one_shot_approval: false,
            delegation,
        }
    }

    fn grant(
        server: &AppServer,
        id: &str,
        subject: &str,
        permission: PermissionId,
        effect: GrantEffect,
    ) {
        server
            .db
            .create_grant(&SecurityGrant {
                id: id.into(),
                subject_id: subject.into(),
                permission,
                resource: GrantResource::Any,
                effect,
                source: GrantSource::User,
                created_at: 1,
                expires_at: None,
            })
            .unwrap();
    }

    #[test]
    fn local_full_access_and_actual_approval_keep_distinct_authority() {
        let server = server();
        let caller = invocation(SecuritySubject::local_user(), "desktop-turn");
        let mut evidence = authority(None, None);
        assert!(still_authorized(&server, &caller, &evidence).is_ok());
        server.config.write().permissions.mode = PermissionMode::PartialApproval;
        assert!(still_authorized(&server, &caller, &evidence).is_err());
        evidence.one_shot_approval = true;
        assert!(still_authorized(&server, &caller, &evidence).is_ok());
        grant(
            &server,
            "local-deny",
            "local-user",
            PermissionId::McpInvoke,
            GrantEffect::Deny,
        );
        assert!(still_authorized(&server, &caller, &evidence).is_err());
        server.db.delete_grant("local-deny").unwrap();
        server.config.write().permissions.mode = PermissionMode::FullAccess;
        grant(
            &server,
            "local-allowed",
            "local-user",
            PermissionId::McpInvoke,
            GrantEffect::Allow,
        );
        evidence = authority(Some("local-allowed"), None);
        assert!(still_authorized(&server, &caller, &evidence).is_ok());
        server.db.delete_grant("local-allowed").unwrap();
        assert!(still_authorized(&server, &caller, &evidence).is_err());
    }

    #[test]
    fn delegated_configuration_uses_parent_while_permissions_follow_child_and_live_parent_denial() {
        let server = server();
        let conversation = server.db.create_conversation("trusted parent").unwrap().id;
        let mut parent = invocation(SecuritySubject::local_user(), &conversation);
        parent.agent_id = Some("persisted-agent".into());
        let subject = SecuritySubject::subagent("fixture");
        let mut child = invocation(subject.clone(), "synthetic-child-conversation");
        child.agent_id = Some("synthetic-agent".into());
        let delegation = DelegatedSubject::new(
            subject.clone(),
            "fixture",
            "handoff".into(),
            parent,
            std::path::PathBuf::from("."),
        );
        let evidence = authority(Some("child-mcp"), Some(delegation));
        let source = evidence.configuration_invocation(&child);
        assert_eq!(source.agent_id.as_deref(), Some("persisted-agent"));
        let owner = ExecutionOwner::Conversation {
            conversation_id: conversation,
        };
        assert!(active_workspace(&server, &owner, source, None).is_ok());
        assert!(active_workspace(&server, &owner, &child, None).is_err());
        assert!(still_authorized(&server, &child, &evidence).is_err());
        grant(
            &server,
            "child-mcp",
            &subject.subject_id,
            PermissionId::McpInvoke,
            GrantEffect::Allow,
        );
        assert!(still_authorized(&server, &child, &evidence).is_ok());
        grant(
            &server,
            "parent-revoked",
            "local-user",
            PermissionId::AgentDelegate,
            GrantEffect::Deny,
        );
        assert!(still_authorized(&server, &child, &evidence).is_err());
    }
}
