import { describe, expect, it } from "vitest";
import workspaceSurfaceSource from "../surfaces/workspace/WorkspaceSurface.tsx?raw";
import navRailSource from "../components/layout/NavRail.tsx?raw";
import systemSource from "./SystemPage.tsx?raw";
import logsSource from "./LogsPage.tsx?raw";
import settingsSource from "../features/settings/canvas/SettingsCanvasPage.tsx?raw";
import managerSource from "../features/settings/canvas/SettingsNodeManager.tsx?raw";
// @ts-expect-error -- Node file access is test-only and not bundled.
import { readFileSync } from "node:fs";

const desktopWorkspaceStyles = readFileSync(new URL("../styles/desktop-workspace.css", import.meta.url), "utf8");

describe("System Center freeze", () => {
  it("keeps system routes and compact NavRail sizing", () => {
    expect(workspaceSurfaceSource).toContain('path="system"');
    expect(workspaceSurfaceSource).toContain('path="logs"');
    expect(workspaceSurfaceSource).toContain('path="settings"');
    expect(desktopWorkspaceStyles).toMatch(/\.app-shell \.nav-rail\s*\{\s*width: 62px;/);
    expect(navRailSource).toContain("--sidebar-active");
  });

  it("keeps the three pages token-based and accessible", () => {
    expect(systemSource).toContain("var(--");
    expect(settingsSource).toContain("settings-canvas-page");
    expect(settingsSource).toContain("settings-scope-label");
    expect(logsSource).toContain("system-center-page");
    expect(logsSource).toContain("aria-label");
    expect(settingsSource).toContain("aria-label");
    expect(systemSource).toContain("min-h-0");
    expect(logsSource).toContain("Modal");
    expect(managerSource).toContain("aria-expanded");
  });
});
