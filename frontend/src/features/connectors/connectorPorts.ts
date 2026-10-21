import type { ConnectorBinding, ConnectorNode } from "../../api/connectors";

export function connectorPort(kind: string): "top" | "bottom" | "left" | "right" {
  if (kind.startsWith("setting_")) return kind.slice(8) as "top"|"bottom"|"left"|"right";
  if (kind === "constraint") return "top";
  if (kind === "memory" || kind === "knowledge") return "bottom";
  if (kind === "conversation") return "left";
  return "right";
}

export function connectorBindingHandles(binding: ConnectorBinding) {
  if (binding.kind === "derived_setting_child") return { sourceHandle: "setting-out", targetHandle: "setting-in" };
  if (binding.kind === "derived_constraint") return { sourceHandle: "constraint-out", targetHandle: "constraint-in" };
  if (binding.kind === "conversation_agent") return { sourceHandle: "conversation-out", targetHandle: "conversation-in" };
  if (binding.kind === "agent_memory" || binding.kind === "agent_knowledge") return { sourceHandle: "context-out", targetHandle: "context-in" };
  return { sourceHandle: "capability-out", targetHandle: "capability-in" };
}

export function isConnectorConnection(connection: { source: string | null; target: string | null; sourceHandle?: string | null; targetHandle?: string | null }, nodes: ConnectorNode[]) {
  const source = nodes.find(node => node.id === connection.source);
  const target = nodes.find(node => node.id === connection.target);
  if (source?.kind === "conversation" && target?.kind === "agent") return connection.sourceHandle === "conversation-out" && connection.targetHandle === "conversation-in";
  if (source?.kind !== "agent" || !target || ["agent", "constraint", "conversation"].includes(target.kind)) return false;
  return connectorPort(target.kind) === "bottom"
    ? connection.sourceHandle === "context-out" && connection.targetHandle === "context-in"
    : connection.sourceHandle === "capability-out" && connection.targetHandle === "capability-in";
}
