import type { CapabilityDescriptor } from "../../api/client";

export interface CapabilityManagementTarget {
  label: string;
  to: string;
}

export function getCapabilityManagementTarget(
  capability: CapabilityDescriptor,
): CapabilityManagementTarget | null {
  // MCP descriptor source IDs identify tools, resources or prompts rather than the server.
  if (capability.provider === "mcp") {
    return { label: "管理 MCP 服务", to: "/capabilities?category=mcp" };
  }
  if (capability.provider === "skill_runtime") {
    return { label: "管理技能来源", to: `/capabilities?category=skill&skill=${encodeURIComponent(capability.name)}` };
  }
  const sourceId = capability.metadata.source_id?.trim();
  if (!sourceId) return null;
  const encoded = encodeURIComponent(sourceId);

  switch (capability.provider) {
    case "builtin":
      return capability.kind === "tool" ? { label: "查看运行时工具", to: `/capabilities?category=tool&tool=${encoded}` } : null;
    case "agent_runtime":
      return { label: "管理智能体来源", to: `/agents?agent=${encoded}` };
    case "workflow_runtime":
      return { label: "管理工作流来源", to: `/capabilities?category=workflow&workflow=${encoded}` };
    default:
      return null;
  }
}
