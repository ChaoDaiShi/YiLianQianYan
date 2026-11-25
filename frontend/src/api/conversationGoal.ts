import { requestProofLoop } from "./proofloop";

export interface ConversationGoalBinding {
  goal_id: string;
  goal_version: number;
  graph_id: string;
  workspace_id: string;
}

export interface ConversationGoalResponse { binding: ConversationGoalBinding | null; restricted?: boolean }

export const getConversationGoal = (id: string, signal?: AbortSignal) =>
  requestProofLoop<ConversationGoalResponse>("GET", `/api/conversations/${encodeURIComponent(id)}/goal`, undefined, signal);

export const bindConversationGoal = (id: string, graphId: string) =>
  requestProofLoop<ConversationGoalResponse>("PUT", `/api/conversations/${encodeURIComponent(id)}/goal`, { graph_id: graphId });
