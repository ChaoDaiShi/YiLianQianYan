import { describe, expect, it } from "vitest";
import workbenchHomeSource from "./WorkbenchHome.tsx?raw";
import chatInputSource from "./ChatInput.tsx?raw";

describe("workbench home responsive composition", () => {
  it("exposes compact-height hooks for the home layout", () => {
    expect(workbenchHomeSource).toContain("home-hero");
    expect(workbenchHomeSource).toContain("home-ambient-strong");
    expect(workbenchHomeSource).toContain("home-status");
    expect(chatInputSource).toContain("home-composer");
    expect(workbenchHomeSource).toContain("home-content");
    expect(workbenchHomeSource).toContain("home-character-image");
    expect(workbenchHomeSource).toContain("home-character-scene");
    expect(workbenchHomeSource).toContain("onOpenCurrentTask");
    expect(workbenchHomeSource).toContain('aria-label="打开当前任务画布"');
    expect(workbenchHomeSource).toContain("home-character-ripple");
    expect(workbenchHomeSource).toContain("/cyrene-home-character.png");
    expect(workbenchHomeSource).toContain("home-quick-actions");
    expect(workbenchHomeSource).toContain("home-quick-action");
    expect(workbenchHomeSource).toContain("quick-action-icon-well");
    expect(chatInputSource).toContain("composer-footer");
  });
});
