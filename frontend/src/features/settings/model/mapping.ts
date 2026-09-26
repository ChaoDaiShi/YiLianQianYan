import type { AppConfig } from "../../../types";

/**
 * Redacted comparison projection.
 *
 * Secrets are blanked before comparison so that merely loading a masked config,
 * or the user typing then clearing a key, never registers as an unsaved change.
 * `clear_*` flags are dropped for the same reason: they are a pending action,
 * not a value the user can see.
 */
export function comparableConfig(config: AppConfig) {
  return JSON.stringify({
    ...config,
    model: {
      ...config.model,
      api_key: "",
      embedding_api_key: "",
      clear_api_key: undefined,
      clear_embedding_api_key: undefined,
    },
    voice: {
      ...config.voice,
      stt: { ...config.voice.stt, api_key: "", clear_api_key: undefined },
      tts: { ...config.voice.tts, api_key: "", clear_api_key: undefined },
    },
  });
}

/** How a configured provider credential is being supplied right now. */
export function secretSourceLabel(source?: string): string {
  switch (source) {
    case "secret_store":
      return "已安全保存到系统凭据库";
    case "environment":
      return "由环境变量提供";
    case "legacy_pending":
      return "检测到旧版明文密钥，待迁移";
    case "none":
    default:
      return "未配置";
  }
}
