import { describe, expect, it } from "vitest";
import appSource from "../App.tsx?raw";
import navRailSource from "../components/layout/NavRail.tsx?raw";
import systemSource from "./SystemPage.tsx?raw";
import logsSource from "./LogsPage.tsx?raw";
import settingsSource from "./SettingsPage.tsx?raw";

describe("System Center freeze", () => {
  it("keeps system routes and compact NavRail sizing", () => {
    expect(appSource).toContain('path="system"');
    expect(appSource).toContain('path="logs"');
    expect(appSource).toContain('path="settings"');
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
