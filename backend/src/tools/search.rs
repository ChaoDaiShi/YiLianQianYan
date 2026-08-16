// ============================================================
// Search tools — grep (regex content search) and glob (file matching)
// ============================================================

use async_trait::async_trait;
use regex::Regex;
use std::path::Path;

use super::trait_def::{Tool, ToolResult};
use crate::utils::text::truncate_chars;

// ── grep ──

pub struct GrepTool {
    workspace_root: String,
}

impl GrepTool {
    pub fn new(workspace_root: &str) -> Self {
        Self {
            workspace_root: workspace_root.to_string(),
        }
    }

    fn should_skip_dir(name: &str) -> bool {
        matches!(
            name,
            "node_modules" | ".git" | "target" | "dist" | ".next" | "__pycache__" | ".venv"
        )
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "在工作区内使用正则表达式搜索文件内容。自动跳过node_modules/.git/target等目录。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "正则表达式搜索模式"
                },
                "path": {
                    "type": "string",
                    "description": "搜索路径（相对于工作区根目录），默认为工作区根目录"
                },
                "glob_filter": {
                    "type": "string",
                    "description": "文件名过滤模式（如 *.rs 或 **/*.toml）"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        let pattern = args["pattern"].as_str().unwrap_or("");
        if pattern.is_empty() {
            return ToolResult::error("pattern不能为空");
        }

        let regex = match Regex::new(pattern) {
            Ok(r) => r,
            Err(e) => return ToolResult::error(format!("无效的正则表达式: {}", e)),
        };

        let search_root = if let Some(p) = args["path"].as_str() {
            if Path::new(p).is_absolute() {
                Path::new(p).to_path_buf()
            } else {
                Path::new(&self.workspace_root).join(p)
            }
        } else {
            Path::new(&self.workspace_root).to_path_buf()
        };

        let glob_pattern = args["glob_filter"].as_str();

        let mut results = Vec::new();
        let mut file_count = 0;
        let mut match_count = 0;

        if let Err(e) = Self::walk_dir(
            &search_root,
            &regex,
            glob_pattern,
            &mut results,
            &mut file_count,
            &mut match_count,
            0,
        ) {
            return ToolResult::error(format!("搜索错误: {}", e));
        }

        if results.is_empty() {
            return ToolResult::success("未找到匹配项");
        }

        // Limit results
        if results.len() > 100 {
            results.truncate(100);
            results.push(format!(
                "\n... (结果过多，仅显示前100条。共搜索{}个文件，找到{}个匹配)",
                file_count, match_count
            ));
        }

        ToolResult::success(results.join("\n"))
    }
}

impl GrepTool {
    fn walk_dir(
        dir: &Path,
        regex: &Regex,
        glob_filter: Option<&str>,
        results: &mut Vec<String>,
        file_count: &mut usize,
        match_count: &mut usize,
        depth: usize,
    ) -> Result<(), String> {
        if depth > 10 || results.len() >= 100 {
            return Ok(());
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            // Never follow symlinks / junctions / reparse points: a symlink can
            // point outside the (gateway-authorized) search root.
            if file_type.is_symlink() {
                continue;
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy();

            if file_type.is_dir() {
                if Self::should_skip_dir(&name) {
                    continue;
                }
                Self::walk_dir(
                    &path,
                    regex,
                    glob_filter,
                    results,
                    file_count,
                    match_count,
                    depth + 1,
                )?;
            } else if file_type.is_file() {
                // Apply glob filter if specified
                if let Some(glob) = glob_filter {
                    let path_str = path.to_string_lossy();
                    let pattern = glob::Pattern::new(glob).map_err(|e| e.to_string())?;
                    if !pattern.matches(&path_str) {
                        continue;
                    }
                }

                *file_count += 1;

                if let Ok(content) = std::fs::read_to_string(&path) {
                    for (line_num, line) in content.lines().enumerate() {
                        if regex.is_match(line) {
                            *match_count += 1;
                            let relative =
                                path.strip_prefix(Path::new("")).unwrap_or(&path).display();
                            let preview = truncate_chars(line, 200);
                            results.push(format!("{}:{}: {}", relative, line_num + 1, preview));

                            if results.len() >= 100 {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

// ── glob ──

pub struct GlobTool {
    workspace_root: String,
}

impl GlobTool {
    pub fn new(workspace_root: &str) -> Self {
        Self {
            workspace_root: workspace_root.to_string(),
        }
    }
}

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "使用glob模式匹配文件路径。用于查找文件名匹配特定模式的文件。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "glob模式（如 **/*.rs 或 src/**/*.ts）"
                },
                "path": {
                    "type": "string",
                    "description": "搜索起始路径，默认为工作区根目录"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        let pattern_str = args["pattern"].as_str().unwrap_or("");
        if pattern_str.is_empty() {
            return ToolResult::error("pattern不能为空");
        }

        let search_root = if let Some(p) = args["path"].as_str() {
            if Path::new(p).is_absolute() {
                Path::new(p).to_path_buf()
            } else {
                Path::new(&self.workspace_root).join(p)
            }
        } else {
            Path::new(&self.workspace_root).to_path_buf()
        };

        // Build full glob pattern
        let full_pattern = format!(
            "{}/{}",
            search_root.display(),
            pattern_str.trim_start_matches('/')
        );

        let mut results = Vec::new();
        let mut count = 0;

        let _pattern = match glob::Pattern::new(&full_pattern) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("无效的glob模式: {}", e)),
        };

        // Canonical containment root: every returned candidate must resolve to
        // a real path within this root (symlink/junction escape → skip).
        let canonical_root = std::fs::canonicalize(&search_root).unwrap_or_else(|_| search_root.clone());

        let glob_iter = match glob::glob(&full_pattern) {
            Ok(paths) => paths,
            Err(_) => return ToolResult::success("未找到匹配项"),
        };
        for entry in glob_iter {
            if let Ok(path) = entry {
                // Skip common ignore directories
                let path_str = path.display().to_string();
                if path_str.contains("node_modules")
                    || path_str.contains("/.git/")
                    || path_str.contains("/target/")
                    || path_str.contains("/dist/")
                {
                    continue;
                }
                // Containment: resolve the real path and require it to be within
                // the canonical search root.
                if let Ok(real) = std::fs::canonicalize(&path) {
                    if !crate::safety::is_within_root(&canonical_root, &real) {
                        continue;
                    }
                }
                results.push(path_str);
                count += 1;

                if count >= 200 {
                    results.push("... (结果过多，仅显示前200个)".to_string());
                    break;
                }
            }
        }

        if results.is_empty() {
            ToolResult::success("未找到匹配项")
        } else {
            results.sort();
            ToolResult::success(results.join("\n"))
        }
    }
}
