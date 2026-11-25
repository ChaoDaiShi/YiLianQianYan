import { describe, expect, it } from "vitest";
import { availablePurposes, buildConfiguredModels, catalogPurposeHints, planModelChanges, type ProviderModelEntity } from "./modelConfigurationState";
import type { LlmModel } from "../../types";
import type { ProviderAccount } from "./modelPools";
import type { BasicServiceProfile } from "../settings/basicServices";

const account: ProviderAccount = { id: "provider-a", label: "账户 A", provider_id: "custom", base_url: "https://gateway.example/v1", profile_ids: ["chat-a"], credential_id: "fixture-ref" };
const chat = { id: "chat-a", label: "对话模型", provider: "custom", model: "model-x", base_url: account.base_url, active: true } as LlmModel;
const vision = { id: "vision-a", label: "图像理解", provider_account_id: account.id, service_kind: "vision", model: "model-x", adapter: "openai" } as BasicServiceProfile;
const entity: ProviderModelEntity = { provider_account_id: account.id, model: "model-x", profiles: { chat: [chat.id], vision: [vision.id] } };

describe("provider model configuration", () => {
  it("shows a multi-purpose model once and keeps identical IDs on distinct accounts separate", () => {
    const otherAccount = { ...account, id: "provider-b", profile_ids: [] };
    const rows = buildConfiguredModels([account, otherAccount], [chat], [vision, { ...vision, id: "vision-b", provider_account_id: otherAccount.id }], [entity, { ...entity, provider_account_id: otherAccount.id, profiles: { vision: ["vision-b"] } }]);
    expect(rows.map(row => [row.account?.id, row.model, Object.keys(row.profiles)])).toEqual([
      [account.id, "model-x", ["chat", "vision"]], [otherAccount.id, "model-x", ["vision"]],
    ]);
  });

  it("plans only added and removed purposes while preserving existing profile IDs", () => {
    const [row] = buildConfiguredModels([account], [chat], [vision], [entity]);
    expect(planModelChanges({ "model-x": { name: "多模态", purposes: ["chat", "image"] } }, [row])).toEqual([
      { model: "model-x", name: "多模态", add: ["image"], remove: [{ purpose: "vision", id: "vision-a" }] },
    ]);
  });

  it("uses explicit catalog metadata without inferring capabilities from model names", () => {
    expect(catalogPurposeHints({ id: "vision-whisper-image", name: "fake" })).toEqual([]);
    expect(catalogPurposeHints({ id: "any", name: "any", input_modalities: ["text", "image"], output_modalities: ["text"] })).toEqual(["chat", "vision"]);
    expect(catalogPurposeHints({ id: "any", name: "any", input_modalities: ["audio"], output_modalities: ["text"] })).toEqual(["stt"]);
    expect(availablePurposes("custom")).toContain("stt");
    expect(availablePurposes("deepseek")).not.toContain("tts");
    expect(availablePurposes("anthropic")).toEqual(["chat", "vision"]);
  });
});
