import { describe, expect, it } from "vitest";
import source from "./SettingsPage.tsx?raw";

describe("Settings contract", () => {
  it("keeps real settings APIs and masked secret controls", () => {
    expect(source).toContain("getSettings");
    expect(source).toContain("updateSettings");
    expect(source).toContain('type="password"');
    expect(source).toContain("api_key_configured");
  });

  it("provides section semantics and save feedback", () => {
    expect(source).toContain("aria-current");
    expect(source).toContain("保存设置");
    expect(source).toContain("未保存");
    expect(source).toContain("保存失败");
  });
});
