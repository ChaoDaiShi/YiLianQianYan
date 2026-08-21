import { describe, expect, it } from "vitest";
import pluginsPageSource from "./PluginsPage.tsx?raw";

describe("Plugins capability surface", () => {
  it("keeps real MCP actions and runtime detail fields", () => {
    expect(pluginsPageSource).toContain("listPlugins");
    expect(pluginsPageSource).toContain("createMcpServer");
    expect(pluginsPageSource).toContain("updateMcpServer");
    expect(pluginsPageSource).toContain("deleteMcpServer");
    expect(pluginsPageSource).toContain("toggleMcpServer");
    expect(pluginsPageSource).toContain("testMcpServer");
    expect(pluginsPageSource).toContain("safe_error");
    expect(pluginsPageSource).toContain("Tools");
    expect(pluginsPageSource).toContain("Resources");
    expect(pluginsPageSource).toContain("Prompts");
    expect(pluginsPageSource).toContain("validateMcpDraft");
    expect(pluginsPageSource).toContain("formError");
    expect(pluginsPageSource).toContain("编辑");
    expect(pluginsPageSource).toContain("删除");
  });

  it("uses shared loading and error surfaces without marketplace or secret exposure", () => {
    expect(pluginsPageSource).toContain("ErrorState");
    expect(pluginsPageSource).toContain("Skeleton");
    expect(pluginsPageSource).toContain("不回显值");
    expect(pluginsPageSource).not.toContain("Marketplace");
    expect(pluginsPageSource).not.toContain("安装插件");
    expect(pluginsPageSource).not.toContain("API Key");
  });
});
