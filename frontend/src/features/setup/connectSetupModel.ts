import { activateLlmModel, createLlmModel, updateLlmModel, verifyLlmModel } from "../../api/system";
import type { LlmModel, LlmModelPayload } from "../../types";

export type ConnectionStage = "saving" | "verifying" | "activating";
export type SetupConnectionResult =
  | { ok: true; model: LlmModel }
  | { ok: false; stage: ConnectionStage };

export async function connectSetupModel(
  payload: LlmModelPayload,
  modelId: string | null,
  onStage: (stage: ConnectionStage) => void,
  onSaved: (model: LlmModel) => void,
): Promise<SetupConnectionResult> {
  onStage("saving");
  const saved = modelId ? await updateLlmModel(modelId, payload) : await createLlmModel(payload);
  if (!saved) return { ok: false, stage: "saving" };
  onSaved(saved);
  onStage("verifying");
  if (!await verifyLlmModel(saved.id)) return { ok: false, stage: "verifying" };
  onStage("activating");
  const model = await activateLlmModel(saved.id);
  return model ? { ok: true, model } : { ok: false, stage: "activating" };
}
