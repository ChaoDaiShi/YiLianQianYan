// ============================================================
// Frontend type definitions — mirrors backend Rust types
// ============================================================

export interface ToolCallRecord {
  toolCallId: string;
  name: string;
  args: Record<string, unknown>;
  status: "running" | "success" | "error";
  result?: string;
}

export interface Message {
  id: string;
  role: string;
  content: string;
  tool_calls?: ToolCallRecord[];
  tool_call_id?: string;
  tool_name?: string;
  created_at: number;
}

export interface ConversationSummary {
  id: string;
  title: string;
  created_at: number;
  updated_at: number;
}

// ============================================================
// Configuration types
// ============================================================

export interface AgentConfig {
  name: string;
  system_prompt: string;
  workspace_root: string;
}

export interface ModelConfig {
  provider: string;
  name: string;
  base_url: string;
  api_key: string;
  api_key_env: string;
  temperature: number;
  max_tokens: number;
  invoke_timeout_ms: number;
}

export interface PermissionsConfig {
  mode: string;
  interrupt_on: string[];
}

export interface SandboxConfig {
  profile: string;
  writable_paths: string[];
  denied_write_paths: string[];
}

export interface SkillsConfig {
  directories: string[];
  progressive_loading: boolean;
}

export interface SubagentsConfig {
  directories: string[];
}

export interface CompactionConfig {
  enabled: boolean;
  context_window: number;
  trigger_threshold: number;
  keep_recent_tokens: number;
}

export interface AppConfig {
  agent: AgentConfig;
  model: ModelConfig;
  permissions: PermissionsConfig;
  sandbox: SandboxConfig;
  skills: SkillsConfig;
  subagents: SubagentsConfig;
  compaction: CompactionConfig;
}
