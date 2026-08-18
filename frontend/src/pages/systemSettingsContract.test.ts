import { describe, expect, it } from "vitest";
import source from "./SettingsPage.tsx?raw";
import modelManagerSource from "../features/llm/ModelManagerPanel.tsx?raw";

describe("Settings contract", () => {
  it("keeps real settings APIs and masked secret controls", () => {
    expect(source).toContain("getSettings");
    expect(source).toContain("updateSettings");
    expect(modelManagerSource).toContain('type="password"');
    expect(modelManagerSource).toContain("api_key_configured");
  });

  it("provides section semantics and save feedback", () => {
    expect(source).toContain("aria-current");
    expect(source).toContain("保存设置");
    expect(source).toContain("未保存");
    expect(source).toContain("保存失败");
  });

  it("integrates legacy model controls into the model manager", () => {
    expect(source).toContain("<ModelManagerPanel");
    expect(source).toContain("legacyModel");
    expect(modelManagerSource).toContain("运行时兼容配置");
    expect(modelManagerSource).toContain("API 地址");
    expect(modelManagerSource).toContain("Embedding 配置");
    expect(source).not.toContain('<Input label="API 地址"');
    expect(source).not.toContain('<h4 className="font-semibold text-sm text-[var(--text-muted)]">Embedding 配置</h4>');
  });
});
