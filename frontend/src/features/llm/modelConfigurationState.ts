import type { LlmModel } from "../../types";
import type { AccountServiceKind, AccountServiceProfile } from "../settings/basicServices";
import type { UpstreamModel } from "./ModelCatalogField";
import { PROVIDER_PRESETS, type ProviderPresetId } from "./llmModelUtils";
import type { ProviderAccount } from "./modelPools";

export type ModelPurpose = "chat" | AccountServiceKind;
export const modelPurposes: Record<ModelPurpose, string> = {
  chat: "对话", vision: "图像理解", image: "图像生成", stt: "语音识别", tts: "语音合成", embedding: "向量检索",
};
const allPurposes = Object.keys(modelPurposes) as ModelPurpose[];

export function availablePurposes(provider: string): ModelPurpose[] {
  if (provider === "custom") return allPurposes;
  const preset = PROVIDER_PRESETS[provider as ProviderPresetId];
  return allPurposes.filter(purpose => (preset?.services as string[] | undefined)?.includes(purpose));
}

export function catalogPurposeHints(model: UpstreamModel): ModelPurpose[] {
  const input = model.input_modalities?.map(value => value.toLowerCase()) ?? [];
  const output = model.output_modalities?.map(value => value.toLowerCase()) ?? [];
  const purposes: ModelPurpose[] = [];
  if (input.includes("text") && output.includes("text")) purposes.push("chat");
  if (input.includes("image") && output.includes("text")) purposes.push("vision");
  if (output.includes("image")) purposes.push("image");
  if (input.includes("audio") && output.includes("text")) purposes.push("stt");
  if (input.includes("text") && output.includes("audio")) purposes.push("tts");
  if (output.includes("embedding")) purposes.push("embedding");
  return purposes;
}

export interface ConfiguredModel {
  key: string;
  account?: ProviderAccount;
  provider: string;
  baseUrl: string;
  model: string;
  name: string;
  profiles: Partial<Record<ModelPurpose, LlmModel | AccountServiceProfile>>;
}

export interface ProviderModelEntity {
  provider_account_id: string;
  model: string;
  profiles: Partial<Record<ModelPurpose, string[]>>;
}

export function buildConfiguredModels(accounts: ProviderAccount[], chatModels: LlmModel[], services: AccountServiceProfile[], entities: ProviderModelEntity[], defaultIds?: Partial<Record<ModelPurpose, string | null>>): ConfiguredModel[] {
  const accountById = new Map(accounts.map(account => [account.id, account]));
  const chatById = new Map(chatModels.map(profile => [profile.id, profile]));
  const serviceById = new Map(services.map(profile => [profile.id, profile]));
  const linkedChat = new Set(entities.flatMap(entity => entity.profiles.chat ?? []));
  const accountRows = entities.flatMap(({ provider_account_id, model, profiles }) => {
    const account = accountById.get(provider_account_id);
    if (!account) return [];
    const current: ConfiguredModel["profiles"] = {};
    for (const purpose of allPurposes) {
      const byId = purpose === "chat" ? chatById : serviceById;
      const candidates = profiles[purpose]?.flatMap(id => { const profile = byId.get(id); return profile ? [profile] : []; }) ?? [];
      const profile = candidates.find(item => item.id === defaultIds?.[purpose]) ?? candidates[0];
      if (profile) current[purpose] = profile;
    }
    const name = current.chat?.label ?? Object.values(current)[0]?.label ?? model;
    return [{ key: JSON.stringify([account.id, model]), account, provider: account.provider_id, baseUrl: account.base_url, model, name, profiles: current }];
  });
  const independentRows = chatModels.filter(profile => !linkedChat.has(profile.id)).map(profile => ({
    key: JSON.stringify([profile.id, profile.model]), provider: profile.provider, baseUrl: profile.base_url, model: profile.model, name: profile.label, profiles: { chat: profile },
  }));
  return [...accountRows, ...independentRows];
}

export type DraftModels = Record<string, { name: string; purposes: ModelPurpose[] }>;

export function planModelChanges(draft: DraftModels, existing: ConfiguredModel[]) {
  return Object.entries(draft).flatMap(([model, { name, purposes }]) => {
    const profiles = existing.find(row => row.model === model)?.profiles ?? {};
    const add = purposes.filter(purpose => !profiles[purpose]);
    const remove = Object.entries(profiles).flatMap(([purpose, profile]) => purposes.includes(purpose as ModelPurpose) ? [] : [{ purpose: purpose as ModelPurpose, id: profile.id }]);
    return add.length || remove.length ? [{ model, name, add, remove }] : [];
  });
}
