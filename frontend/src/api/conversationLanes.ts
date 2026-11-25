import { requestResult } from "../core/api/http";
import type { ConversationRunStatus } from "../types";

export interface ConversationLane {
  id: string;
  title: string;
  kind: "chat" | "work";
  run_status: ConversationRunStatus;
}

const path = (id: string) => `/api/conversations/${encodeURIComponent(id)}/lanes`;

export async function listConversationLanes(id: string): Promise<ConversationLane[]> {
  const result = await requestResult<ConversationLane[]>("GET", path(id));
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export async function createConversationLane(id: string, title: string, kind: "auto" | ConversationLane["kind"]): Promise<ConversationLane> {
  const result = await requestResult<ConversationLane>("POST", path(id), { title, kind });
  if (!result.ok) throw new Error(result.error);
  return result.data;
}
