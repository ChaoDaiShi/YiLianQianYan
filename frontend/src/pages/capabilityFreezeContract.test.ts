import { describe, expect, it } from "vitest";
import appSource from "../App.tsx?raw";
import navRailSource from "../components/layout/NavRail.tsx?raw";
import skillsSource from "./SkillsPage.tsx?raw";
import agentsSource from "./AgentsPage.tsx?raw";
import capabilitiesSource from "./CapabilitiesPage.tsx?raw";

describe("Capability Center freeze and responsive contract", () => {
  it("keeps all four routes and the frozen compact NavRail", () => {
    expect(appSource).toContain('path="skills"');
    expect(appSource).toContain('path="plugins"');
    expect(appSource).toContain('path="agents"');
    expect(appSource).toContain('path="capabilities"');
    expect(navRailSource).toContain("min-[960px]:w-[80px]");
    expect(navRailSource).toContain("min-[960px]:w-[68px]");
  });

  it("defines token-based capability surfaces with responsive internal scrolling", () => {
    expect(skillsSource).toContain('className="capability-page skills-page"');
    expect(agentsSource).toContain("capability-card-grid");
    expect(capabilitiesSource).toContain("capability-list-scroll");
    expect(capabilitiesSource).toContain("capability-split-layout");
  });
});
