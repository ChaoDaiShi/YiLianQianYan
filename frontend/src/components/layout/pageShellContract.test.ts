import { describe, expect, it } from "vitest";
import { NAV_ENTRIES, getNavEntryById, getNavEntryByPath } from "./navRegistry";
import pageShellSource from "./PageShell.tsx?raw";

describe("PageShell and Navigation Registry Contracts", () => {
  it("defines single source of truth for all 15 navigation routes", () => {
    expect(NAV_ENTRIES.length).toBe(15);
    const routes = NAV_ENTRIES.map((e) => e.to);
    expect(routes).toContain("/tasks");
    expect(routes).toContain("/chat");
    expect(routes).toContain("/search");
    expect(routes).toContain("/workflows");
    expect(routes).toContain("/workspaces");
    expect(routes).toContain("/capabilities");
    expect(routes).toContain("/settings");

    expect(getNavEntryById("tasks")?.label).toBe("任务");
    expect(getNavEntryByPath("/chat")?.id).toBe("chat");
    expect(getNavEntryByPath("/chat/123")?.id).toBe("chat");
  });

  it("ensures PageShell exposes standard and workspace variants with transparent canvas", () => {
    expect(pageShellSource).toContain("page-canvas");
    expect(pageShellSource).toContain("page-header");
    expect(pageShellSource).toContain("PageShellHeader");
    expect(pageShellSource).toContain("variant");
    expect(pageShellSource).toContain("maxWidth");
  });
});
