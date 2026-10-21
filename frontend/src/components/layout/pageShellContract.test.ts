import { describe, expect, it } from "vitest";
import { NAV_ENTRIES, getNavEntryById, getNavEntryByPath } from "./navRegistry";
import pageShellSource from "./PageShell.tsx?raw";

describe("PageShell and Navigation Registry Contracts", () => {
  it("defines single source of truth for current navigation routes", () => {
    expect(NAV_ENTRIES.length).toBe(11);
    const routes = NAV_ENTRIES.map((e) => e.to);
    expect(routes).toContain("/connectors");
    expect(routes).toContain("/chat");
    expect(routes).not.toContain("/search");
    expect(routes).not.toContain("/workflows");
    expect(routes).toContain("/workspaces");
    expect(routes).toContain("/capabilities");
    expect(routes).toContain("/settings");

    expect(getNavEntryById("connectors")?.label).toBe("画布");
    expect(getNavEntryByPath("/chat")?.id).toBe("chat");
    expect(getNavEntryByPath("/chat/123")?.id).toBe("chat");
  });

  it("ensures PageShell exposes standard and workspace variants with transparent canvas", () => {
    expect(pageShellSource).toContain("page-canvas");
    expect(pageShellSource).toContain("page-header");
    expect(pageShellSource).toContain("PageShellHeader");
    expect(pageShellSource).toContain("hasCustomDisplay");
    expect(pageShellSource).toContain("variant");
    expect(pageShellSource).toContain("maxWidth");
  });

  it("ensures WORKBENCH_VIEWPORT_CLASS_NAME contains grid utility to prevent column collapsing", async () => {
    const { WORKBENCH_VIEWPORT_CLASS_NAME } = await import("./workspaceLayout");
    expect(WORKBENCH_VIEWPORT_CLASS_NAME).toContain("workbench-grid");
    expect(WORKBENCH_VIEWPORT_CLASS_NAME).toContain("grid");
  });
});
