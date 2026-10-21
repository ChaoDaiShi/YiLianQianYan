import type { ConnectorBinding, ConnectorResource } from "../../api/connectors";
export function bindingForResource(agentId: string, resource: ConnectorResource): Omit<ConnectorBinding, "id"> {
 const agent = `agent:${agentId}`;
 if (resource.kind === "conversation") return { kind: "conversation_agent", source_id: resource.id, target_id: agent };
 if (resource.kind === "model") { if (!resource.model_source) throw new Error("模型来源未指定。"); return { kind: "agent_model", source_id: agent, target_id: resource.id, service_kind: resource.service_kind ?? "chat", model_source: resource.model_source }; }
 if (resource.kind === "memory" || resource.kind === "knowledge") return { kind: resource.kind === "memory" ? "agent_memory" : "agent_knowledge", source_id: agent, target_id: resource.id };
 if (["skill", "workflow", "mcp", "tool", "plugin"].includes(resource.kind)) return { kind: "agent_capability", source_id: agent, target_id: resource.id };
 throw new Error("该节点不能作为资源绑定。");
}
