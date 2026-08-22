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

  it("exposes only Cyrene system light and dark appearance choices", () => {
    expect(source).toContain("跟随系统");
    expect(source).toContain("白天");
    expect(source).toContain("夜间");
    expect(source).toContain("theme.setMode");
    expect(source).toContain('role="radiogroup"');
    expect(source).not.toContain("暖色本地");
    expect(source).not.toContain("精密中性");
    expect(source).not.toContain("石墨专业");
    expect(source).not.toContain("高对比");
  });

  it("keeps background images but hides advanced theme editing", () => {
    expect(source).toContain("上传背景图");
    expect(source).toContain("清除背景");
    expect(source).not.toContain("自定义 CSS 变量");
    expect(source).not.toContain("导出主题");
    expect(source).not.toContain("导入主题");
    expect(source).not.toContain("面板透明度");
    expect(source).not.toContain(">模糊<");
    expect(source).not.toContain(">字号<");
  });
});
