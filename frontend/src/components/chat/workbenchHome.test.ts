import { describe, expect, it } from "vitest";
import workbenchHomeSource from "./WorkbenchHome.tsx?raw";
import chatInputSource from "./ChatInput.tsx?raw";

describe("workbench home responsive composition", () => {
  it("exposes compact-height hooks for the home layout", () => {
    expect(workbenchHomeSource).toContain("home-hero");
    expect(workbenchHomeSource).toContain("home-status");
    expect(chatInputSource).toContain("home-composer");
    expect(workbenchHomeSource).toContain("home-content");
    expect(workbenchHomeSource).toContain("home-brand-button");
    expect(workbenchHomeSource).toContain("onOpenCurrentTask");
    expect(workbenchHomeSource).toContain('"打开当前任务"');
    expect(workbenchHomeSource).toContain('"打开任务工作流"');
    expect(workbenchHomeSource).not.toContain('aria-label="打开连接器"');
    expect(workbenchHomeSource).toContain("/favicon.png");
    expect(workbenchHomeSource).toContain("home-quick-actions");
    expect(workbenchHomeSource).toContain("home-quick-action");
    expect(chatInputSource).toContain("composer-actions");
  });
});
