import { describe, expect, it } from "vitest";
import agentsPageSource from "./AgentsPage.tsx?raw";

describe("Agents capability surface", () => {
  it("keeps real Agent fields and the existing create action", () => {
    expect(agentsPageSource).toContain("listAgents");
    expect(agentsPageSource).toContain("createAgent");
    expect(agentsPageSource).toContain("instructions");
    expect(agentsPageSource).toContain("allowed_tools");
    expect(agentsPageSource).toContain("capabilities");
    expect(agentsPageSource).toContain("max_iterations");
    expect(agentsPageSource).toContain("agent.enabled");
    expect(agentsPageSource).toContain("agent.source");
    expect(agentsPageSource).toContain("aria-selected");
  });

  it("uses shared loading and error states without inventing online behavior", () => {
    expect(agentsPageSource).toContain("ErrorState");
    expect(agentsPageSource).toContain("Skeleton");
    expect(agentsPageSource).not.toContain("在线");
    expect(agentsPageSource).not.toContain("健康");
    expect(agentsPageSource).not.toContain("删除 Agent");
  });
});
