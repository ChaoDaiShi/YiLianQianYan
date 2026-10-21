import type { Conversation, Message, ProcessToolCall } from "../../types";
import { hydratePersistedExecutionRecords, toToolCallRecords } from "./reducer";

/** Projects original protocol rows; source messages and tool results remain intact in storage. */
export function projectConversationMessages(conversation: Conversation): Message[] {
  const execution = new Map(toToolCallRecords(hydratePersistedExecutionRecords(conversation.execution_history, conversation.id)).map(record => [record.toolCallId, record]));
  const output: Message[] = [];
  let group: Message[] = [];
  function flush() {
    if (!group.length) return;
    const user = group.find(message => message.role === "user");
    const calls = new Map<string, ProcessToolCall>();
    const visible: Message[] = group.filter(message => message.content.trim().length > 0 && (message.role === "user" || message.role === "assistant" || message.role === "tool" && message.content.startsWith("data:image/"))).map(message => ({ ...message, tool_calls: undefined }));
    for (const message of group) {
      const raw = message.tool_calls as unknown;
      const tools = typeof raw === "string" ? JSON.parse(raw) : raw;
      if (!Array.isArray(tools)) continue;
      for (const tool of tools) {
        const id = tool.id || tool.toolCallId;
        if (!id) continue;
        const record = execution.get(id) || (tool.toolCallId ? tool : undefined);
        const result = group.find(row => row.role === "tool" && row.tool_call_id === id)?.content;
        calls.set(id, { toolCallId: id, name: tool.function?.name || tool.name, arguments: tool.function?.arguments,
          args: record?.args, result: record?.result ?? result, execution: record });
      }
    }
    const reasoning = user && Object.prototype.hasOwnProperty.call(conversation.turn_reasoning || {}, user.id)
      ? conversation.turn_reasoning![user.id]
      : group.map(message => conversation.message_reasoning?.[message.id]).filter(Boolean).join("\n\n");
    if (reasoning || calls.size) {
      let target = [...visible].reverse().find(message => message.role === "assistant");
      if (!target) { target = { id: group.find(message => message.role === "assistant")?.id || `process:${user?.id || group[0].id}`, role: "assistant", content: "", created_at: group[group.length - 1].created_at }; visible.push(target); }
      target.process = { reasoning: reasoning || undefined, tools: [...calls.values()] };
    }
    output.push(...visible); group = [];
  }
  for (const message of conversation.messages) {
    if (message.role === "user") flush();
    group.push(message);
  }
  flush();
  return output;
}
