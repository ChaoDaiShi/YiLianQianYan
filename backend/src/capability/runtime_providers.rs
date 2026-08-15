// ============================================================
// Runtime-source providers: Subagent, Agent, Workflow, Skill.
//
// Each maps an existing registry/database source into capability descriptors.
// They never execute anything. Instructions / paths / secrets are never exposed.
// ============================================================

use async_trait::async_trait;

use super::model::{
    CapabilityDescriptor, CapabilityId, CapabilityKind, CapabilityMetadata, CapabilityPermission,
    CapabilityProviderKind, CapabilityRisk, CapabilityRuntimeStatus,
};
use super::provider::{CapabilityProvider, CapabilityProviderError};
use crate::db::Database;
use crate::server::DiscoveredSubagent;
use crate::tools::skill::DiscoveredSkill;

fn descriptor(
    id: &str,
    kind: CapabilityKind,
    provider: CapabilityProviderKind,
    name: &str,
    description: &str,
    status: CapabilityRuntimeStatus,
    enabled: bool,
    metadata: CapabilityMetadata,
) -> CapabilityDescriptor {
    CapabilityDescriptor {
        id: CapabilityId::new(id).expect("capability id is valid"),
        kind,
        provider,
        name: name.to_string(),
        description: description.to_string(),
        input_schema: None,
        risk: CapabilityRisk::Dynamic,
        permissions: Vec::<CapabilityPermission>::new(),
        status,
        enabled,
        metadata,
    }
}

// ── Subagent ──

pub struct SubagentProvider {
    subagents: Vec<DiscoveredSubagent>,
}

impl SubagentProvider {
    pub fn new(subagents: Vec<DiscoveredSubagent>) -> Self {
        Self { subagents }
    }
}

#[async_trait]
impl CapabilityProvider for SubagentProvider {
    fn provider_kind(&self) -> CapabilityProviderKind {
        CapabilityProviderKind::Subagent
    }

    async fn discover(&self) -> Result<Vec<CapabilityDescriptor>, CapabilityProviderError> {
        let descriptors = self
            .subagents
            .iter()
            .map(|s| {
                descriptor(
                    &format!("subagent.{}", s.name),
                    CapabilityKind::Subagent,
                    CapabilityProviderKind::Subagent,
                    &s.name,
                    &s.description,
                    CapabilityRuntimeStatus::Ready,
                    true,
                    CapabilityMetadata {
                        source_id: Some(s.name.clone()),
                        source_name: Some(s.name.clone()),
                        runtime_ready: true,
                        ..Default::default()
                    },
                )
            })
            .collect();
        Ok(descriptors)
    }
}

// ── Agent ──

pub struct AgentProvider {
    db: Database,
}

impl AgentProvider {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl CapabilityProvider for AgentProvider {
    fn provider_kind(&self) -> CapabilityProviderKind {
        CapabilityProviderKind::AgentRuntime
    }

    async fn discover(&self) -> Result<Vec<CapabilityDescriptor>, CapabilityProviderError> {
        let agents = self
            .db
            .list_agent_definitions()
            .map_err(|error| CapabilityProviderError::Discovery(error))?;
        let descriptors = agents
            .into_iter()
            .map(|agent| {
                let status = if agent.enabled {
                    CapabilityRuntimeStatus::Ready
                } else {
                    CapabilityRuntimeStatus::Disabled
                };
                descriptor(
                    &format!("agent.{}", agent.id),
                    CapabilityKind::Agent,
                    CapabilityProviderKind::AgentRuntime,
                    &agent.name,
                    &agent.description,
                    status,
                    agent.enabled,
                    CapabilityMetadata {
                        source_id: Some(agent.id.to_string()),
                        source_name: Some(agent.name.clone()),
                        tags: agent.capabilities,
                        runtime_ready: agent.enabled,
                        extra: serde_json::json!({
                            "max_iterations": agent.max_iterations,
                        }),
                        ..Default::default()
                    },
                )
            })
            .collect();
        Ok(descriptors)
    }
}

// ── Workflow ──

pub struct WorkflowProvider {
    db: Database,
}

impl WorkflowProvider {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl CapabilityProvider for WorkflowProvider {
    fn provider_kind(&self) -> CapabilityProviderKind {
        CapabilityProviderKind::WorkflowRuntime
    }

    async fn discover(&self) -> Result<Vec<CapabilityDescriptor>, CapabilityProviderError> {
        let graphs = self
            .db
            .list_workflow_graphs()
            .map_err(|error| CapabilityProviderError::Discovery(error))?;
        let descriptors = graphs
            .into_iter()
            .map(|graph| {
                let status = if graph.definition.validate().is_ok() {
                    CapabilityRuntimeStatus::Ready
                } else {
                    CapabilityRuntimeStatus::Misconfigured
                };
                let id = graph.id.clone();
                let name = graph.name.clone();
                let description = graph.description.clone();
                descriptor(
                    &format!("workflow.{id}"),
                    CapabilityKind::Workflow,
                    CapabilityProviderKind::WorkflowRuntime,
                    &name,
                    &description,
                    status,
                    true,
                    CapabilityMetadata {
                        source_id: Some(id),
                        source_name: Some(name.clone()),
                        runtime_ready: status == CapabilityRuntimeStatus::Ready,
                        ..Default::default()
                    },
                )
            })
            .collect();
        Ok(descriptors)
    }
}

// ── Skill ──

pub struct SkillProvider {
    skills: Vec<DiscoveredSkill>,
}

impl SkillProvider {
    pub fn new(skills: Vec<DiscoveredSkill>) -> Self {
        Self { skills }
    }
}

#[async_trait]
impl CapabilityProvider for SkillProvider {
    fn provider_kind(&self) -> CapabilityProviderKind {
        CapabilityProviderKind::SkillRuntime
    }

    async fn discover(&self) -> Result<Vec<CapabilityDescriptor>, CapabilityProviderError> {
        let descriptors = self
            .skills
            .iter()
            .map(|s| {
                let mut desc = descriptor(
                    &format!("skill.{}", s.name),
                    CapabilityKind::Skill,
                    CapabilityProviderKind::SkillRuntime,
                    &s.name,
                    &s.description,
                    CapabilityRuntimeStatus::Ready,
                    true,
                    CapabilityMetadata {
                        source_id: Some(s.name.clone()),
                        source_name: Some(s.name.clone()),
                        runtime_ready: true,
                        ..Default::default()
                    },
                );
                // Skills are metadata-only: no side-effect risk.
                desc.risk = CapabilityRisk::Low;
                desc
            })
            .collect();
        Ok(descriptors)
    }
}
