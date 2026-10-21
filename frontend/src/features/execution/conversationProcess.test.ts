import { describe, expect, it } from "vitest";
import { projectConversationMessages } from "./conversationProcess";
import type { Conversation } from "../../types";
const conversation: Conversation = { id: "c", title: "test", created_at: 1, updated_at: 4, messages: [
  { id: "u", role: "user", content: "read", created_at: 1 },
  { id: "call", role: "assistant", content: "", created_at: 2, tool_calls: JSON.stringify([{ id: "tool", function: { name: "read_file", arguments: '{"path":"a.txt"}' } }]) as unknown as never },
  { id: "result", role: "tool", content: "actual file", tool_call_id: "tool", created_at: 2 },
  { id: "answer", role: "assistant", content: "answer", created_at: 3 },
], message_reasoning: { call: "real provider thinking" }, execution_history: [{ toolCallId: "tool", name: "read_file", args: { path: "a.txt" }, executionStatus: "succeeded", approvalStatus: "not_required", verificationStatus: "passed", sequence: 1, result: "actual file" }] };
describe("conversation process projection", () => {
  it("restores actual reasoning and tool lifecycle onto the corresponding answer", () => {
    const messages = projectConversationMessages(conversation);
    expect(messages.map(message => message.id)).toEqual(["u", "answer"]);
    expect(messages[1].process?.reasoning).toBe("real provider thinking");
    expect(messages[1].process?.tools[0].execution?.status).toBe("success");
    expect(messages[1].process?.tools[0].result).toBe("actual file");
  });
  it("retains a reasoning and approval-only turn without inventing an answer", () => {
    const messages = projectConversationMessages({ ...conversation, messages: conversation.messages.slice(0, 2), turn_reasoning: { u: "partial provider stream" } });
    expect(messages[1].content).toBe("");
    expect(messages[1].process?.reasoning).toBe("partial provider stream");
    expect(messages[1].process?.tools[0].toolCallId).toBe("tool");
  });
  it("does not invent thinking or claim an unrecorded tool call succeeded", () => {
    const messages = projectConversationMessages({ ...conversation, message_reasoning: {}, execution_history: [] });
    expect(messages[1].process?.reasoning).toBeUndefined();
    expect(messages[1].process?.tools[0].execution).toBeUndefined();
    expect(messages[1].process?.tools[0].arguments).toBe('{"path":"a.txt"}');
  });
  it("does not expose stored system instructions as chat messages", () => {
    const messages = projectConversationMessages({ ...conversation, messages: [{ id: "system", role: "system", content: "private project instruction", created_at: 0 }, ...conversation.messages] });
    expect(messages.some(message => message.content.includes("private project instruction"))).toBe(false);
    expect(messages.find(message => message.id === "answer")?.process?.reasoning).toBe("real provider thinking");
  });
});
