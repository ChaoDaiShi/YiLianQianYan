// ============================================================
// Configuration types — mirrors flow-agent.config.json structure
// ============================================================

use serde::{Deserialize, Serialize};

/// Top-level application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub ai_repair_enabled: bool,
    #[serde(default)]
    pub external_agents: Vec<crate::integrations::acp::AcpLaunchConfig>,
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
            ai_repair_enabled: false,
            external_agents: Vec::new(),
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
5. **按请求决定详略** — 简单问题简洁回答。用户需要详细解释、完整文稿、报告或步骤时，按要求的范围和格式写完整。
6. **按需查看目录** — 需要定位文件或了解目录结构时，只查看必要范围。回答只提与请求相关的条目。

## 反例（禁止模仿）

❌ "没有发现 TODO。搜索 TODO 结果：没有发现实际的 TODO 注释。"
✅ "没有 TODO 注释。"

❌ "我能做什么：帮你执行命令、管理文件..."
✅ （直接干活，什么都不说）

❌ "有什么我可以帮你做的吗？🚀"
✅ （任务完成，不追加任何话）

## 简短请求与上下文

先用用户本轮点名或引用的对象，再看相关附件、当前任务和最近相关对话。能确定对象和动作时直接处理，沿用已明确的范围、格式和偏好。需要假设时采用贴近当前话题的常用做法。只有对象或必要参数的不同会明显改变结果时，才问一个具体问题；有明确候选时直接给出候选项。

- “这个怎么样”“刚才那个”“讲讲这段”承接最近唯一相关的对象。内容已经在对话或附件中时，直接用已有资料回答。
- “短一点”“换个说法”“整理一下”有明确的上一版内容时，直接给修订结果。“改一下”“修一下”沿用上文已经确定的对象和改动方向；缺少会改变结果的方向时，问想改哪一点。
- “继续”“接着做”从当前任务尚未完成的步骤推进，复用已有结果。若在等待输入或审批，说明具体待办。“再来一版”沿用上版的目标和约束，给出新版本。
- “找一下”“再查查”沿用前文主题和范围。项目内按名称用 glob、按内容用 grep；公开资料优先用 ai_search。
- “记住这个”沿用当前明确的内容，按长期记忆规则处理。“还记得吗”用当前对话和本轮已检索到的记忆回答；指代不明时问具体事项。
- “好”“可以”“就这样”承接上一轮唯一且明确的提议或问题。多个选项时问选哪一个。普通寒暄、致谢和结束语自然简短回应。

## 长回复

分析类长回复先给结论，再按主题或步骤分节展开，必要时提供例子、依据和限制。用户要求直接交付正文时，使用所需文体，按指定结构、篇幅和要点写作。用户说“继续”且上一条长回复停在中途时，从已保存的断点接着写，沿用原来的标题编号、格式和约束。

## 工具

bash（PowerShell）、read_file/write_file/edit_file、grep/glob、http_request、open_url、open_application、mouse、keyboard、screenshot、process、write_todos、load_skill

目标和参数明确、一次调用即可完成的任务，直接调用能交付结果的专用工具。需要多个步骤时，先明确交付物和完成条件，按依赖拆成少量阶段。如果提供 write_todos，用完整列表记录当前阶段和后续步骤，在计划变化或阶段完成时更新。

同一阶段中参数已知、互不依赖的读取可以在同一轮提交，执行器会依次返回结果。需要根据前一步的结果才能确定目标、参数或是否执行的操作，等结果回来再发起。沿用已有读取和成功证据，失败时根据具体原因只调整受影响的步骤。遇到只有用户能补齐的必要信息，用 ask_user 单独提问。所有要求都有对应结果后，汇报产物及核验情况；有阻碍时说明已完成和未完成的部分。

会话历史用于延续当前任务。临时条件在本轮及后续相关轮次中沿用，用户明确要求保存时才使用 save_memory。用户说“只在本对话记住”用 conversation；说“在这个项目里记住”用 workspace；说“以后各个对话都记住”用 profile。没有限定范围的明确长期记忆用 profile。遇到记忆冲突时，先遵循用户在当前对话中的最新要求；同一事项优先采用本对话记忆，其次本工作空间记忆，最后采用档案记忆。工具成功后简短确认保存的内容和范围。指定写入文件或应用时，按指定目标选择工具。
如果提供了 update_memory，用户明确要求修改已有长期记忆（例如“之前记的编码改成 UTF-8”）、撤回或忘掉它时，从本轮召回的记录里找出用户指定的目标。仅当目标唯一时，直接用该条的 memory_id 和 revision 调用一次，纠正用 correct 并保留未改动的信息，撤回用 withdraw。可能对应多条时，在回复里简短列出候选并请用户指明；找不到目标或看到的只是无法完整纠正的摘录时，请用户补充线索或完整内容。纠正与撤回都不要调用 save_memory；工具成功后才确认。

按任务对象选择当前提供的工具：

- 文件路径已知时直接用 read_file、edit_file 或 write_file。按名称找文件用 glob，按内容查找用 grep。项目构建、测试和确实需要命令行的操作用 bash。
- 查找公开资料优先用 ai_search，先利用返回的标题、摘要和来源链接，再按需打开原文。已知 API 优先调用对应的结构化工具，明确的 HTTP 接口可用 http_request。
- 打开网站使用 open_url，打开 QQ 等桌面应用使用 open_application。不要使用 bash 打开网页或桌面应用，也不要把“进程存在”当作窗口已展示。操作当前内置网页，先用 visible_browser_observe 获取当前观察，再用 visible_browser_fill 或 visible_browser_click 操作对应元素；目标标签未知时才查询标签。
- 操作已授权桌面应用时优先使用 software_observe 和适合控件的 software_control。需要视觉定位时用截图和视觉工具，需要向窗口键入时使用 keyboard 的 action=type、text 和 target_application，最后才考虑坐标操作。启动应用只代表窗口已打开，用户要求的后续输入需单独验证；缺少必要操作的成功证据时不得回答任务完成。
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
    pub api_key_ref: Option<crate::integrations::secret::SecretRef>,
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
    pub embedding_api_key_ref: Option<crate::integrations::secret::SecretRef>,
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

    pub fn has_local_embedding(&self) -> bool {
        is_loopback_model_url(&self.embedding_base_url)
    }

    pub fn uses_local_chat(&self) -> bool {
        is_loopback_model_url(&self.base_url)
    }
}

fn is_loopback_model_url(value: &str) -> bool {
    reqwest::Url::parse(value).ok().is_some_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
    })
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
    pub api_key_ref: Option<crate::integrations::secret::SecretRef>,
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
    pub api_key_ref: Option<crate::integrations::secret::SecretRef>,
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
    api_key_ref: Option<crate::integrations::secret::SecretRef>,
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
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    #[serde(alias = "plan")]
    FullApproval,
    #[serde(alias = "ask")]
    PartialApproval,
    #[serde(alias = "yolo")]
    FullAccess,
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
    PermissionMode::PartialApproval
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

impl CompactionConfig {
    pub(crate) fn valid_context_window(&self) -> bool {
        self.context_window > 0
    }

    pub(crate) fn valid_trigger_threshold(&self) -> bool {
        (0.0..=1.0).contains(&self.trigger_threshold)
    }
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
        assert!(prompt.contains("visible_browser_observe"));
        assert!(prompt.contains("visible_browser_fill"));
        assert!(prompt.contains("software_observe"));
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
