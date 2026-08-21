import type { CapabilityDescriptor } from "../../api/client";

export interface CapabilityManagementTarget {
  label: string;
  to: string;
}

export function getCapabilityManagementTarget(
  capability: CapabilityDescriptor,
): CapabilityManagementTarget | null {
  const sourceId = capability.metadata.source_id?.trim();
  if (!sourceId) return null;
  const encoded = encodeURIComponent(sourceId);

  switch (capability.provider) {
    case "skill_runtime":
      return { label: "管理技能来源", to: `/skills?skill=${encoded}` };
    case "mcp":
      return { label: "管理 MCP 服务", to: `/plugins?server=${encoded}` };
    case "agent_runtime":
      return { label: "管理智能体来源", to: `/agents?agent=${encoded}` };
    case "workflow_runtime":
      return { label: "管理工作流来源", to: `/workflows?workflow=${encoded}` };
    default:
      return null;
  }
}
