import { describe, expect, it } from "vitest";
import pageSource from "./SettingsPage.tsx?raw";
import configSource from "../features/settings/model/config.ts?raw";
import mappingSource from "../features/settings/model/mapping.ts?raw";
import agentSettingsSource from "../features/settings/sections/AgentSettings.tsx?raw";
import appearanceSettingsSource from "../features/settings/sections/AppearanceSettings.tsx?raw";
import compactionSettingsSource from "../features/settings/sections/CompactionSettings.tsx?raw";
import modelSettingsSource from "../features/settings/sections/ModelSettings.tsx?raw";
import permissionSettingsSource from "../features/settings/sections/PermissionSettings.tsx?raw";
import providerReadinessCardSource from "../features/settings/sections/ProviderReadinessCard.tsx?raw";
import sandboxSettingsSource from "../features/settings/sections/SandboxSettings.tsx?raw";
import skillPathSettingsSource from "../features/settings/sections/SkillPathSettings.tsx?raw";
import voiceSettingsSource from "../features/settings/sections/VoiceSettings.tsx?raw";
import modelManagerSource from "../features/llm/ModelManagerPanel.tsx?raw";
import moduleSetupSource from "../components/system/ModuleSetup.tsx?raw";
import typesSource from "../types/index.ts?raw";

// Settings is one surface split across a composition page, its model and its
// sections. These assertions are about that surface, so they read all of it —
// each string below is still required to exist somewhere on the surface.
const source = [
  pageSource,
  configSource,
  mappingSource,
  providerReadinessCardSource,
  agentSettingsSource,
  appearanceSettingsSource,
  compactionSettingsSource,
  modelSettingsSource,
  permissionSettingsSource,
  sandboxSettingsSource,
  skillPathSettingsSource,
  voiceSettingsSource,
].join("\n");

describe("Settings contract", () => {
  it("keeps real settings APIs and masked secret controls", () => {
    expect(source).toContain("getSettings");
    expect(source).toContain("updateSettings");
    expect(modelManagerSource).toContain('type="password"');
    expect(modelManagerSource).toContain("api_key_configured");
  });

  it("preserves secure voice provider settings and write-only credentials", () => {
    expect(typesSource).toContain("export interface VoiceConfig");
    expect(typesSource).toContain("voice: VoiceConfig");
    expect(source).toContain('{ key: "voice", label: "语音" }');
    expect(typesSource).toContain("export interface VoiceSttConfig");
    expect(typesSource).toContain("export interface VoiceTtsConfig");
    expect(source).toContain("语音识别（STT）");
    expect(source).toContain("语音合成（TTS）");
    expect(source).toContain("config.voice.stt");
    expect(source).toContain("config.voice.tts");
    expect(source).toContain("api_key_configured");
    expect(source).toContain("clearVoiceSecret");
    expect(source).toContain('type="password"');
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
    expect(modelManagerSource).toContain("基础设置");
    expect(modelManagerSource).toContain("API 地址");
    expect(modelManagerSource).toContain("Embedding 配置");
    expect(source).not.toContain('<Input label="API 地址" value={config.model.base_url}');
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

  it("keeps provider credentials write-only and makes settings actions scroll with their card", () => {
    expect(modelManagerSource).toContain("replaceApiKey");
    expect(source).toContain("replaceVoiceKey");
    expect(source).toContain("window.confirm");
    expect(source).toContain("provider_readiness");
    expect(source).toContain("settings-save-card");
    expect(source).not.toContain("system-settings-savebar");
    expect(modelManagerSource).toContain("clearProfileSecret");
    expect(modelManagerSource).toContain("clear_api_key: true");
  });

  it("offers a thin provider step after first-run module setup without requiring voice", () => {
    expect(moduleSetupSource).toContain("开始使用忆涟");
    expect(moduleSetupSource).toContain("getProviderReadiness");
    expect(moduleSetupSource).toContain("模型服务");
    expect(moduleSetupSource).toContain("语音服务（可选）");
    expect(moduleSetupSource).toContain("去配置");
    expect(moduleSetupSource).toContain("稍后设置");
  });
});
