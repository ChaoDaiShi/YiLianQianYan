// ============================================================
// ToolRegistry — manages the collection of available tools
// ============================================================

use std::collections::HashMap;
use std::sync::Arc;

use super::bash::BashTool;
use super::fs::{EditFileTool, ReadFileTool, WriteFileTool};
use super::gui_launch::{OpenApplicationTool, OpenUrlTool};
use super::http_client::HttpRequestTool;
use super::input::{KeyboardTool, MouseTool};
use super::process::ProcessTool;
use super::screenshot::ScreenshotTool;
use super::search::{GlobTool, GrepTool};
use super::skill::{LoadSkillTool, WriteTodosTool};
use super::trait_def::{RiskLevel, Tool, ToolExecutionContext, ToolInfo};
use super::upscale::UpscaleTool;

/// Registry holding all available tools
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    process_registry: crate::isolation::SharedManagedProcessRegistry,
}

impl ToolRegistry {
    /// Create an empty registry
    pub fn new() -> Self {
        Self::new_with_process_registry(std::sync::Arc::new(
            crate::isolation::ManagedProcessRegistry::new(),
        ))
    }

    pub fn new_with_process_registry(
        process_registry: crate::isolation::SharedManagedProcessRegistry,
    ) -> Self {
        Self {
            tools: HashMap::new(),
            process_registry,
        }
    }

    /// Create a registry with all default built-in tools
    pub fn with_defaults(workspace_root: &str) -> Self {
        Self::with_defaults_and_process_registry(
            workspace_root,
            std::sync::Arc::new(crate::isolation::ManagedProcessRegistry::new()),
        )
    }

    pub fn with_defaults_and_process_registry(
        workspace_root: &str,
        process_registry: crate::isolation::SharedManagedProcessRegistry,
    ) -> Self {
        let mut registry = Self::new_with_process_registry(process_registry);

        // System control tools
        registry.register(Arc::new(BashTool::new(workspace_root)));

        // File system tools
        registry.register(Arc::new(ReadFileTool::new(workspace_root)));
        registry.register(Arc::new(WriteFileTool::new(workspace_root)));
        registry.register(Arc::new(EditFileTool::new(workspace_root)));

        // Search tools
        registry.register(Arc::new(GrepTool::new(workspace_root)));
        registry.register(Arc::new(GlobTool::new(workspace_root)));

        // HTTP and visible GUI launch tools
        registry.register(Arc::new(HttpRequestTool::new()));
        registry.register(Arc::new(OpenUrlTool::new()));
        registry.register(Arc::new(OpenApplicationTool::new()));

        // Skill management
        registry.register(Arc::new(LoadSkillTool::new(workspace_root)));

        // Task planning
        registry.register(Arc::new(WriteTodosTool));

        // Process management
        registry.register(Arc::new(ProcessTool));

        // Input simulation tools
        registry.register(Arc::new(MouseTool));
        registry.register(Arc::new(KeyboardTool::new()));

        // Screenshot tool
        registry.register(Arc::new(ScreenshotTool));

        // AI image upscale tool
        registry.register(Arc::new(UpscaleTool::new()));

        registry
    }

    /// Register a tool
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    pub fn process_registry(&self) -> crate::isolation::SharedManagedProcessRegistry {
        std::sync::Arc::clone(&self.process_registry)
    }

    /// List all registered tools
    pub fn all(&self) -> Vec<&Arc<dyn Tool>> {
        self.tools.values().collect()
    }

    /// Convert all tools to OpenAI-compatible function definitions
    pub fn to_openai_tools(&self) -> Vec<serde_json::Value> {
        self.tools.values().map(|t| t.to_openai_tool()).collect()
    }

    /// List all tools as ToolInfo for frontend display
    pub fn list_tools(&self) -> Vec<ToolInfo> {
        self.tools.values().map(|t| t.to_info()).collect()
    }

    /// Compatibility shim: direct registry execution is disabled. All tools
    /// must be dispatched through the SecurityExecutionGateway.
    pub async fn execute(&self, name: &str, _args: serde_json::Value) -> Option<super::ToolResult> {
        if self.tools.contains_key(name) {
            Some(super::ToolResult::error(
                "direct ToolRegistry execution is disabled; use SecurityExecutionGateway",
            ))
        } else {
            None
        }
    }

    pub async fn execute_with_context(
        &self,
        name: &str,
        args: serde_json::Value,
        context: &ToolExecutionContext,
    ) -> Option<super::ToolResult> {
        if let Some(tool) = self.tools.get(name) {
            Some(tool.execute_with_context(args, context).await)
        } else {
            None
        }
    }

    /// Check if a tool requires user approval
    pub fn requires_approval(&self, name: &str) -> bool {
        self.tools
            .get(name)
            .map(|t| t.requires_approval())
            .unwrap_or(false)
    }

    /// Get the default risk level of a tool by name.
    pub fn risk_level(&self, name: &str) -> Option<RiskLevel> {
        self.tools.get(name).map(|t| t.risk_level())
    }

    /// Get tool count
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if no tools registered
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::ToolRegistry;

    #[tokio::test]
    async fn direct_execution_of_side_effecting_tool_requires_gateway() {
        let registry = ToolRegistry::with_defaults(".");
        let result = registry
            .execute(
                "bash",
                serde_json::json!({ "command": "Write-Output bypass" }),
            )
            .await
            .expect("registered bash tool");
        assert!(!result.ok);
        assert!(result.content.contains("SecurityExecutionGateway"));
    }

    #[test]
    fn default_registry_exposes_structured_visible_gui_tools() {
        let registry = ToolRegistry::with_defaults(".");

        assert!(registry.get("open_url").is_some());
        assert!(registry.get("open_application").is_some());
    }
}
