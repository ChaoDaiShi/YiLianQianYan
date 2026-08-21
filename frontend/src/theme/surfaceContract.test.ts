import { describe, expect, it } from "vitest";
// @ts-expect-error -- Node file access is test-only and not bundled.
import { readFileSync } from "node:fs";
import appShellSource from "../components/layout/AppShell.tsx?raw";
import agentsSource from "../pages/AgentsPage.tsx?raw";
import capabilitiesSource from "../pages/CapabilitiesPage.tsx?raw";
import chatSource from "../pages/ChatPage.tsx?raw";
import knowledgeSource from "../pages/KnowledgePage.tsx?raw";
import logsSource from "../pages/LogsPage.tsx?raw";
import memorySource from "../pages/MemoryPage.tsx?raw";
import pluginsSource from "../pages/PluginsPage.tsx?raw";
import settingsSource from "../pages/SettingsPage.tsx?raw";
import skillsSource from "../pages/SkillsPage.tsx?raw";
import systemSource from "../pages/SystemPage.tsx?raw";
import tasksSource from "../pages/TaskCenterPage.tsx?raw";
import workflowsSource from "../pages/WorkflowsPage.tsx?raw";
import workspaceDetailSource from "../pages/WorkspaceDetailPage.tsx?raw";
import workspacesSource from "../pages/WorkspacesPage.tsx?raw";

const css = readFileSync(new URL("../index.css", import.meta.url), "utf8");

function rule(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return css.match(new RegExp(`${escaped}\\s*\\{([\\s\\S]*?)\\}`))?.[1] ?? "";
}

describe("Cyrene canvas and surface contract", () => {
  it("keeps AppShell as the single canvas owner", () => {
    expect(appShellSource).toContain("shell-ambient");
    expect(appShellSource).toContain("var(--bg-app)");
    expect(appShellSource).toContain("var(--shell-overlay)");
  });

  it.each([
    agentsSource,
    capabilitiesSource,
    chatSource,
    knowledgeSource,
    logsSource,
    memorySource,
    pluginsSource,
    settingsSource,
    skillsSource,
    systemSource,
    tasksSource,
    workflowsSource,
    workspaceDetailSource,
    workspacesSource,
  ])("marks every business page as a transparent page canvas", (source) => {
    expect(source).toContain("page-canvas");
  });

  it("defines page canvas without an opaque fill", () => {
    const body = rule(".page-canvas");
    expect(body).toContain("background: transparent");
    expect(body).not.toContain("var(--bg-app)");
    expect(body).not.toContain("var(--surface-solid)");
  });

  it("keeps the workflow grid without an opaque canvas color", () => {
    const body = rule(".workflow-canvas-viewport");
    expect(body).toContain("background-image");
    expect(body).not.toContain("background-color: var(--bg-app)");
  });

  it("uses a translucent conversation wash", () => {
    const body = rule('.workbench-grid[data-chat-view="conversation"]');
    expect(body).toContain("var(--page-wash)");
    expect(body).not.toMatch(/rgba\([^)]*,\s*0\.9[2-9]\)/);
  });

  it("keeps full viewport blur disabled", () => {
    expect(rule(".workbench-grid")).not.toContain("backdrop-filter");
    expect(rule(".app-shell")).not.toContain("backdrop-filter");
  });
});
