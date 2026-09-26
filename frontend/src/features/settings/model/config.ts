import { Monitor, Moon, Sun } from "lucide-react";
import type { AppConfig } from "../../../types";
import type { SectionKey } from "./types";

/**
 * The baseline config the page renders before the backend answers, and the
 * shape every partial response is merged onto. An unconfigured field must have
 * a defined value here or the form would render `undefined`.
 */
export const defaultConfig: AppConfig = {
  agent: { name: "忆涟千言", system_prompt: "你是一个桌面AI助手...", workspace_root: "" },
  model: {
    provider: "openai",
    name: "deepseek-v4-flash",
    base_url: "https://api.deepseek.com/v1",
    api_key: "",
    api_key_env: "OPENAI_API_KEY",
    temperature: 0,
    max_tokens: 16384,
    invoke_timeout_ms: 120000,
    embedding_model: "",
    embedding_base_url: "",
    embedding_api_key: "",
    embedding_api_key_env: "",
  },
  voice: {
    stt: {
      provider: "openai-compatible",
      base_url: "https://api.openai.com/v1",
      model: "",
      language: "zh",
      api_key: "",
      api_key_env: "OPENAI_API_KEY",
      timeout_ms: 60000,
    },
    tts: {
      provider: "minimax",
      base_url: "https://api.minimax.io",
      model: "speech-2.8-turbo",
      voice: "female-shaonv",
      language: "zh",
      api_key: "",
      api_key_env: "",
      timeout_ms: 60000,
    },
  },
  permissions: { mode: "ask", interrupt_on: ["bash", "write_file", "edit_file", "http_request"] },
  sandbox: { profile: "workspace-write", writable_paths: [], denied_write_paths: [] },
  skills: { directories: [], progressive_loading: true },
  subagents: { directories: [] },
  compaction: { enabled: true, context_window: 200000, trigger_threshold: 0.8, keep_recent_tokens: 20000 },
};

export const SECTIONS: { key: SectionKey; label: string }[] = [
  { key: "model", label: "模型" },
  { key: "voice", label: "语音" },
  { key: "agent", label: "智能体" },
  { key: "permissions", label: "权限与安全" },
  { key: "sandbox", label: "沙箱" },
  { key: "compaction", label: "压缩" },
  { key: "skills", label: "技能与子智能体" },
  { key: "appearance", label: "外观" },
];

/** Only Cyrene system / light / dark are offered; there is no custom theme. */
export const APPEARANCE_MODES = [
  {
    id: "system",
    label: "跟随系统",
    description: "自动使用系统的明暗外观",
    icon: Monitor,
  },
  {
    id: "light",
    label: "白天",
    description: "清透的月光白与淡粉紫界面",
    icon: Sun,
  },
  {
    id: "dark",
    label: "夜间",
    description: "低眩光的深紫色夜色界面",
    icon: Moon,
  },
] as const;
