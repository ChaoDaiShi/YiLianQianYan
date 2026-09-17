// ============================================================
// Frontend type definitions — mirrors backend Rust types
// ============================================================

export interface ToolCallRecord {
  toolCallId: string;
  name: string;
  args: Record<string, unknown>;
  status: "running" | "success" | "error" | "blocked";
  result?: string;
  riskLevel?: "low" | "medium" | "high" | "critical" | "unknown";
  approvalStatus?:
    | "not_required"
    | "pending"
    | "approved"
    | "rejected"
    | "cancelled"
    | "expired";
  verificationStatus?: "not_requested" | "pending" | "passed" | "failed";
  verificationReason?: string;
  startedAt?: number;
  finishedAt?: number;
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
  run_status?: ConversationRunStatus;
  run_error?: string | null;
  run_started_at?: number | null;
  run_finished_at?: number | null;
  created_at: number;
  updated_at: number;
}

export type ConversationRunStatus =
  | "idle"
  | "running"
  | "waiting_approval"
  | "completed"
  | "failed"
  | "cancelled"
  | "interrupted";

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
  api_key_configured?: boolean;
  api_key_source?: string;
  api_key_env: string;
  clear_api_key?: boolean;
  temperature: number;
  max_tokens: number;
  invoke_timeout_ms: number;
  embedding_model: string;
  embedding_base_url: string;
  embedding_api_key: string;
  embedding_api_key_configured?: boolean;
  embedding_api_key_source?: string;
  embedding_api_key_env: string;
  clear_embedding_api_key?: boolean;
}

export interface VoiceSttConfig {
  provider: string;
  base_url: string;
  model: string;
  language: string;
  api_key: string;
  api_key_configured?: boolean;
  api_key_source?: string;
  api_key_env: string;
  clear_api_key?: boolean;
  timeout_ms: number;
}

export interface VoiceTtsConfig extends VoiceSttConfig {
  voice: string;
}

export interface VoiceConfig {
  stt: VoiceSttConfig;
  tts: VoiceTtsConfig;
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
  voice: VoiceConfig;
  permissions: PermissionsConfig;
  sandbox: SandboxConfig;
  skills: SkillsConfig;
  subagents: SubagentsConfig;
  compaction: CompactionConfig;
  secret_store_status?: string;
  migration_pending?: number;
}

export interface LlmModel {
  id: string;
  provider: string;
  label: string;
  model: string;
  base_url: string;
  api_format: string;
  api_key_configured: boolean;
  api_key_source: string;
  api_key_env: string;
  temperature: number;
  max_tokens: number;
  invoke_timeout_ms: number;
  active: boolean;
  verified_at?: number | null;
  last_error?: string | null;
  created_at: number;
  updated_at: number;
}

export interface LlmModelPayload {
  provider: string;
  label: string;
  model: string;
  base_url: string;
  api_format: "openai";
  api_key?: string;
  api_key_env?: string;
  clear_api_key?: boolean;
  temperature: number;
  max_tokens: number;
  invoke_timeout_ms: number;
}

export interface LlmUsageDay {
  date: string;
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
}

export interface LlmUsageReport {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  days: LlmUsageDay[];
}
