use super::{DiscoveredSkill, SkillCompatibility};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};

const MAX_SKILL_DEPTH: usize = 4;
#[derive(Debug, Clone, Serialize)]
pub struct SkillSource {
    pub id: String,
    pub label: String,
    pub path: PathBuf,
    pub kind: String,
    pub exists: bool,
    pub skill_count: usize,
    pub error: Option<String>,
}
pub struct SkillDiscovery {
    skills: BTreeMap<String, DiscoveredSkill>,
    sources: Vec<SkillSource>,
    available_tools: HashSet<String>,
}
impl SkillDiscovery {
    pub fn discover(directories: &[String], workspace_root: &str) -> Self {
        let home = dirs::home_dir();
        let codex_home = std::env::var_os("CODEX_HOME").map(PathBuf::from);
        let bundled = std::env::var_os("YILIAN_BUNDLED_SKILLS").map(PathBuf::from);
        Self::discover_with_bundled_roots(
            directories,
            Path::new(workspace_root),
            home.as_deref(),
            codex_home.as_deref(),
            bundled.as_deref(),
        )
    }
    /// Explicit environment roots keep fixtures independent of installed user skills.
    pub(super) fn discover_with_roots(
        directories: &[String],
        workspace: &Path,
        home: Option<&Path>,
        codex_home: Option<&Path>,
    ) -> Self {
        Self::discover_with_bundled_roots(directories, workspace, home, codex_home, None)
    }
    fn discover_with_bundled_roots(
        directories: &[String],
        workspace: &Path,
        home: Option<&Path>,
        codex_home: Option<&Path>,
        bundled: Option<&Path>,
    ) -> Self {
        let mut sources = Vec::new();
        if let Some(path) = bundled {
            add_source(
                &mut sources,
                "bundled".into(),
                "忆涟千言内置".into(),
                path.to_path_buf(),
                "bundled",
            );
        }
        for (index, directory) in directories.iter().enumerate() {
            let path = PathBuf::from(directory);
            add_source(
                &mut sources,
                format!("configured-{}", index + 1),
                "自定义目录".into(),
                if path.is_absolute() {
                    path
                } else {
                    workspace.join(path)
                },
                "configured",
            );
        }
        let roots = [
            ("agents", "通用 Agent"),
            ("codex", "Codex"),
            ("claude", "Claude Code"),
            ("github", "GitHub / VS Code"),
            ("copilot", "GitHub Copilot"),
        ];
        for (key, label) in roots {
            add_source(
                &mut sources,
                format!("workspace-{key}"),
                format!("工作空间 {label}"),
                workspace.join(format!(".{key}/skills")),
                "workspace",
            );
        }
        if let Some(home) = home {
            for (key, label) in roots {
                let path = if key == "codex" {
                    codex_home
                        .map(PathBuf::from)
                        .unwrap_or_else(|| home.join(".codex"))
                        .join("skills")
                } else {
                    home.join(format!(".{key}/skills"))
                };
                add_source(
                    &mut sources,
                    format!("user-{key}"),
                    format!("用户 {label}"),
                    path,
                    "user",
                );
            }
            let codex = codex_home
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".codex"));
            plugin_sources(
                &mut sources,
                &codex.join("plugins/cache"),
                "codex",
                "Codex 插件",
            );
            plugin_sources(
                &mut sources,
                &home.join(".claude/plugins/cache"),
                "claude",
                "Claude Code 插件",
            );
        }
        let mut discovery = Self {
            skills: BTreeMap::new(),
            sources: Vec::new(),
            available_tools: crate::tools::ToolRegistry::with_defaults(
                workspace.to_string_lossy().as_ref(),
            )
            .list_tools()
            .into_iter()
            .map(|tool| tool.name)
            .collect(),
        };
        let mut seen_files = HashSet::new();
        for mut source in sources {
            source.exists = source.path.is_dir();
            if source.exists && source.error.is_none() {
                let mut visited = HashSet::new();
                if source.kind == "bundled" {
                    for name in ["code-reviewer", "doc-writer", "python-expert"] {
                        let directory = source.path.join(name);
                        if directory.join("SKILL.md").is_file() {
                            if let Err(error) = discovery.scan(
                                &directory,
                                1,
                                &source,
                                &mut visited,
                                &mut seen_files,
                            ) {
                                source.error = Some(error);
                                break;
                            }
                        }
                    }
                } else if let Err(error) =
                    discovery.scan(&source.path, 0, &source, &mut visited, &mut seen_files)
                {
                    source.error = Some(error);
                }
            }
            source.skill_count = discovery
                .skills
                .values()
                .filter(|skill| skill.source_id == source.id)
                .count();
            discovery.sources.push(source);
        }
        tracing::info!(
            count = discovery.skills.len(),
            sources = discovery.sources.len(),
            "Discovered local skills"
        );
        discovery
    }
    fn scan(
        &mut self,
        dir: &Path,
        depth: usize,
        source: &SkillSource,
        visited: &mut HashSet<PathBuf>,
        seen_files: &mut HashSet<PathBuf>,
    ) -> Result<(), String> {
        if depth > MAX_SKILL_DEPTH {
            return Ok(());
        }
        let canonical = dir
            .canonicalize()
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        if !visited.insert(canonical) {
            return Ok(());
        }
        let skill_md = dir.join("SKILL.md");
        if skill_md.is_file() {
            let canonical = skill_md.canonicalize().map_err(|e| e.to_string())?;
            if !seen_files.insert(canonical) {
                return Ok(());
            }
            let content = std::fs::read_to_string(&skill_md)
                .map_err(|e| format!("{}: {e}", skill_md.display()))?;
            let base = dir
                .file_name()
                .ok_or("技能目录缺少名称")?
                .to_string_lossy()
                .to_string();
            let display_name = frontmatter(&content, "name").unwrap_or_else(|| base.clone());
            let description = frontmatter(&content, "description")
                .or_else(|| {
                    content
                        .lines()
                        .find_map(|line| line.trim().strip_prefix("# ").map(str::to_string))
                })
                .unwrap_or_else(|| "无描述".into());
            let mut name = base.clone();
            if self.skills.contains_key(&name) {
                name = format!("{}:{base}", source.id);
            }
            let mut number = 2;
            while self.skills.contains_key(&name) {
                name = format!("{}:{base}-{number}", source.id);
                number += 1;
            }
            self.skills.insert(
                name.clone(),
                DiscoveredSkill {
                    name,
                    description,
                    path: skill_md,
                    root_dir: dir.to_path_buf(),
                    display_name,
                    source_id: source.id.clone(),
                    source: source.label.clone(),
                    source_kind: source.kind.clone(),
                    source_root: source.path.clone(),
                    compatibility: SkillCompatibility::inspect(&content, &self.available_tools),
                    loaded_content: None,
                },
            );
            // References/scripts belong to this skill and are never catalogued as skills.
            return Ok(());
        }
        for path in children(dir)? {
            if path.is_dir() {
                self.scan(&path, depth + 1, source, visited, seen_files)?;
            }
        }
        Ok(())
    }
    pub fn all(&self) -> Vec<&DiscoveredSkill> {
        self.skills.values().collect()
    }
    pub fn get(&self, name: &str) -> Option<&DiscoveredSkill> {
        self.skills.get(name)
    }
    pub fn get_mut(&mut self, name: &str) -> Option<&mut DiscoveredSkill> {
        self.skills.get_mut(name)
    }
    pub fn sources(&self) -> &[SkillSource] {
        &self.sources
    }
    pub fn available_tools(&self) -> &HashSet<String> {
        &self.available_tools
    }
    pub fn set_available_tools(&mut self, tools: HashSet<String>) {
        self.available_tools = tools;
        for skill in self.skills.values_mut() {
            skill.compatibility = SkillCompatibility::resolve(
                skill.compatibility.required_tools.clone(),
                &self.available_tools,
            );
        }
    }
    pub fn has_skills(&self) -> bool {
        self.skills
            .values()
            .any(|skill| skill.compatibility.runtime_ready)
    }
    pub fn render_prompt_section(&self) -> String {
        if self.skills.is_empty() {
            return String::new();
        }
        let mut section = String::from(
            "## Available Skills\n\n你可以使用 `load_skill` 工具按需加载以下技能的完整内容：\n\n",
        );
        for skill in self
            .skills
            .values()
            .filter(|skill| skill.compatibility.runtime_ready)
        {
            section.push_str(&format!(
                "- **{}**: {} (来源: {})\n",
                skill.name, skill.description, skill.source
            ));
        }
        section.push_str("\n使用 `load_skill(name=\"<名称>\")` 加载技能的完整指引。加载结果包含实际技能根目录，供解析 scripts/ 和 references/。");
        section
    }
}
fn add_source(
    sources: &mut Vec<SkillSource>,
    id: String,
    label: String,
    path: PathBuf,
    kind: &str,
) {
    sources.push(SkillSource {
        id,
        label,
        path,
        kind: kind.into(),
        exists: false,
        skill_count: 0,
        error: None,
    });
}
fn children(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut entries = std::fs::read_dir(root)
        .map_err(|e| format!("{}: {e}", root.display()))?
        .map(|entry| entry.map(|entry| entry.path()).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();
    Ok(entries)
}
fn plugin_sources(sources: &mut Vec<SkillSource>, cache: &Path, key: &str, label: &str) {
    if !cache.is_dir() {
        return;
    }
    // Only <cache>/<marketplace>/<package>/<version>/skills. No walk through
    // arbitrary home folders, package source files, or plugin dependencies.
    let result = (|| -> Result<(), String> {
        for market in children(cache)?.into_iter().filter(|path| path.is_dir()) {
            for package in children(&market)?.into_iter().filter(|path| path.is_dir()) {
                for version in children(&package)?.into_iter().filter(|path| path.is_dir()) {
                    let skills = version.join("skills");
                    if !skills.is_dir() {
                        continue;
                    }
                    let package_name = package
                        .file_name()
                        .ok_or("插件目录缺少名称")?
                        .to_string_lossy();
                    let market_name = market
                        .file_name()
                        .ok_or("市场目录缺少名称")?
                        .to_string_lossy();
                    let version_name = version
                        .file_name()
                        .ok_or("版本目录缺少名称")?
                        .to_string_lossy();
                    let id = format!("{key}-plugin-{market_name}-{package_name}-{version_name}");
                    add_source(
                        sources,
                        id,
                        format!("{label} · {package_name} {version_name}"),
                        skills,
                        "plugin",
                    );
                }
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        sources.push(SkillSource {
            id: format!("{key}-plugin-cache"),
            label: format!("{label}缓存"),
            path: cache.to_path_buf(),
            kind: "plugin".into(),
            exists: true,
            skill_count: 0,
            error: Some(error),
        });
    }
}
fn frontmatter(content: &str, key: &str) -> Option<String> {
    let mut lines = content.trim_start_matches('\u{feff}').lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let lines = lines
        .take_while(|line| line.trim() != "---")
        .collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        let Some(value) = line.strip_prefix(&format!("{key}:")) else {
            continue;
        };
        let value = value.trim();
        if matches!(value, "|" | ">" | "|-" | ">-") {
            let value = lines
                .iter()
                .skip(index + 1)
                .take_while(|line| line.starts_with([' ', '\t']) || line.is_empty())
                .map(|line| line.trim())
                .collect::<Vec<_>>()
                .join(" ");
            return (!value.is_empty()).then_some(value);
        }
        let value = if value.starts_with('"') {
            serde_json::from_str::<String>(value).ok()?
        } else {
            value.trim_matches('\'').replace("''", "'")
        };
        return (!value.is_empty()).then_some(value);
    }
    None
}
