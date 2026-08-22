// ============================================================
// File system tools — read_file, write_file, edit_file
// ============================================================

use async_trait::async_trait;
use std::path::Path;

use super::trait_def::{RiskLevel, Tool, ToolResult};

/// Resolve a path relative to the workspace root
fn resolve_path(workspace_root: &str, path: &str) -> std::path::PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        Path::new(workspace_root).join(p)
    }
}

// ── read_file ──

pub struct ReadFileTool {
    workspace_root: String,
}

impl ReadFileTool {
    pub fn new(workspace_root: &str) -> Self {
        Self {
            workspace_root: workspace_root.to_string(),
        }
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "读取文件内容。返回文件文本内容，最多50000字符。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "要读取的文件路径（相对或绝对路径）"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        let path_str = args["path"].as_str().unwrap_or("");
        if path_str.is_empty() {
            return ToolResult::error("path不能为空");
        }

        let file_path = resolve_path(&self.workspace_root, path_str);

        match std::fs::read_to_string(&file_path) {
            Ok(content) => {
                if content.len() > 50000 {
                    let truncated: String = content.chars().take(50000).collect();
                    ToolResult::success(format!(
                        "{}\n\n... (文件过大，已截断至50000字符)",
                        truncated
                    ))
                } else {
                    ToolResult::success(content)
                }
            }
            Err(e) => ToolResult::error(format!("无法读取文件 {}: {}", file_path.display(), e)),
        }
    }
}

// ── write_file ──

pub struct WriteFileTool {
    workspace_root: String,
}

impl WriteFileTool {
    pub fn new(workspace_root: &str) -> Self {
        Self {
            workspace_root: workspace_root.to_string(),
        }
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "将内容写入文件。如果文件已存在则覆盖，不存在则创建。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "要写入的文件路径"
                },
                "content": {
                    "type": "string",
                    "description": "要写入的文件内容"
                }
            },
            "required": ["path", "content"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Medium
    }

    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        let path_str = args["path"].as_str().unwrap_or("");
        let content = args["content"].as_str().unwrap_or("");

        if path_str.is_empty() {
            return ToolResult::error("path不能为空");
        }

        let file_path = resolve_path(&self.workspace_root, path_str);

        // Create parent directories if needed
        if let Some(parent) = file_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return ToolResult::error(format!("无法创建目录 {}: {}", parent.display(), e));
            }
        }

        match std::fs::write(&file_path, content) {
            Ok(_) => ToolResult::success(format!(
                "文件已写入: {} ({} 字符)",
                file_path.display(),
                content.len()
            )),
            Err(e) => ToolResult::error(format!("无法写入文件 {}: {}", file_path.display(), e)),
        }
    }
}

// ── edit_file ──

pub struct EditFileTool {
    workspace_root: String,
}

impl EditFileTool {
    pub fn new(workspace_root: &str) -> Self {
        Self {
            workspace_root: workspace_root.to_string(),
        }
    }
}

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "在文件中查找并替换文本。只替换第一次出现的匹配。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "要编辑的文件路径"
                },
                "find": {
                    "type": "string",
                    "description": "要查找的文本（精确匹配）"
                },
                "replace": {
                    "type": "string",
                    "description": "替换为的文本"
                }
            },
            "required": ["path", "find", "replace"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Medium
    }

    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        let path_str = args["path"].as_str().unwrap_or("");
        let find = args["find"].as_str().unwrap_or("");
        let replace = args["replace"].as_str().unwrap_or("");

        if path_str.is_empty() {
            return ToolResult::error("path不能为空");
        }
        if find.is_empty() {
            return ToolResult::error("find不能为空");
        }

        let file_path = resolve_path(&self.workspace_root, path_str);

        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(e) => {
                return ToolResult::error(format!("无法读取文件 {}: {}", file_path.display(), e))
            }
        };

        if let Some(pos) = content.find(find) {
            let new_content = format!(
                "{}{}{}",
                &content[..pos],
                replace,
                &content[pos + find.len()..]
            );

            match std::fs::write(&file_path, &new_content) {
                Ok(_) => ToolResult::success(format!(
                    "文件已编辑: {} — 替换了第{}个字符处的内容",
                    file_path.display(),
                    pos + 1
                )),
                Err(e) => ToolResult::error(format!("无法写入文件 {}: {}", file_path.display(), e)),
            }
        } else {
            ToolResult::error(format!(
                "在文件 {} 中未找到要替换的文本",
                file_path.display()
            ))
        }
    }
}
