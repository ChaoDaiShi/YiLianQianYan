import { describe, expect, it } from "vitest";
import appSource from "../App.tsx?raw";

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
    expect(appSource).toContain('import ChatPage from "./pages/ChatPage"');

    for (const page of lazyPages) {
      expect(appSource).toContain(`lazy(() => import("./pages/${page}"))`);
      expect(appSource).not.toContain(`import ${page} from "./pages/${page}"`);
    }
  });

  it("preserves route paths and provides a shared Suspense surface", () => {
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
      expect(appSource).toContain(`path="${path}"`);
    }

    expect(appSource).toContain("Suspense");
    expect(appSource).toContain("RouteLoadingSurface");
  });
});
