// ============================================================
// Skill tool — progressive loading of SKILL.md files
//
// Mirrors the TypeScript skill system:
// - Skills are discovered from configured directories
// - SKILL.md files define skill metadata (name, description)
// - load_skill tool loads a skill into context on demand
// - Skills rendered into system prompt for LLM awareness
// ============================================================

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::trait_def::{Tool, ToolResult};

/// A discovered skill from a SKILL.md file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredSkill {
    /// Skill name (derived from directory name)
    pub name: String,
    /// Short description (first paragraph or frontmatter)
    pub description: String,
    /// Path to the SKILL.md file
    pub path: PathBuf,
    /// The skill's root directory (for resolving references/)
    pub root_dir: PathBuf,
    /// Metadata from the originating read-only directory. `name` remains the
    /// unique load identifier; display_name preserves SKILL.md's own name.
    pub display_name: String,
    pub source_id: String,
    pub source: String,
    pub source_kind: String,
    pub source_root: PathBuf,
    pub compatibility: SkillCompatibility,
    /// Whether the full content has been loaded
    #[serde(skip)]
    pub loaded_content: Option<String>,
}

impl DiscoveredSkill {
    /// Load the full SKILL.md content
    pub fn load_content(&mut self) -> Result<String, String> {
        if let Some(ref cached) = self.loaded_content {
            return Ok(cached.clone());
        }
        let content = std::fs::read_to_string(&self.path)
            .map_err(|e| format!("无法读取 {}: {}", self.path.display(), e))?;
        self.loaded_content = Some(content.clone());
        Ok(content)
    }

    /// Load a specific references/ subfile (by name or prefix match)
    pub fn load_reference(&self, part: &str) -> Result<Option<String>, String> {
        let refs_dir = self.root_dir.join("references");
        if !refs_dir.exists() || !refs_dir.is_dir() {
            return Ok(None);
        }

        // Try exact match first, then prefix match
        for entry in std::fs::read_dir(&refs_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let fname = entry.file_name().to_string_lossy().to_string();
            let stem = Path::new(&fname)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            if stem == part || stem.starts_with(part) {
                let content = std::fs::read_to_string(entry.path())
                    .map_err(|e| format!("无法读取参考文件: {}", e))?;
                return Ok(Some(content));
            }
        }

        Ok(None)
    }
}

mod discovery;
pub use discovery::SkillDiscovery;
mod compatibility;
pub use compatibility::SkillCompatibility;
#[cfg(test)]
mod discovery_tests;

// ── load_skill Tool ──

pub struct LoadSkillTool {
    catalog: SkillCatalog,
}
enum SkillCatalog {
    Workspace(String),
    Shared(std::sync::Arc<parking_lot::RwLock<SkillDiscovery>>),
}

impl LoadSkillTool {
    pub fn new(workspace_root: &str) -> Self {
        Self {
            catalog: SkillCatalog::Workspace(workspace_root.to_string()),
        }
    }
    pub fn with_discovery(discovery: std::sync::Arc<parking_lot::RwLock<SkillDiscovery>>) -> Self {
        Self {
            catalog: SkillCatalog::Shared(discovery),
        }
    }
}

#[async_trait]
impl Tool for LoadSkillTool {
    fn name(&self) -> &str {
        "load_skill"
    }

    fn description(&self) -> &str {
        "按需加载指定skill的SKILL.md完整内容。用于获取某个领域技能的详细指引。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "要加载的skill名称"
                },
                "part": {
                    "type": "string",
                    "description": "可选：加载skill的references/子文件（按文件名匹配）"
                }
            },
            "required": ["name"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        let name = args["name"].as_str().unwrap_or("");
        let part = args["part"].as_str();

        if name.is_empty() {
            return ToolResult::error("name不能为空");
        }

        let (skill, available) = match &self.catalog {
            SkillCatalog::Shared(discovery) => {
                let discovery = discovery.read();
                (
                    discovery.get(name).cloned(),
                    discovery.available_tools().clone(),
                )
            }
            SkillCatalog::Workspace(root) => {
                let discovery = SkillDiscovery::discover(&["./skills".to_string()], root);
                (
                    discovery.get(name).cloned(),
                    discovery.available_tools().clone(),
                )
            }
        };
        let Some(mut skill) = skill else {
            return ToolResult::error(format!(
                "未找到skill '{}'。可用的skills请查看系统提示词中的 Available Skills 列表。",
                name
            ));
        };
        if !skill.compatibility.runtime_ready {
            return ToolResult::error(skill.compatibility.reason.clone()).with_structured(Some(
                serde_json::json!({"compatibility":skill.compatibility}),
            ));
        }
        if let Some(part_name) = part {
            return match skill.load_reference(part_name) {
                Ok(Some(content)) => {
                    let compatibility = SkillCompatibility::inspect(&content, &available);
                    if !compatibility.runtime_ready {
                        return ToolResult::error(compatibility.reason.clone()).with_structured(
                            Some(serde_json::json!({"compatibility":compatibility})),
                        );
                    }
                    ToolResult::success(format!(
                    "{}\n# {} - {}\n\n{}\n\n技能根目录: {}",
                    compatibility.execution_notes(),
                    name,
                    part_name,
                    content,
                    skill.root_dir.display()
                )).with_structured(Some(serde_json::json!({"name":name,"root_dir":skill.root_dir,"compatibility":compatibility})))
                }
                Ok(None) => {
                    ToolResult::error(format!("Skill '{}' 没有找到引用文件 '{}'", name, part_name))
                }
                Err(error) => ToolResult::error(error),
            };
        }
        match skill.load_content() {
            Ok(content) => {
                let compatibility = SkillCompatibility::inspect(&content, &available);
                if !compatibility.runtime_ready {
                    return ToolResult::error(compatibility.reason.clone())
                        .with_structured(Some(serde_json::json!({"compatibility":compatibility})));
                }
                ToolResult::success(format!(
                "{}\n# Skill: {}\n来源: {}\n\n{}\n\n---\n技能根目录: {}",
                compatibility.execution_notes(),
                name,
                skill.source,
                content,
                skill.root_dir.display()
            )).with_structured(Some(serde_json::json!({"name":name,"root_dir":skill.root_dir,"compatibility":compatibility})))
            }
            Err(error) => ToolResult::error(error),
        }
    }
}

// ── write_todos Tool ──

pub struct WriteTodosTool;

#[async_trait]
impl Tool for WriteTodosTool {
    fn name(&self) -> &str {
        "write_todos"
    }

    fn description(&self) -> &str {
        "创建和更新待办事项列表。用于规划复杂、多步骤的任务。每次调用传入完整的待办列表快照（非增量）。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "description": "完整的待办事项列表",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string", "description": "唯一标识符" },
                            "content": { "type": "string", "description": "待办内容" },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed", "cancelled"],
                                "description": "状态"
                            },
                            "priority": {
                                "type": "string",
                                "enum": ["high", "medium", "low"],
                                "description": "优先级"
                            }
                        },
                        "required": ["content", "status"]
                    }
                }
            },
            "required": ["todos"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        let todos = match args["todos"].as_array() {
            Some(t) => t,
            None => return ToolResult::error("todos必须是一个数组"),
        };

        let mut output = String::from("## 待办事项\n\n");
        let status_icons = |s: &str| -> &str {
            match s {
                "completed" => "✅",
                "in_progress" => "🔄",
                "cancelled" => "❌",
                _ => "⬜",
            }
        };

        for (i, todo) in todos.iter().enumerate() {
            let content = todo["content"].as_str().unwrap_or("(无内容)");
            let status = todo["status"].as_str().unwrap_or("pending");
            let priority = todo["priority"].as_str().unwrap_or("");
            let id = todo["id"].as_str().unwrap_or("");

            let id_str = if id.is_empty() {
                format!("{}", i + 1)
            } else {
                id.to_string()
            };

            let prio_str = match priority {
                "high" => " 🔴",
                "medium" => " 🟡",
                "low" => " 🟢",
                _ => "",
            };

            output.push_str(&format!(
                "{} {}. {} `{}`{}\n",
                status_icons(status),
                id_str,
                content,
                status,
                prio_str
            ));
        }

        let counts: Vec<String> = ["pending", "in_progress", "completed", "cancelled"]
            .iter()
            .map(|s| {
                let count = todos
                    .iter()
                    .filter(|t| t["status"].as_str().unwrap_or("") == *s)
                    .count();
                format!("{} {}= {}", status_icons(s), s, count)
            })
            .filter(|s| !s.contains("= 0"))
            .collect();

        output.push_str(&format!("\n统计: {}", counts.join(" | ")));
        output.push_str("\n\n提示: 每次调用请传入完整的待办列表快照，不要只传增量。");

        ToolResult::success(output)
    }
}
