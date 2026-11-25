use crate::{
    app::state::AppServer,
    modules::harness_profiles as harness,
    safety::{
        PermissionId, RequestedPermission, ResourceDescriptor, ResourceScope,
        ToolSecurityDescriptor,
    },
    tools::{Tool, ToolResult},
};
use serde_json::json;
use std::{collections::HashSet, sync::Arc};

pub(super) struct ConnectorWorkflowTool {
    pub(super) server: Arc<AppServer>,
    pub(super) graph_ids: HashSet<String>,
    pub(super) agent_id: Option<String>,
    pub(super) selection: Option<harness::Selection>,
}
#[async_trait::async_trait]
impl Tool for ConnectorWorkflowTool {
    fn name(&self) -> &str {
        "run_workflow"
    }
    fn description(&self) -> &str {
        "运行当前配置中的工作流，返回真实运行 ID，节点仍经执行网关授权。"
    }
    fn parameters(&self) -> serde_json::Value {
        json!({"type":"object","properties":{"graph_id":{"type":"string","enum":self.graph_ids.iter().collect::<Vec<_>>()}},"required":["graph_id"],"additionalProperties":false})
    }
    fn security_descriptor(
        &self,
        args: &serde_json::Value,
    ) -> Result<ToolSecurityDescriptor, crate::safety::DescriptorError> {
        let id = args["graph_id"]
            .as_str()
            .filter(|id| self.graph_ids.contains(*id))
            .ok_or_else(|| {
                crate::safety::DescriptorError::InvalidDescriptor("工作流未绑定到当前智能体".into())
            })?;
        Ok(ToolSecurityDescriptor {
            tool_name: self.name().into(),
            requested_permissions: vec![RequestedPermission {
                permission: PermissionId::AgentPlan,
                scope: ResourceScope::AgentInternal,
            }],
            resources: vec![ResourceDescriptor::Agent {
                action: format!("run_workflow:{id}"),
            }],
            default_risk: crate::tools::RiskLevel::Medium,
            side_effects: vec![],
        })
    }
    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        let Some(id) = args["graph_id"]
            .as_str()
            .filter(|id| self.graph_ids.contains(*id))
        else {
            return ToolResult::error("工作流未绑定");
        };
        let work = crate::modules::workflow::application::run_service::start_workflow_run_selected(
            &self.server,
            id,
            self.selection.clone(),
        );
        let result = match &self.agent_id {
            Some(agent) => {
                crate::safety::execution_gateway::CURRENT_AGENT_ID
                    .scope(agent.clone(), work)
                    .await
            }
            None => work.await,
        };
        match result {
   Ok(run)=>ToolResult::success(json!({"run_id":run.run_id,"execution_id":run.execution_id,"status":run.status,"message":"工作流已启动，尚未确认完成"}).to_string()),Err(error)=>ToolResult::error(error.to_string())
  }
    }
}
