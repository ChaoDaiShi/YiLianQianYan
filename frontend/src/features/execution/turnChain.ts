import type { ConversationTurn } from "../../api/contextManifest";
import type { Message } from "../../types";
import type { PendingApproval } from "../../types/approval";
import type { ExecutionRecord } from "./model";

function callIds(message: Message): string[] {
  const ids = message.tool_call_id ? [message.tool_call_id] : [];
  let calls: unknown = message.tool_calls;
  if (typeof calls === "string") {
    try { calls = JSON.parse(calls); } catch { calls = []; }
  }
  if (Array.isArray(calls)) {
    for (const call of calls) {
      if (typeof call?.id === "string") ids.push(call.id);
      else if (typeof call?.toolCallId === "string") ids.push(call.toolCallId);
    }
  }
  for (const tool of message.process?.tools ?? []) ids.push(tool.toolCallId);
  return ids;
}

export function projectTurnChain(turn: ConversationTurn, messages: Message[], history: ExecutionRecord[], persisted: Message[] = [], approvals: PendingApproval[] = []) {
  const boundary = messages.findIndex(message => message.id === turn.user_message_id);
  if (boundary < 0) return null;
  const following = messages.slice(boundary + 1);
  const nextUser = following.findIndex(message => message.role === "user");
  const turnMessages = nextUser < 0 ? following : following.slice(0, nextUser);
  const currentCalls = new Set(turnMessages.flatMap(callIds));
  const otherCalls = new Set([...messages.slice(0, boundary), ...following.slice(turnMessages.length)].flatMap(callIds));
  const persistedBoundary = persisted.findIndex(message => message.id === turn.user_message_id);
  let lastSavedMessage: Message | undefined;
  if (persistedBoundary >= 0) {
    let inside = false;
    persisted.forEach((message, index) => {
      if (index === persistedBoundary) { inside = true; return; }
      if (index > persistedBoundary && message.role === "user") inside = false;
      if (inside) lastSavedMessage = message;
      for (const id of callIds(message)) (inside ? currentCalls : otherCalls).add(id);
    });
  }
  for (const approval of approvals) {
    const timestamp = Date.parse(approval.created_at);
    if (!otherCalls.has(approval.tool_call_id) && timestamp >= turn.started_at && (turn.finished_at == null || timestamp <= turn.finished_at)) {
      currentCalls.add(approval.tool_call_id);
    }
  }
  const actions = history.filter(record => currentCalls.has(record.toolCallId) || (
    !otherCalls.has(record.toolCallId) && record.startedAt != null && record.startedAt >= turn.started_at &&
    (turn.finished_at == null || record.startedAt <= turn.finished_at)
  ));
  const savedReply = turn.status === "completed" && lastSavedMessage?.role === "assistant" &&
    lastSavedMessage.content.trim() && !callIds(lastSavedMessage).length ? lastSavedMessage : undefined;
  return {
    request: messages[boundary],
    response: [...turnMessages].reverse().find(message => message.role === "assistant" && Boolean(message.content.trim())),
    savedReply,
    actions,
    pending: actions.filter(action => action.approvalStatus === "pending").length,
    approved: actions.filter(action => action.approvalStatus === "approved").length,
    rejected: actions.filter(action => action.approvalStatus === "rejected").length,
    verified: actions.filter(action => action.verificationStatus === "passed").length,
  };
}
