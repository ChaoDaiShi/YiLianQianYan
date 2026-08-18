import { describe, expect, it } from "vitest";
import { buildModelTree, buildUsageQuery, PROVIDER_PRESETS } from "./llmModelUtils";

describe("LLM model utilities", () => {
  it("provides common OpenAI-compatible provider presets", () => {
    expect(PROVIDER_PRESETS.deepseek.baseUrl).toContain("deepseek.com");
    expect(PROVIDER_PRESETS.qwen.baseUrl).toContain("dashscope");
    expect(PROVIDER_PRESETS.glm.baseUrl).toContain("bigmodel.cn");
    expect(PROVIDER_PRESETS.openai.baseUrl).toContain("api.openai.com");
  });

  it("caps tree branches and keeps each model within the viewBox", () => {
    const models = Array.from({ length: 40 }, (_, index) => ({
      id: `model-${index}`,
      label: `模型 ${index}`,
      provider: "custom",
      model: `model-${index}`,
      active: index === 0,
    }));

    const nodes = buildModelTree(models);
    expect(nodes).toHaveLength(32);
    expect(nodes.every((node) => node.x >= 40 && node.x <= 960)).toBe(true);
    expect(nodes.every((node) => node.y >= 60 && node.y <= 390)).toBe(true);
  });

  it("builds a bounded usage query from model and dates", () => {
    expect(buildUsageQuery("model-1", "2026-08-01", "2026-08-18")).toBe(
      "?model_id=model-1&from=1785542400000&to=1787097600001",
    );
  });
});
