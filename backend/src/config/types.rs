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
    pub voice: VoiceConfig,
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
            voice: VoiceConfig::default(),
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

bash（PowerShell）、read_file/write_file/edit_file、grep/glob、http_request、open_url、open_application、mouse、keyboard、screenshot、process、write_todos、load_skill

打开网站必须使用 open_url；打开 QQ 等桌面 GUI 应用必须使用 open_application。不要使用 bash 打开网页或桌面应用，也不要把“进程存在”当作窗口已展示。

桌面多步骤任务必须逐项执行。启动应用只代表窗口已打开，不代表后续输入已经完成；需要输入文本时必须继续调用 keyboard，使用 action=type、text 和 target_application 指定目标应用。只有所有用户要求的可观察操作都有成功工具结果后才能回答完成；缺少任何一步时不得回答任务完成。
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
    /// Direct API key. Write-only input (LEGACY MIGRATION ONLY); never persisted.
    #[serde(default, skip_serializing)]
    pub api_key: String,
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,
    /// Stable SecretRef for the chat API key (persisted; value lives in the
    /// OS-backed SecretStore).
    #[serde(default)]
    pub api_key_ref: Option<crate::secret::SecretRef>,
    /// Write-only request signal to clear the stored chat API key.
    #[serde(default, skip_serializing)]
    pub clear_api_key: bool,
    #[serde(default)]
    pub temperature: f64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_timeout_ms")]
    pub invoke_timeout_ms: u64,
    // ── Embedding model (independent from chat) ──
    /// Embedding model name (empty = not configured).
    #[serde(default)]
    pub embedding_model: String,
    /// Base URL for the embeddings endpoint.
    #[serde(default)]
    pub embedding_base_url: String,
    /// Direct embedding API key. Write-only input (LEGACY MIGRATION ONLY).
    #[serde(default, skip_serializing)]
    pub embedding_api_key: String,
    /// Environment variable that holds the embedding API key.
    #[serde(default)]
    pub embedding_api_key_env: String,
    /// Stable SecretRef for the embedding API key (persisted).
    #[serde(default)]
    pub embedding_api_key_ref: Option<crate::secret::SecretRef>,
    /// Write-only request signal to clear the stored embedding API key.
    #[serde(default, skip_serializing)]
    pub clear_embedding_api_key: bool,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            name: default_model_name(),
            base_url: default_base_url(),
            api_key: String::new(),
            api_key_env: default_api_key_env(),
            api_key_ref: None,
            clear_api_key: false,
            temperature: 0.0,
            max_tokens: default_max_tokens(),
            invoke_timeout_ms: default_timeout_ms(),
            embedding_model: String::new(),
            embedding_base_url: String::new(),
            embedding_api_key: String::new(),
            embedding_api_key_env: String::new(),
            embedding_api_key_ref: None,
            clear_embedding_api_key: false,
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

    /// Resolve the embedding API key (independent from chat API key).
    pub fn resolve_embedding_api_key(&self) -> Option<String> {
        if !self.embedding_api_key.is_empty() {
            return Some(self.embedding_api_key.clone());
        }
        if !self.embedding_api_key_env.is_empty() {
            return std::env::var(&self.embedding_api_key_env).ok();
        }
        None
    }

    /// Whether an embedding model is configured.
    pub fn has_embedding(&self) -> bool {
        !self.embedding_model.is_empty() && !self.embedding_base_url.is_empty()
    }
}

// ── Voice providers ──

/// Persisted, provider-neutral voice settings. Credentials are write-only and
/// are stored through SecretStore; only the stable SecretRef is serialized.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VoiceConfig {
    pub stt: VoiceSttConfig,
    pub tts: VoiceTtsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceSttConfig {
    #[serde(default = "default_voice_provider")]
    pub provider: String,
    #[serde(default = "default_voice_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_voice_language")]
    pub language: String,
    #[serde(default, skip_serializing)]
    pub api_key: String,
    #[serde(default = "default_voice_api_key_env")]
    pub api_key_env: String,
    #[serde(default)]
    pub api_key_ref: Option<crate::secret::SecretRef>,
    #[serde(default)]
    pub clear_api_key: bool,
    #[serde(default = "default_voice_timeout_ms")]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceTtsConfig {
    #[serde(default = "default_tts_provider")]
    pub provider: String,
    #[serde(default = "default_tts_base_url")]
    pub base_url: String,
    #[serde(default = "default_tts_model")]
    pub model: String,
    #[serde(default = "default_tts_voice")]
    pub voice: String,
    #[serde(default = "default_voice_language")]
    pub language: String,
    #[serde(default, skip_serializing)]
    pub api_key: String,
    #[serde(default)]
    pub api_key_env: String,
    #[serde(default)]
    pub api_key_ref: Option<crate::secret::SecretRef>,
    #[serde(default)]
    pub clear_api_key: bool,
    #[serde(default = "default_voice_timeout_ms")]
    pub timeout_ms: u64,
}

impl Default for VoiceSttConfig {
    fn default() -> Self {
        Self {
            provider: default_voice_provider(),
            base_url: default_voice_base_url(),
            model: String::new(),
            language: default_voice_language(),
            api_key: String::new(),
            api_key_env: default_voice_api_key_env(),
            api_key_ref: None,
            clear_api_key: false,
            timeout_ms: default_voice_timeout_ms(),
        }
    }
}

impl Default for VoiceTtsConfig {
    fn default() -> Self {
        Self {
            provider: default_tts_provider(),
            base_url: default_tts_base_url(),
            model: default_tts_model(),
            voice: default_tts_voice(),
            language: default_voice_language(),
            api_key: String::new(),
            api_key_env: String::new(),
            api_key_ref: None,
            clear_api_key: false,
            timeout_ms: default_voice_timeout_ms(),
        }
    }
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            stt: VoiceSttConfig::default(),
            tts: VoiceTtsConfig::default(),
        }
    }
}

impl VoiceSttConfig {
    pub fn structurally_configured(&self) -> bool {
        !self.provider.trim().is_empty()
            && !self.base_url.trim().is_empty()
            && !self.model.trim().is_empty()
            && self.timeout_ms > 0
    }
}

impl VoiceTtsConfig {
    pub fn structurally_configured(&self) -> bool {
        !self.provider.trim().is_empty()
            && !self.base_url.trim().is_empty()
            && !self.model.trim().is_empty()
            && !self.voice.trim().is_empty()
            && self.timeout_ms > 0
    }
}

#[derive(Default, Deserialize)]
struct VoiceConfigWire {
    #[serde(default)]
    stt: Option<VoiceSttConfig>,
    #[serde(default)]
    tts: Option<VoiceTtsConfig>,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    stt_model: String,
    #[serde(default)]
    tts_model: String,
    #[serde(default)]
    voice: String,
    #[serde(default)]
    language: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    api_key_env: String,
    #[serde(default)]
    api_key_ref: Option<crate::secret::SecretRef>,
    #[serde(default)]
    clear_api_key: bool,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

impl<'de> Deserialize<'de> for VoiceConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = VoiceConfigWire::deserialize(deserializer)?;
        if wire.stt.is_some() || wire.tts.is_some() {
            return Ok(Self {
                stt: wire.stt.unwrap_or_default(),
                tts: wire.tts.unwrap_or_default(),
            });
        }

        let provider = if wire.provider.trim().is_empty() {
            default_voice_provider()
        } else {
            wire.provider
        };
        let base_url = if wire.base_url.trim().is_empty() {
            default_voice_base_url()
        } else {
            wire.base_url
        };
        let language = if wire.language.trim().is_empty() {
            default_voice_language()
        } else {
            wire.language
        };
        let timeout_ms = wire.timeout_ms.unwrap_or_else(default_voice_timeout_ms);
        let credential = (
            wire.api_key,
            if wire.api_key_env.trim().is_empty() {
                default_voice_api_key_env()
            } else {
                wire.api_key_env
            },
            wire.api_key_ref,
            wire.clear_api_key,
        );
        let stt = VoiceSttConfig {
            provider: provider.clone(),
            base_url: base_url.clone(),
            model: wire.stt_model,
            language: language.clone(),
            api_key: credential.0.clone(),
            api_key_env: credential.1.clone(),
            api_key_ref: credential.2.clone(),
            clear_api_key: credential.3,
            timeout_ms,
        };
        let tts = VoiceTtsConfig {
            provider,
            base_url,
            model: wire.tts_model,
            voice: if wire.voice.trim().is_empty() {
                "alloy".to_string()
            } else {
                wire.voice
            },
            language,
            api_key: credential.0,
            api_key_env: credential.1,
            api_key_ref: credential.2,
            clear_api_key: credential.3,
            timeout_ms,
        };
        Ok(Self { stt, tts })
    }
}

fn default_voice_provider() -> String {
    "openai-compatible".to_string()
}

fn default_voice_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_tts_provider() -> String {
    "minimax".to_string()
}

fn default_tts_base_url() -> String {
    "https://api.minimax.io".to_string()
}

fn default_tts_model() -> String {
    "speech-2.8-turbo".to_string()
}

fn default_tts_voice() -> String {
    "female-shaonv".to_string()
}

fn default_voice_language() -> String {
    "zh".to_string()
}

fn default_voice_api_key_env() -> String {
    "OPENAI_API_KEY".to_string()
}

fn default_voice_timeout_ms() -> u64 {
    60_000
}

fn default_provider() -> String {
    "openai".to_string()
}
fn default_model_name() -> String {
    "deepseek-v4-flash".to_string()
}
fn default_base_url() -> String {
    "https://api.deepseek.com/v1".to_string()
}
fn default_api_key_env() -> String {
    "OPENAI_API_KEY".to_string()
}
fn default_max_tokens() -> u32 {
    16384
}
fn default_timeout_ms() -> u64 {
    120000
}

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

fn default_permission_mode() -> PermissionMode {
    PermissionMode::Ask
}
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

fn default_profile() -> SandboxProfile {
    SandboxProfile::WorkspaceWrite
}

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

fn default_true() -> bool {
    true
}
fn default_context_window() -> usize {
    200000
}
fn default_trigger_threshold() -> f64 {
    0.8
}
fn default_keep_recent() -> usize {
    20000
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_prompt_routes_gui_requests_to_structured_tools() {
        let prompt = default_system_prompt();

        assert!(prompt.contains("open_url"));
        assert!(prompt.contains("open_application"));
        assert!(prompt.contains("keyboard"));
        assert!(prompt.contains("target_application"));
        assert!(prompt.contains("不要使用 bash 打开网页或桌面应用"));
        assert!(prompt.contains("启动应用只代表窗口已打开"));
        assert!(prompt.contains("不得回答任务完成"));
    }

    #[test]
    fn model_config_deserializes_without_embedding_fields() {
        let json = r#"{
            "provider": "openai",
            "name": "deepseek-v4-flash",
            "base_url": "https://api.deepseek.com/v1",
            "temperature": 0.0,
            "max_tokens": 16384,
            "invoke_timeout_ms": 120000
        }"#;
        let config: ModelConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.name, "deepseek-v4-flash");
        assert!(config.embedding_model.is_empty());
        assert!(config.embedding_base_url.is_empty());
        assert!(!config.has_embedding());
    }

    #[test]
    fn model_config_with_embedding_fields() {
        let json = r#"{
            "provider": "openai",
            "name": "deepseek-v4-flash",
            "base_url": "https://api.deepseek.com/v1",
            "temperature": 0.0,
            "max_tokens": 16384,
            "invoke_timeout_ms": 120000,
            "embedding_model": "text-embedding-3-small",
            "embedding_base_url": "https://api.openai.com/v1",
            "embedding_api_key": "sk-embed",
            "embedding_api_key_env": "EMBED_KEY"
        }"#;
        let config: ModelConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.embedding_model, "text-embedding-3-small");
        assert_eq!(config.embedding_base_url, "https://api.openai.com/v1");
        assert!(config.has_embedding());
        assert_eq!(
            config.resolve_embedding_api_key(),
            Some("sk-embed".to_string())
        );
    }

    #[test]
    fn embedding_api_key_falls_back_to_env_when_direct_is_empty() {
        let temp_key = "test-embed-env-key-12345";
        std::env::set_var("YILIAN_TEST_EMBED_KEY", temp_key);
        let config = ModelConfig {
            embedding_api_key: String::new(),
            embedding_api_key_env: "YILIAN_TEST_EMBED_KEY".to_string(),
            ..Default::default()
        };
        assert_eq!(
            config.resolve_embedding_api_key(),
            Some(temp_key.to_string())
        );
        std::env::remove_var("YILIAN_TEST_EMBED_KEY");
    }

    #[test]
    fn embedding_api_key_env_wins_over_empty_direct() {
        // No env var set for this one
        let config = ModelConfig {
            embedding_api_key: String::new(),
            embedding_api_key_env: "EMBED_KEY_NOT_SET_XYZ".to_string(),
            ..Default::default()
        };
        assert_eq!(config.resolve_embedding_api_key(), None);
    }

    #[test]
    fn has_embedding_requires_both_model_and_base_url() {
        let config = ModelConfig {
            embedding_model: "text-embedding-3-small".to_string(),
            embedding_base_url: String::new(),
            ..Default::default()
        };
        assert!(!config.has_embedding());
    }
}
