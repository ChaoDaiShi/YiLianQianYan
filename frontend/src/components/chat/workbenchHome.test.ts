import { describe, expect, it } from "vitest";
import workbenchHomeSource from "./WorkbenchHome.tsx?raw";
import chatInputSource from "./ChatInput.tsx?raw";

describe("workbench home responsive composition", () => {
  it("exposes compact-height hooks for the home layout", () => {
    expect(workbenchHomeSource).toContain("home-hero");
    expect(workbenchHomeSource).toContain("home-status");
    expect(chatInputSource).toContain("home-composer");
    expect(workbenchHomeSource).toContain("home-content");
    expect(workbenchHomeSource).toContain("home-character-image");
    expect(workbenchHomeSource).toContain("/cyrene-home-character.png");
    expect(workbenchHomeSource).toContain("home-quick-actions");
    expect(workbenchHomeSource).toContain("home-quick-action");
  });
});
