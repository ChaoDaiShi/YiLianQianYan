import { describe, expect, it } from "vitest";
// @ts-expect-error -- Node file access is test-only and not bundled.
import { readFileSync } from "node:fs";
import workspaceSurfaceSource from "../surfaces/workspace/WorkspaceSurface.tsx?raw";
import shellSource from "../components/layout/AppShell.tsx?raw";
import loadingSource from "../components/layout/RouteLoadingSurface.tsx?raw";
import workspaceLayoutSource from "../components/layout/workspaceLayout.ts?raw";

const stylesSource = readFileSync(new URL("../index.css", import.meta.url), "utf8");

const lazyPages = [
  "TaskCenterPage",
  "SystemPage",
  "LogsPage",
  "SettingsPage",
  "SkillsPage",
  "PluginsPage",
  "WorkflowsPage",
  "WorkspacesPage",
  "WorkspaceDetailPage",
  "AgentsPage",
  "CapabilitiesPage",
  "MemoryPage",
  "KnowledgePage",
];

describe("route loading contract", () => {
  it("keeps ChatPage eager and loads feature pages lazily", () => {
    expect(workspaceSurfaceSource).toContain('import ChatPage from "../../pages/ChatPage"');

    for (const page of lazyPages) {
      expect(workspaceSurfaceSource).toContain(`lazy(() => import("../../pages/${page}"))`);
      expect(workspaceSurfaceSource).not.toContain(`import ${page} from "../../pages/${page}"`);
    }
  });

  it("preserves route paths and keeps lazy loading inside the persistent shell", () => {
    for (const path of [
      "chat",
      "chat/:id",
      "tasks",
      "system",
      "logs",
      "settings",
      "workflows",
      "workspaces",
      "workspaces/:id",
      "memory",
      "knowledge",
    ]) {
      expect(workspaceSurfaceSource).toContain(`path="${path}"`);
    }

    expect(workspaceSurfaceSource).not.toContain("Suspense");
    expect(workspaceSurfaceSource).not.toContain("RouteLoadingSurface");
    expect(shellSource).toContain("Suspense");
    expect(shellSource).toContain("RouteLoadingSurface");
    expect(shellSource).toContain("<Outlet />");
  });

  it("avoids a page-wide entrance flash and delays the loading placeholder", () => {
    expect(workspaceLayoutSource).not.toContain("animate-page-in");
    expect(loadingSource).toContain("route-loading-surface");
    expect(loadingSource).toContain("route-loading-placeholder");
    expect(stylesSource).toContain(".route-loading-placeholder");
    expect(stylesSource).toContain("animation-delay: 120ms");
  });
});
