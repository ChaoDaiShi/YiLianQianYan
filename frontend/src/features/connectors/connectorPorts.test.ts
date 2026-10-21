import { describe, expect, it } from "vitest";
import { connectorBindingHandles, connectorPort, isConnectorConnection } from "./connectorPorts";
const binding = (kind: string) => ({ id: kind, kind, source_id: "source", target_id: "target" });
describe("connector semantic ports", () => {
  it("groups constraints above, context below, conversation left and capabilities right", () => {
    expect(connectorPort("constraint")).toBe("top");
    expect(["memory", "knowledge"].map(connectorPort)).toEqual(["bottom", "bottom"]);
    expect(connectorPort("conversation")).toBe("left");
    expect(["tool", "model", "skill", "plugin", "workflow", "mcp"].map(connectorPort)).toEqual(Array(6).fill("right"));
  });
  it("uses corresponding source and target ports for persisted bindings", () => {
    expect(connectorBindingHandles(binding("derived_setting_child"))).toEqual({ sourceHandle:"setting-out", targetHandle:"setting-in" });
    expect(connectorBindingHandles(binding("derived_constraint"))).toEqual({ sourceHandle: "constraint-out", targetHandle: "constraint-in" });
    expect(connectorBindingHandles(binding("conversation_agent"))).toEqual({ sourceHandle: "conversation-out", targetHandle: "conversation-in" });
    for (const kind of ["agent_memory", "agent_knowledge"]) expect(connectorBindingHandles(binding(kind))).toEqual({ sourceHandle: "context-out", targetHandle: "context-in" });
    for (const kind of ["agent_model", "agent_capability"]) expect(connectorBindingHandles(binding(kind))).toEqual({ sourceHandle: "capability-out", targetHandle: "capability-in" });
  });
});


describe("connector drag routing", () => {
  const nodes = ["agent", "conversation", "memory", "tool", "constraint"].map(kind => ({ id: kind, kind, entity_id: kind, label: kind, x: 0, y: 0 }));
  it("accepts conversation, context and capability connections only through matching ports", () => {
    expect(isConnectorConnection({source:"conversation",target:"agent",sourceHandle:"conversation-out",targetHandle:"conversation-in"},nodes)).toBe(true);
    expect(isConnectorConnection({source:"agent",target:"memory",sourceHandle:"context-out",targetHandle:"context-in"},nodes)).toBe(true);
    expect(isConnectorConnection({source:"agent",target:"tool",sourceHandle:"capability-out",targetHandle:"capability-in"},nodes)).toBe(true);
    expect(isConnectorConnection({source:"agent",target:"tool",sourceHandle:"context-out",targetHandle:"capability-in"},nodes)).toBe(false);
    expect(isConnectorConnection({source:"tool",target:"memory",sourceHandle:"capability-out",targetHandle:"context-in"},nodes)).toBe(false);
    expect(isConnectorConnection({source:"agent",target:"constraint",sourceHandle:"capability-out",targetHandle:"constraint-in"},nodes)).toBe(false);
  });
});
