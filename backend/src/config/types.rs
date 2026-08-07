// ============================================================
// Configuration types — mirrors flow-agent.config.json structure
// ============================================================

use serde::{Deserialize, Serialize};

/// Top-level application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub model: ModelConfig,
    #[serde(default)]
    pub permissions: PermissionsConfig,
    #[serde(default)]
    pub sandbox: SandboxConfig,
    #[serde(default)]
    pub compaction: CompactionConfig,
    #[serde(default)]
    pub skills: SkillsConfig,
    #[serde(default)]
    pub subagents: SubagentsConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            agent: AgentConfig::default(),
            model: ModelConfig::default(),
            permissions: PermissionsConfig::default(),
            sandbox: SandboxConfig::default(),
            compaction: CompactionConfig::default(),
            skills: SkillsConfig::default(),
            subagents: SubagentsConfig::default(),
        }
    }
}

// ── Agent ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_agent_name")]
    pub name: String,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
    #[serde(default = "default_workspace_root")]
    pub workspace_root: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: default_agent_name(),
            system_prompt: default_system_prompt(),
            workspace_root: default_workspace_root(),
        }
    }
}

fn default_agent_name() -> String {
    "忆涟千言".to_string()
}

fn default_system_prompt() -> String {
    r#"你是 Windows 桌面 AI 助手。直接干活，少说话。环境：PowerShell，项目根目录。

## 铁律

1. **禁止重复** — 同一个结论绝不说两遍。不要换种说法再说一次。
2. **禁止能力介绍** — 绝对不要输出"我能做什么""我的能力包括""我可以帮你"等内容，除非用户原话问"你能干什么"。
3. **禁止模板结尾** — 不要用"有什么我可以帮你的吗""需要我进一步xxx吗""尽管说"等结尾。任务完成就停。
4. **结果在前** — 第一行直接给结论，详情放后面。
5. **一句话原则** — 能用一句话回答的，绝不用一段话。用户没问的背景信息一律省略。
6. **不列目录** — 除非用户原话包含"列出文件""看看目录""有什么文件"等明确指令。

## 反例（禁止模仿）

❌ "没有发现 TODO。搜索 TODO 结果：没有发现实际的 TODO 注释。"
✅ "没有 TODO 注释。"

❌ "我能做什么：帮你执行命令、管理文件..."
✅ （直接干活，什么都不说）

❌ "有什么我可以帮你做的吗？🚀"
✅ （任务完成，不追加任何话）

## 工具

bash（PowerShell）、read_file/write_file/edit_file、grep/glob、http_request、process、write_todos、load_skill
"#.to_string()
}

fn default_workspace_root() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string())
}

// ── Model ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_model_name")]
    pub name: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,
    #[serde(default)]
    pub temperature: f64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_timeout_ms")]
    pub invoke_timeout_ms: u64,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            name: default_model_name(),
            base_url: default_base_url(),
            api_key: String::new(),
            api_key_env: default_api_key_env(),
            temperature: 0.0,
            max_tokens: default_max_tokens(),
            invoke_timeout_ms: default_timeout_ms(),
        }
    }
}

impl ModelConfig {
    /// Resolve API key from direct value or environment variable
    pub fn resolve_api_key(&self) -> Option<String> {
        if !self.api_key.is_empty() {
            return Some(self.api_key.clone());
        }
        if !self.api_key_env.is_empty() {
            return std::env::var(&self.api_key_env).ok();
        }
        None
    }
}

fn default_provider() -> String { "openai".to_string() }
fn default_model_name() -> String { "deepseek-v4-flash".to_string() }
fn default_base_url() -> String { "https://api.deepseek.com/v1".to_string() }
fn default_api_key_env() -> String { "OPENAI_API_KEY".to_string() }
fn default_max_tokens() -> u32 { 16384 }
fn default_timeout_ms() -> u64 { 120000 }

// ── Permissions ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionMode {
    Yolo,
    Ask,
    Plan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionsConfig {
    #[serde(default = "default_permission_mode")]
    pub mode: PermissionMode,
    #[serde(default = "default_interrupt_on")]
    pub interrupt_on: Vec<String>,
}

impl Default for PermissionsConfig {
    fn default() -> Self {
        Self {
            mode: default_permission_mode(),
            interrupt_on: default_interrupt_on(),
        }
    }
}

fn default_permission_mode() -> PermissionMode { PermissionMode::Ask }
fn default_interrupt_on() -> Vec<String> {
    vec![
        "bash".to_string(),
        "write_file".to_string(),
        "edit_file".to_string(),
        "http_request".to_string(),
    ]
}

// ── Sandbox ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxProfile {
    Custom,
    WorkspaceWrite,
    ReadOnly,
    Open,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    #[serde(default = "default_profile")]
    pub profile: SandboxProfile,
    #[serde(default)]
    pub writable_paths: Vec<String>,
    #[serde(default)]
    pub denied_write_paths: Vec<String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            profile: default_profile(),
            writable_paths: vec![],
            denied_write_paths: vec![],
        }
    }
}

impl SandboxConfig {
    /// Get the effective workspace root for sandbox operations
    pub fn workspace_root(&self) -> String {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string())
    }
}

fn default_profile() -> SandboxProfile { SandboxProfile::WorkspaceWrite }

// ── Compaction ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_context_window")]
    pub context_window: usize,
    #[serde(default = "default_trigger_threshold")]
    pub trigger_threshold: f64,
    #[serde(default = "default_keep_recent")]
    pub keep_recent_tokens: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            context_window: default_context_window(),
            trigger_threshold: default_trigger_threshold(),
            keep_recent_tokens: default_keep_recent(),
        }
    }
}

fn default_true() -> bool { true }
fn default_context_window() -> usize { 200000 }
fn default_trigger_threshold() -> f64 { 0.8 }
fn default_keep_recent() -> usize { 20000 }

// ── Skills ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsConfig {
    #[serde(default)]
    pub directories: Vec<String>,
    #[serde(default = "default_true")]
    pub progressive_loading: bool,
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            directories: vec!["./skills".to_string()],
            progressive_loading: true,
        }
    }
}

// ── Subagents ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentsConfig {
    #[serde(default)]
    pub directories: Vec<String>,
}

impl Default for SubagentsConfig {
    fn default() -> Self {
        Self {
            directories: vec!["./.agents/agents".to_string()],
        }
    }
}
