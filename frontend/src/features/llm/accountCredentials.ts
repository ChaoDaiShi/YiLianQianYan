import type { LlmModel } from "../../types";
import type { ProviderAccount } from "./modelPools";

export function eligibleCredentialModels(models: LlmModel[], provider: string, baseUrl: string) {
  return models.filter(model => model.provider === provider && model.base_url.trim().replace(/\/+$/, "") === baseUrl.trim().replace(/\/+$/, "") && model.api_key_source === "secret_store" && model.api_key_configured);
}
export function credentialChoice(key: string, modelId: string) {
  return key.trim() ? { credential_action: "replace" as const, credential_fields: { api_key: key } } : modelId ? { credential_action: "reuse_model" as const, credential_model_id: modelId, credential_fields: {} } : { credential_action: "keep" as const, credential_fields: {} };
}
export function credentialReady(account: ProviderAccount | undefined, provider: string, key: string, modelId: string) {
  return Boolean(key.trim() || modelId || account?.credential_status === "configured" || account?.credential_status === "not_required" || (!account && (provider === "ollama" || provider === "lmstudio")));
}
export const credentialLabels = { configured: "凭据已配置", missing: "凭据未配置", unavailable: "凭据存储不可用", not_required: "本地服务无需凭据" } as const;
