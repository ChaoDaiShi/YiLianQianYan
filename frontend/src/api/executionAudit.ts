import { requestResult } from "./transport";

export interface ExecutionAuditEvent {
  event_type: string;
  tool_call_id: string | null;
  decision_status: string | null;
  created_at: number;
  details?: { reason?: string };
}

export async function getTurnExecutionAudit(conversationId: string, start: number, end: number | null) {
  const query = new URLSearchParams({ conversation_id: conversationId, start_at: String(start), limit: "500" });
  if (end != null) query.set("end_at", String(end));
  const result = await requestResult<{ events: ExecutionAuditEvent[] }>("GET", `/api/security/audit?${query}`);
  return result.ok ? result.data.events : [];
}
