// ============================================================
// AppServer — shared state for the HTTP server
// ============================================================

use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::config::types::AppConfig;
use crate::db::Database;
use crate::safety::approval::ApprovalStore;
use crate::tools::registry::ToolRegistry;
use crate::tools::skill::SkillDiscovery;

/// A single log entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LogEntry {
    pub timestamp: i64,
    pub level: String,  // "info" | "warn" | "error" | "debug" | "tool" | "chat"
    pub source: String, // "api", "agent", "tool", "system"
    pub message: String,
}

/// Thread-safe ring buffer for in-memory application logs (cloneable)
#[derive(Clone)]
pub struct LogBuffer {
    entries: Arc<Mutex<Vec<LogEntry>>>,
    max_entries: usize,
}

impl LogBuffer {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Arc::new(Mutex::new(Vec::with_capacity(max_entries))),
            max_entries,
        }
    }

    pub fn push(&self, level: &str, source: &str, message: &str) {
        let entry = LogEntry {
            timestamp: chrono::Utc::now().timestamp_millis(),
            level: level.to_string(),
            source: source.to_string(),
            message: message.to_string(),
        };
        let mut entries = self.entries.lock();
        if entries.len() >= self.max_entries {
            entries.remove(0);
        }
        entries.push(entry);
    }

    pub fn drain(&self) -> Vec<LogEntry> {
        let mut entries = self.entries.lock();
        std::mem::take(&mut *entries)
    }

    pub fn recent(&self, count: usize) -> Vec<LogEntry> {
        let entries = self.entries.lock();
        let start = if entries.len() > count {
            entries.len() - count
        } else {
            0
        };
        entries[start..].to_vec()
    }
}

/// Subagent metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiscoveredSubagent {
    pub name: String,
    pub description: String,
    pub path: String,
    pub allowed_tools: Vec<String>,
    pub model: Option<String>,
    pub workdir: Option<String>,
}

/// Shared application server state
pub struct AppServer {
    pub db: Database,
    pub config: Arc<RwLock<AppConfig>>,
    pub tool_registry: Arc<ToolRegistry>,
    pub skill_discovery: Arc<RwLock<SkillDiscovery>>,
    pub subagents: Vec<DiscoveredSubagent>,
    /// Workspace root used for tool path resolution and verification.
    pub workspace_root: String,
    /// Active generation tasks (conversation_id → cancel token)
    pub active_tasks: Mutex<HashMap<String, CancellationToken>>,
    /// In-memory log ring buffer
    pub log_buffer: LogBuffer,
    /// Pending high-risk tool approvals awaiting user decision
    pub approval_store: ApprovalStore,
}

impl AppServer {
    pub fn new(db_path: &std::path::Path, workspace_root: &str) -> Result<Self, String> {
        let db = Database::new(db_path).map_err(|e| e.to_string())?;
        let mut config = db.get_settings().unwrap_or_default();

        // Auto-migrate old system prompts to the new one
        let needs_migration = config.agent.system_prompt.is_empty()
            || config.agent.system_prompt.contains("Flow 工作流编排 Agent")
            || config.agent.system_prompt.contains("LangGraph")
            || config.agent.system_prompt.contains("用 2-3 句话概括");
        if needs_migration {
            config.agent.system_prompt = crate::config::types::AgentConfig::default().system_prompt;
            let _ = db.save_settings(&config);
            tracing::info!("System prompt migrated to new version");
        }

        let tool_registry = Arc::new(ToolRegistry::with_defaults(workspace_root));

        let skill_dirs = if config.skills.directories.is_empty() {
            vec!["./skills".to_string()]
        } else {
            config.skills.directories.clone()
        };
        let skill_discovery = Arc::new(RwLock::new(SkillDiscovery::discover(
            &skill_dirs,
            workspace_root,
        )));

        let subagent_dirs = if config.subagents.directories.is_empty() {
            vec!["./.agents/agents".to_string()]
        } else {
            config.subagents.directories.clone()
        };
        let subagents = Self::discover_subagents(workspace_root, &subagent_dirs);

        let log_buffer = LogBuffer::new(2000);

        Ok(Self {
            db,
            config: Arc::new(RwLock::new(config)),
            tool_registry,
            skill_discovery,
            subagents,
            workspace_root: workspace_root.to_string(),
            active_tasks: Mutex::new(HashMap::new()),
            log_buffer,
            approval_store: ApprovalStore::new(),
        })
    }

    /// Build the full system prompt with skills + subagents + memories sections
    pub fn build_system_prompt(&self) -> String {
        let config = self.config.read();
        let mut prompt = config.agent.system_prompt.clone();

        // Inject relevant long-term memories
        let memories = self.db.get_relevant_memories("", 10).unwrap_or_default();
        if !memories.is_empty() {
            prompt.push_str("\n\n## 用户长期记忆 (Long-term Memories)\n\n");
            prompt
                .push_str("以下是从之前对话中提取的关于用户的重要信息和偏好，请在回答时参考：\n\n");
            for mem in &memories {
                let category_label = match mem.category.as_str() {
                    "fact" => "事实",
                    "preference" => "偏好",
                    "knowledge" => "知识",
                    "note" => "笔记",
                    _ => &mem.category,
                };
                prompt.push_str(&format!("- [{}] {}\n", category_label, mem.content));
            }
        }

        let sd = self.skill_discovery.read();
        if sd.has_skills() {
            prompt.push_str("\n\n");
            prompt.push_str(&sd.render_prompt_section());
        }

        if !self.subagents.is_empty() {
            prompt.push_str("\n\n## Available Subagents\n\n");
            for sub in &self.subagents {
                prompt.push_str(&format!(
                    "- **{}**: {} (tools: {})\n",
                    sub.name,
                    sub.description,
                    sub.allowed_tools.join(", ")
                ));
            }
        }

        prompt
    }

    fn discover_subagents(root: &str, dirs: &[String]) -> Vec<DiscoveredSubagent> {
        let mut agents = Vec::new();
        for dir in dirs {
            let resolved = std::path::Path::new(root).join(dir);
            if !resolved.exists() {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(&resolved) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_dir() {
                        continue;
                    }
                    let agent_md = path.join("AGENT.md");
                    if !agent_md.exists() {
                        continue;
                    }
                    let name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    if let Ok(content) = std::fs::read_to_string(&agent_md) {
                        let desc = Self::extract_fm(&content, "description").unwrap_or_default();
                        let tools: Vec<String> = Self::extract_fm(&content, "tools")
                            .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
                            .unwrap_or_default();
                        let model = Self::extract_fm(&content, "model");
                        let workdir = Self::extract_fm(&content, "workdir");
                        agents.push(DiscoveredSubagent {
                            name,
                            description: desc,
                            path: agent_md.display().to_string(),
                            allowed_tools: tools,
                            model,
                            workdir,
                        });
                    }
                }
            }
        }
        agents
    }

    fn extract_fm(content: &str, field: &str) -> Option<String> {
        let mut in_fm = false;
        let prefix = format!("{}:", field);
        for line in content.lines() {
            let t = line.trim();
            if t == "---" {
                in_fm = !in_fm;
                continue;
            }
            if in_fm && t.starts_with(&prefix) {
                return Some(
                    t[prefix.len()..]
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string(),
                );
            }
        }
        None
    }
}
