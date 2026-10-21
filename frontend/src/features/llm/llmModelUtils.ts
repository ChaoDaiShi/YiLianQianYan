import type { LlmModel } from "../../types";

export { default as PROVIDER_PRESETS } from "./providerCatalog.json";
import PROVIDER_PRESETS from "./providerCatalog.json";

export type ProviderPresetId = keyof typeof PROVIDER_PRESETS;

export interface ModelTreeNode {
  id: string;
  label: string;
  provider: string;
  model: string;
  active: boolean;
  x: number;
  y: number;
}

export function buildModelTree(models: Array<Pick<LlmModel, "id" | "label" | "provider" | "model" | "active">>): ModelTreeNode[] {
  return models.slice(0, 32).map((model, index) => {
    const column = index % 8;
    const row = Math.floor(index / 8);
    return {
      ...model,
      x: 120 + column * 110,
      y: 100 + row * 90,
    };
  });
}

export function buildUsageQuery(modelId: string, from: string, to: string): string {
  const params = new URLSearchParams();
  if (modelId) params.set("model_id", modelId);
  if (from) params.set("from", String(Date.parse(`${from}T00:00:00.000Z`)));
  if (to) params.set("to", String(Date.parse(`${to}T00:00:00.000Z`) + 86_400_000 + 1));
  const query = params.toString();
  return query ? `?${query}` : "";
}

export function providerLabel(provider: string): string {
  return PROVIDER_PRESETS[provider as ProviderPresetId]?.label ?? provider;
}
