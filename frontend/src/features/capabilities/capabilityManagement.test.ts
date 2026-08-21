import { describe, expect, it } from "vitest";
import type { CapabilityDescriptor } from "../../api/client";
import { getCapabilityManagementTarget } from "./capabilityManagement";

function capability(overrides: Partial<CapabilityDescriptor>): CapabilityDescriptor {
  return {
    id: "skill:organize-files",
    kind: "skill",
    provider: "skill_runtime",
    name: "整理文件",
    description: "整理工作区文件",
    risk: "low",
    permissions: [],
    status: "ready",
    enabled: true,
    metadata: {
      source_id: "organize-files",
      source_name: "organize-files",
      tags: [],
      runtime_ready: true,
      extra: {},
    },
    ...overrides,
  };
}

describe("capability source management", () => {
  it("routes editable providers to their real management surface", () => {
    expect(getCapabilityManagementTarget(capability({ provider: "skill_runtime" }))).toEqual({
      label: "管理技能来源",
      to: "/skills?skill=organize-files",
    });
    expect(getCapabilityManagementTarget(capability({ provider: "mcp", metadata: { source_id: "local fs", source_name: null, tags: [], runtime_ready: true, extra: {} } }))).toEqual({
      label: "管理 MCP 服务",
      to: "/plugins?server=local%20fs",
    });
    expect(getCapabilityManagementTarget(capability({ provider: "agent_runtime", metadata: { source_id: "agent-1", source_name: null, tags: [], runtime_ready: true, extra: {} } }))).toEqual({ label: "管理智能体来源", to: "/agents?agent=agent-1" });
    expect(getCapabilityManagementTarget(capability({ provider: "workflow_runtime", metadata: { source_id: "flow-1", source_name: null, tags: [], runtime_ready: true, extra: {} } }))).toEqual({ label: "管理工作流来源", to: "/workflows?workflow=flow-1" });
  });

  it("does not pretend builtin capabilities are directly editable", () => {
    expect(getCapabilityManagementTarget(capability({ provider: "builtin" }))).toBeNull();
    expect(getCapabilityManagementTarget(capability({ provider: "subagent" }))).toBeNull();
  });
});
