import { describe, expect, it } from "vitest";
import workspaceSurfaceSource from "../surfaces/workspace/WorkspaceSurface.tsx?raw";
import skillsSource from "./SkillsPage.tsx?raw";
import agentsSource from "./AgentsPage.tsx?raw";
import capabilitiesSource from "../features/capabilities/CapabilityRegistry.tsx?raw";
// @ts-expect-error -- Node file access is test-only and not bundled.
import { readFileSync } from "node:fs";

const desktopWorkspaceStyles = readFileSync(new URL("../styles/desktop-workspace.css", import.meta.url), "utf8");

describe("Capability Center freeze and responsive contract", () => {
  it("keeps capability and agent routes with compact NavRail, removes standalone editors", () => {
    expect(workspaceSurfaceSource).not.toContain('path="skills"');
    expect(workspaceSurfaceSource).not.toContain('path="plugins"');
    expect(workspaceSurfaceSource).toContain('path="agents"');
    expect(workspaceSurfaceSource).toContain('path="capabilities"');
    expect(desktopWorkspaceStyles).toMatch(/\.app-shell \.nav-rail\s*\{\s*width: 62px;/);
    expect(desktopWorkspaceStyles).toMatch(/@media \(max-width: 959px\)\s*\{\s*\.app-shell \.nav-rail\s*\{\s*width: 50px;/);
  });

  it("defines token-based capability surfaces with responsive internal scrolling", () => {
    expect(skillsSource).toContain("capability-page skills-page page-canvas");
    expect(agentsSource).toContain("management-agent-list-scroll");
    expect(agentsSource).toContain("management-agent-editor");
    expect(capabilitiesSource).toContain("capability-list-scroll");
    expect(capabilitiesSource).toContain("capability-split-layout");
  });
});
