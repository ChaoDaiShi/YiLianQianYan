use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillToolMapping {
    pub from: String,
    pub to: String,
    pub instructions: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCompatibility {
    pub status: String,
    pub runtime_ready: bool,
    pub required_tools: Vec<String>,
    pub missing_tools: Vec<String>,
    pub tool_mappings: Vec<SkillToolMapping>,
    pub reason: String,
}
impl SkillCompatibility {
    pub fn inspect(content: &str, available: &HashSet<String>) -> Self {
        let tokens =
            regex::Regex::new(r"[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*").unwrap();
        let host = [
            "use_figma",
            "get_design_context",
            "create_new_file",
            "generate_diagram",
            "use_sites",
            "create_site",
            "publish_site",
            "get_site",
            "create_source_repository_write_credential",
            "deploy_site_version",
            "deploy_private_site_version",
            "save_site_version",
            "save_version_and_deploy_private",
            "get_deployment_status",
            "open_in_codex",
            "image_gen",
            "view_image",
            "artifact_tool",
            "ToolSearch",
            "tool_search",
            "request_user_input",
            "spawn_agent",
            "send_message",
            "write_stdin",
            "cua_repl",
            "use_browser",
            "browser_run",
            "image_gen.imagegen",
            "web.run",
            "exec_command",
        ];
        let aliases = [
            "Read",
            "Write",
            "Edit",
            "Glob",
            "Grep",
            "Bash",
            "functions.exec_command",
            "tools.exec_command",
            "exec_command",
            "functions.apply_patch",
        ];
        let mut required = HashSet::new();
        for found in tokens.find_iter(content) {
            let token = found.as_str();
            let namespace = ["tools.", "skills.", "notion.", "gdrive.", "sites."]
                .iter()
                .any(|prefix| token.starts_with(prefix));
            let source_file = [".md", ".json", ".py", ".ts", ".js"]
                .iter()
                .any(|extension| token.ends_with(extension));
            if token.starts_with("mcp__")
                || token.starts_with("functions.")
                || (namespace && !source_file)
                || host.contains(&token)
                || available.contains(token)
            {
                required.insert(token.to_string());
            }
        }
        let declared =
            regex::Regex::new(r"(?m)^(?:allowed-tools|required-tools):[ \t]*([^\r\n]*)").unwrap();
        for declaration in declared.captures_iter(content) {
            for tool in declaration_tools(&declaration[1]) {
                required.insert(tool);
            }
            if matches!(declaration[1].trim(), "" | "|" | ">" | "|-" | ">-") {
                let end = declaration.get(0).unwrap().end();
                for line in content[end..].lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    if !line.starts_with([' ', '\t']) && !line.trim_start().starts_with("- ") {
                        break;
                    }
                    for tool in declaration_tools(line) {
                        required.insert(tool);
                    }
                }
            }
        }
        if required.iter().any(|tool| {
            matches!(
                tool.as_str(),
                "functions.exec_command" | "tools.exec_command" | "exec_command"
            )
        }) && regex::Regex::new(r#"(?i)\btty["']?\s*[:=]\s*true"#)
            .unwrap()
            .is_match(content)
        {
            required.insert("interactive_process_session".into());
        }
        // Claude-style capitalized tool names must be named as tools, rather
        // than the ordinary verbs Read/Write inside prose.
        for alias in aliases {
            if content.contains(&format!("`{alias}`"))
                || content.contains(&format!("{alias}("))
                || content.contains(&format!("allowed-tools: {alias}"))
            {
                required.insert(alias.to_string());
            }
        }
        let mut required_tools = required.into_iter().collect::<Vec<_>>();
        required_tools.sort();
        Self::resolve(required_tools, available)
    }
    pub fn resolve(required_tools: Vec<String>, available: &HashSet<String>) -> Self {
        let mut missing_tools = Vec::new();
        let mut tool_mappings = Vec::new();
        for name in &required_tools {
            if available.contains(name) {
                continue;
            }
            if let Some(mapping) = mapping(name) {
                if available.contains(&mapping.to) {
                    tool_mappings.push(mapping);
                    continue;
                }
            }
            missing_tools.push(name.clone());
        }
        let runtime_ready = missing_tools.is_empty();
        let status = if !runtime_ready {
            "external_host"
        } else if tool_mappings.is_empty() {
            "portable"
        } else {
            "adapted"
        }
        .to_string();
        let reason = if !runtime_ready {
            format!("此流程依赖当前工具注册表中不存在的宿主能力：{}。可查看原文；需连接对应工具或将流程适配为本项目真实工具后使用。",missing_tools.join("、"))
        } else if tool_mappings.is_empty() {
            "流程可由本项目当前工具加载；本地脚本、依赖和权限仍按实际执行结果验证。".into()
        } else {
            "已提供明确的本项目工具与参数适配，按适配说明执行原流程。".into()
        };
        Self {
            status,
            runtime_ready,
            required_tools,
            missing_tools,
            tool_mappings,
            reason,
        }
    }
    pub fn execution_notes(&self) -> String {
        let mut notes=String::from("## 忆涟千言执行约定\n当前文件是 Skill 流程说明，不是插件安装包，也不注册新函数。只调用当前工具列表中的真实工具；每次实际执行仍经过项目权限、取消与验证链路。scripts/references 相对路径以文末真实技能根目录解析。\n");
        for mapping in &self.tool_mappings {
            notes.push_str(&format!(
                "- 原文 `{}` 改用 `{}`：{}\n",
                mapping.from, mapping.to, mapping.instructions
            ));
        }
        notes.push_str("原文的工具名称按上述显式适配执行，不要调用未注册的宿主函数。\n");
        notes
    }
}

/// Claude tool declarations permit selectors such as Bash(git:*) and quoted
/// inline arrays. A selector is an argument restriction, never another tool.
fn declaration_tools(value: &str) -> Vec<String> {
    let mut characters = value.chars().peekable();
    let mut tools = Vec::new();
    while let Some(character) = characters.next() {
        if character == '#' {
            break;
        }
        if !character.is_ascii_alphabetic() && character != '_' {
            continue;
        }
        let mut tool = String::from(character);
        while let Some(&next) = characters.peek() {
            if next.is_ascii_alphanumeric() || matches!(next, '_' | '.') {
                tool.push(next);
                characters.next();
            } else {
                break;
            }
        }
        tools.push(tool);
        while characters.peek().is_some_and(|next| next.is_whitespace()) {
            characters.next();
        }
        if characters.peek() != Some(&'(') {
            continue;
        }
        characters.next();
        let mut depth = 1usize;
        let mut quote = None;
        let mut escaped = false;
        for next in characters.by_ref() {
            if escaped {
                escaped = false;
                continue;
            }
            if next == '\\' {
                escaped = true;
                continue;
            }
            if let Some(delimiter) = quote {
                if next == delimiter {
                    quote = None;
                }
            } else {
                match next {
                    '\'' | '"' => quote = Some(next),
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    tools
}
fn mapping(name: &str) -> Option<SkillToolMapping> {
    let (to,instructions)=match name {
        "Read" =>("read_file","传入 {path}；本工具不支持原宿主的 offset/limit 参数。"),
        "Write" =>("write_file","传入 {path,content}。"),
        "Edit" =>("edit_file","传入 {path,find,replace}，只替换第一处；不要沿用 old_string/new_string/all_replace 参数。"),
        "Glob" =>("glob","传入 {pattern,path?}。"),
        "Grep" =>("grep","传入 {pattern,path?}；使用当前工具 schema 支持的参数。"),
        "Bash"|"functions.exec_command"|"tools.exec_command"|"exec_command" =>("bash","单次命令传入 {command,timeout_ms?}；cmd 映射为 command。Windows 实际使用 PowerShell，不支持交互 tty、会话句柄或 write_stdin；不得声称已创建宿主会话。需要工作目录时在命令内明确设置目录并正确引用路径，不传 workdir/yield_time_ms。"),
        _=>return None,
    };
    Some(SkillToolMapping {
        from: name.into(),
        to: to.into(),
        instructions: instructions.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn available() -> HashSet<String> {
        [
            "read_file",
            "write_file",
            "edit_file",
            "glob",
            "grep",
            "bash",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    }
    #[test]
    fn procedural_skills_and_explicit_native_adaptations_are_actionable() {
        let portable = SkillCompatibility::inspect(
            "Read a document and summarize it with `read_file`.",
            &available(),
        );
        assert!(portable.runtime_ready);
        assert_eq!(portable.status, "portable");
        let adapted = SkillCompatibility::inspect(
            "Use `Read` then `functions.exec_command` to run a single build command.",
            &available(),
        );
        assert!(adapted.runtime_ready);
        assert_eq!(adapted.status, "adapted");
        assert!(adapted
            .tool_mappings
            .iter()
            .any(|mapping| mapping.to == "bash"));
        assert!(adapted.execution_notes().contains("timeout_ms"));
    }
    #[test]
    fn host_functions_require_real_registration_and_never_get_an_invented_bridge() {
        let blocked = SkillCompatibility::inspect(
            "Call `use_figma` and `mcp__private__canvas` then `image_gen`.",
            &available(),
        );
        assert!(!blocked.runtime_ready);
        assert_eq!(blocked.status, "external_host");
        assert!(blocked.missing_tools.contains(&"use_figma".into()));
        let mut registered = available();
        registered.insert("use_figma".into());
        assert!(SkillCompatibility::inspect("Call `use_figma`.", &registered).runtime_ready);
        assert!(
            !SkillCompatibility::inspect(
                "Use `functions.exec_command` and `functions.write_stdin`.",
                &available()
            )
            .runtime_ready
        );
    }

    #[test]
    fn scoped_tool_declarations_do_not_turn_command_selectors_into_missing_tools() {
        for declaration in [
            "Bash(git:*) Read",
            "Bash(uv run --python 3.12 script.py) Read",
            "[\"Bash(git:*)\", \"Read\"]",
            "['Bash(uv run python.py)', 'Read']",
            "\"Bash(git:*) Read\"",
            "Bash(git log --format=(%h)) Read",
        ] {
            let compatibility = SkillCompatibility::inspect(
                &format!("---\nallowed-tools: {declaration}\n---\n# Review procedure"),
                &available(),
            );
            assert!(
                compatibility.runtime_ready,
                "{declaration}: {:?}",
                compatibility.missing_tools
            );
            assert_eq!(
                compatibility.required_tools,
                vec!["Bash", "Read"],
                "{declaration}"
            );
            assert!(compatibility.missing_tools.is_empty());
            assert_eq!(compatibility.tool_mappings.len(), 2);
        }
        let missing = SkillCompatibility::inspect(
            "---\nrequired-tools: [\"Bash(git:*)\", \"mcp__canvas__render\"]\n---\n# Render",
            &available(),
        );
        assert_eq!(missing.missing_tools, vec!["mcp__canvas__render"]);
        assert!(!missing.runtime_ready);
        let sequence = SkillCompatibility::inspect(
            "---\nallowed-tools:\n  - \"Bash(git:*)\"\n  - Read\ndescription: A workflow\n---\n# Review",
            &available(),
        );
        assert!(sequence.runtime_ready);
        assert_eq!(sequence.required_tools, vec!["Bash", "Read"]);
    }
}
