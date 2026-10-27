import { beforeEach, describe, expect, it, vi } from "vitest";
import { activateLlmModel, createLlmModel, updateLlmModel, verifyLlmModel } from "../../api/system";
import type { LlmModel, LlmModelPayload } from "../../types";
import { connectSetupModel } from "./connectSetupModel";

vi.mock("../../api/system", () => ({
  createLlmModel: vi.fn(), updateLlmModel: vi.fn(),
  verifyLlmModel: vi.fn(), activateLlmModel: vi.fn(),
}));

const payload: LlmModelPayload = {
  provider: "deepseek", label: "deepseek-chat", model: "deepseek-chat",
  base_url: "https://api.deepseek.com/v1", api_format: "openai",
  temperature: 0, max_tokens: 16384, invoke_timeout_ms: 120000,
};
const saved = { ...payload, id: "first-model", active: false } as LlmModel;

describe("first-model connection", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(createLlmModel).mockResolvedValue(saved);
    vi.mocked(updateLlmModel).mockResolvedValue(saved);
    vi.mocked(verifyLlmModel).mockResolvedValue({ model: { ...saved, verified_at: 123 } });
    vi.mocked(activateLlmModel).mockResolvedValue({ ...saved, active: true, verified_at: 123 });
  });

  it("saves, verifies and activates in order, retaining the saved profile before verification", async () => {
    const events: string[] = [];
    const result = await connectSetupModel(payload, null, stage => events.push(stage), model => events.push(model.id));
    expect(events).toEqual(["saving", "first-model", "verifying", "activating"]);
    expect(result).toMatchObject({ ok: true, model: { id: "first-model", active: true } });
    expect(verifyLlmModel).toHaveBeenCalledWith("first-model");
    expect(activateLlmModel).toHaveBeenCalledWith("first-model");
    expect(createLlmModel).toHaveBeenCalledExactlyOnceWith(payload);
  });

  it("stops at a save failure without verifying or activating", async () => {
    vi.mocked(createLlmModel).mockResolvedValue(null);
    expect(await connectSetupModel(payload, null, vi.fn(), vi.fn())).toEqual({ ok: false, stage: "saving" });
    expect(verifyLlmModel).not.toHaveBeenCalled();
    expect(activateLlmModel).not.toHaveBeenCalled();
  });

  it("keeps a failed verification profile for an explicit retry without activation", async () => {
    vi.mocked(verifyLlmModel).mockResolvedValue(null);
    const remember = vi.fn();
    expect(await connectSetupModel(payload, null, vi.fn(), remember)).toEqual({ ok: false, stage: "verifying" });
    expect(remember).toHaveBeenCalledExactlyOnceWith(saved);
    expect(activateLlmModel).not.toHaveBeenCalled();
    await connectSetupModel(payload, saved.id, vi.fn(), remember);
    expect(updateLlmModel).toHaveBeenCalledExactlyOnceWith(saved.id, payload);
    expect(createLlmModel).toHaveBeenCalledTimes(1);
  });

  it("does not report completion if activation fails", async () => {
    vi.mocked(activateLlmModel).mockResolvedValue(null);
    expect(await connectSetupModel(payload, saved.id, vi.fn(), vi.fn())).toEqual({ ok: false, stage: "activating" });
  });
});
