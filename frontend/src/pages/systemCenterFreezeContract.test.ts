import { describe, expect, it } from "vitest";
import workspaceSurfaceSource from "../surfaces/workspace/WorkspaceSurface.tsx?raw";
import navRailSource from "../components/layout/NavRail.tsx?raw";
import systemSource from "./SystemPage.tsx?raw";
import logsSource from "./LogsPage.tsx?raw";
import settingsSource from "./SettingsPage.tsx?raw";

describe("System Center freeze", () => {
  it("keeps system routes and compact NavRail sizing", () => {
    expect(workspaceSurfaceSource).toContain('path="system"');
    expect(workspaceSurfaceSource).toContain('path="logs"');
    expect(workspaceSurfaceSource).toContain('path="settings"');
    expect(navRailSource).toContain("min-[960px]:w-[80px]");
    expect(navRailSource).toContain("--sidebar-active");
  });

  it("keeps the three pages token-based and accessible", () => {
    expect(systemSource).toContain("var(--");
    expect(settingsSource).toContain("var(--");
    expect(logsSource).toContain("system-center-page");
    expect(logsSource).toContain("aria-label");
    expect(settingsSource).toContain("aria-label");
    expect(systemSource).toContain("min-h-0");
    expect(logsSource).toContain("Drawer");
    expect(settingsSource).toContain("aria-current");
  });
});
