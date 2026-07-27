// ============================================================
// ToolRegistry — manages the collection of available tools
// ============================================================

use std::collections::HashMap;
use std::sync::Arc;

use super::trait_def::{Tool, ToolInfo};
use super::bash::BashTool;
use super::fs::{ReadFileTool, WriteFileTool, EditFileTool};
use super::search::{GrepTool, GlobTool};
use super::http_client::HttpRequestTool;
use super::skill::{LoadSkillTool, WriteTodosTool};
use super::process::ProcessTool;
use super::input::{MouseTool, KeyboardTool};
use super::screenshot::ScreenshotTool;

/// Registry holding all available tools
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// Create an empty registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Create a registry with all default built-in tools
    pub fn with_defaults(workspace_root: &str) -> Self {
        let mut registry = Self::new();

        // System control tools
        registry.register(Arc::new(BashTool::new(workspace_root)));

        // File system tools
        registry.register(Arc::new(ReadFileTool::new(workspace_root)));
        registry.register(Arc::new(WriteFileTool::new(workspace_root)));
        registry.register(Arc::new(EditFileTool::new(workspace_root)));

        // Search tools
        registry.register(Arc::new(GrepTool::new(workspace_root)));
        registry.register(Arc::new(GlobTool::new(workspace_root)));

        // HTTP tool
        registry.register(Arc::new(HttpRequestTool::new()));

        // Skill management
        registry.register(Arc::new(LoadSkillTool::new(workspace_root)));

        // Task planning
        registry.register(Arc::new(WriteTodosTool));

        // Process management
        registry.register(Arc::new(ProcessTool));

        // Input simulation tools
        registry.register(Arc::new(MouseTool));
        registry.register(Arc::new(KeyboardTool));

        // Screenshot tool
        registry.register(Arc::new(ScreenshotTool));

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

    /// Execute a tool by name
    pub async fn execute(&self, name: &str, args: serde_json::Value) -> Option<super::ToolResult> {
        if let Some(tool) = self.tools.get(name) {
            Some(tool.execute(args).await)
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
