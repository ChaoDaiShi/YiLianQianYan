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
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing;

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

/// Skill discovery engine — scans configured directories for SKILL.md files
pub struct SkillDiscovery {
    skills: HashMap<String, DiscoveredSkill>,
}

impl SkillDiscovery {
    /// Discover skills from configured directories
    pub fn discover(directories: &[String], workspace_root: &str) -> Self {
        let mut skills = HashMap::new();

        for dir in directories {
            let resolved = if Path::new(dir).is_absolute() {
                PathBuf::from(dir)
            } else {
                Path::new(workspace_root).join(dir)
            };

            if !resolved.exists() {
                tracing::debug!("Skill directory not found: {}", resolved.display());
                continue;
            }

            Self::scan_directory(&resolved, &mut skills);
        }

        tracing::info!("Discovered {} skills from {:?}", skills.len(), directories);
        Self { skills }
    }

    /// Scan a directory for skill subdirectories containing SKILL.md
    fn scan_directory(dir: &Path, skills: &mut HashMap<String, DiscoveredSkill>) {
        if !dir.is_dir() {
            return;
        }

        // Check if this directory itself has a SKILL.md
        let skill_md = dir.join("SKILL.md");
        if skill_md.exists() {
            let name = dir
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let description = Self::extract_description(&skill_md);
            skills.insert(
                name.clone(),
                DiscoveredSkill {
                    name,
                    description,
                    path: skill_md,
                    root_dir: dir.to_path_buf(),
                    loaded_content: None,
                },
            );
            return; // Don't recurse into skill directories
        }

        // Recursively scan subdirectories (max depth 2)
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    Self::scan_directory(&path, skills);
                }
            }
        }
    }

    /// Extract description from SKILL.md (first heading content or first paragraph)
    fn extract_description(path: &Path) -> String {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                // Try to find first paragraph after # heading
                let mut in_first_heading = false;
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("# ") {
                        in_first_heading = true;
                        // The heading itself might describe
                        let heading = trimmed.trim_start_matches("# ").to_string();
                        if heading.len() > 3 {
                            return heading;
                        }
                        continue;
                    }
                    if in_first_heading && !trimmed.is_empty() {
                        return trimmed.to_string();
                    }
                }
                "无描述".to_string()
            }
            Err(_) => "无法读取".to_string(),
        }
    }

    /// Get all discovered skills
    pub fn all(&self) -> Vec<&DiscoveredSkill> {
        self.skills.values().collect()
    }

    /// Get a skill by name
    pub fn get(&self, name: &str) -> Option<&DiscoveredSkill> {
        self.skills.get(name)
    }

    /// Get mutable reference to a skill (for lazy loading content)
    pub fn get_mut(&mut self, name: &str) -> Option<&mut DiscoveredSkill> {
        self.skills.get_mut(name)
    }

    /// Render skills section for system prompt
    pub fn render_prompt_section(&self) -> String {
        if self.skills.is_empty() {
            return String::new();
        }

        let mut section = String::from("## Available Skills\n\n");
        section.push_str("你可以使用 `load_skill` 工具按需加载以下技能的完整内容：\n\n");

        for skill in self.skills.values() {
            section.push_str(&format!("- **{}**: {}\n", skill.name, skill.description));
        }

        section.push_str("\n使用 `load_skill(name=\"<名称>\")` 加载技能的完整指引。");
        section
    }

    /// Check if any skills are available
    pub fn has_skills(&self) -> bool {
        !self.skills.is_empty()
    }
}

// ── load_skill Tool ──

pub struct LoadSkillTool {
    skills_dir: PathBuf,
    workspace_root: String,
}

impl LoadSkillTool {
    pub fn new(workspace_root: &str) -> Self {
        Self {
            skills_dir: PathBuf::from(workspace_root).join("skills"),
            workspace_root: workspace_root.to_string(),
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

        // Search in multiple locations
        let search_paths = vec![
            self.skills_dir.join(name).join("SKILL.md"),
            Path::new(&self.workspace_root)
                .join("skills")
                .join(name)
                .join("SKILL.md"),
            Path::new(&self.workspace_root)
                .join(".agents")
                .join(name)
                .join("SKILL.md"),
        ];

        for skill_path in &search_paths {
            if skill_path.exists() {
                let root_dir = skill_path.parent().unwrap_or(Path::new("."));
                let mut skill = DiscoveredSkill {
                    name: name.to_string(),
                    description: String::new(),
                    path: skill_path.clone(),
                    root_dir: root_dir.to_path_buf(),
                    loaded_content: None,
                };

                // If asking for a specific reference part
                if let Some(part_name) = part {
                    match skill.load_reference(part_name) {
                        Ok(Some(content)) => {
                            return ToolResult::success(format!(
                                "# {} - {}\n\n{}",
                                name, part_name, content
                            ));
                        }
                        Ok(None) => {
                            return ToolResult::error(format!(
                                "Skill '{}' 没有找到引用文件 '{}'",
                                name, part_name
                            ));
                        }
                        Err(e) => return ToolResult::error(e),
                    }
                }

                // Load full SKILL.md
                match skill.load_content() {
                    Ok(content) => {
                        return ToolResult::success(format!(
                            "# Skill: {}\n\n{}\n\n---\n技能根目录: {}",
                            name,
                            content,
                            root_dir.display()
                        ));
                    }
                    Err(e) => return ToolResult::error(e),
                }
            }
        }

        ToolResult::error(format!(
            "未找到skill '{}'。可用的skills请查看系统提示词中的 Available Skills 列表。",
            name
        ))
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
