import type { AppConfig, LlmModel, LlmModelPayload } from "../../types";
import type { AccountServiceKind, AccountServiceProfile } from "../settings/basicServices";
import { credentialChoice } from "./accountCredentials";
import { PROVIDER_PRESETS, type ProviderPresetId } from "./llmModelUtils";
import type { UpstreamModel } from "./ModelCatalogField";
import { buildConfiguredModels, type ModelPurpose, type ProviderModelEntity } from "./modelConfigurationState";
import { listModelPools, listProviderAccounts, poolRequest, type ProviderAccount } from "./modelPools";

export interface DefaultSource { source: "profile" | "pool" | null; id: string | null }
export const accountServiceKinds = ["vision", "image", "stt", "tts", "embedding"] as const;
const accountPath = "/api/provider-accounts";
const chatPath = "/api/llm/models";
const servicePath = "/api/service-profiles";
const defaultChatBinding = "/api/model-pool-bindings/default";
const recordPath = (path: string, id: string) => `${path}/${encodeURIComponent(id)}`;

export async function loadModelConfiguration() {
  const [accounts, chats, services, entities, config, pools, chatBinding, ...serviceDefaults] = await Promise.all([
    listProviderAccounts(),
    poolRequest<LlmModel[]>("GET", chatPath),
    poolRequest<AccountServiceProfile[]>("GET", servicePath),
    poolRequest<ProviderModelEntity[]>("GET", "/api/llm/provider-models"),
    poolRequest<AppConfig>("GET", "/api/settings"),
    listModelPools(),
    poolRequest<{ pool_id: string | null }>("GET", defaultChatBinding),
    ...accountServiceKinds.map(kind => poolRequest<DefaultSource>("GET", `/api/service-defaults/${kind}`)),
  ] as const);
  const defaults: Record<ModelPurpose, DefaultSource> = {
    chat: chatBinding.pool_id ? { source: "pool", id: chatBinding.pool_id } : { source: "profile", id: chats.find(chat => chat.active)?.id ?? null },
    vision: serviceDefaults[0], image: serviceDefaults[1], stt: serviceDefaults[2], tts: serviceDefaults[3], embedding: serviceDefaults[4],
  };
  const defaultIds = Object.fromEntries(Object.entries(defaults).map(([purpose, source]) => [purpose, source.id]));
  return { accounts, chats, services, config, pools, defaults, rows: buildConfiguredModels(accounts, chats, services, entities, defaultIds) };
}

export type ModelConfigurationData = Awaited<ReturnType<typeof loadModelConfiguration>>;

export function discoverProviderModels(provider: string, baseUrl: string, apiKey: string, accountId?: string, credentialModelId?: string) {
  return poolRequest<{ models: UpstreamModel[] }>("POST", `${chatPath}/discover`, {
    provider, base_url: baseUrl, api_key: apiKey.trim(), provider_account_id: accountId, model_id: credentialModelId,
  });
}

export function saveCatalogAccount(provider: string, baseUrl: string, label: string, apiKey: string, credentialModelId: string, existing?: ProviderAccount) {
  return poolRequest<ProviderAccount>(existing ? "PUT" : "POST", existing ? recordPath(accountPath, existing.id) : accountPath, {
    id: existing?.id ?? "", provider_id: provider, base_url: baseUrl, label,
    profile_ids: existing?.profile_ids ?? [], ...credentialChoice(apiKey.trim(), credentialModelId),
  });
}

export function createModelPurpose(account: ProviderAccount, model: string, label: string, purpose: ModelPurpose, maxTokens?: number | null) {
  if (purpose === "chat") {
    const payload: LlmModelPayload = {
      provider_account_id: account.id, provider: account.provider_id, base_url: account.base_url, label, model,
      api_format: "openai", temperature: account.provider_id === "anthropic" ? 1 : 0,
      max_tokens: maxTokens && maxTokens <= 1_000_000 ? maxTokens : 16_384, invoke_timeout_ms: 120_000,
    };
    return poolRequest<LlmModel>("POST", chatPath, payload);
  }
  const adapters = PROVIDER_PRESETS[account.provider_id as ProviderPresetId]?.adapter_by_service as Partial<Record<AccountServiceKind, string>> | undefined;
  return poolRequest<AccountServiceProfile>("POST", servicePath, {
    label, model, provider_account_id: account.id, service_kind: purpose, adapter: adapters?.[purpose] ?? "openai", parameters: {},
  });
}

export function removeModelPurpose(purpose: ModelPurpose, id: string) {
  return poolRequest("DELETE", recordPath(purpose === "chat" ? chatPath : servicePath, id));
}

export async function useAsDefault(purpose: ModelPurpose, id: string, existing: DefaultSource) {
  if (purpose === "chat") {
    await poolRequest("POST", `${recordPath(chatPath, id)}/activate`);
    if (existing.source === "pool") await poolRequest("PUT", defaultChatBinding, { pool_id: null });
  } else {
    await poolRequest("PUT", `/api/service-defaults/${purpose}`, { source: "profile", id });
  }
}

export function verifyConfiguredModel(purpose: ModelPurpose, id: string, imageUrl?: string) {
  if (purpose === "chat") return poolRequest("POST", `${recordPath(chatPath, id)}/verify`);
  if (purpose === "vision" || purpose === "image") {
    return poolRequest<{ text?: string; image_url?: string }>("POST", `${recordPath(servicePath, id)}/test`, {
      prompt: purpose === "vision" ? "请用一句话描述图片内容。" : "白色背景上的一朵粉色莲花，简洁插画。", image_url: imageUrl,
    });
  }
  return poolRequest("POST", `/api/providers/${purpose}/verify`);
}
