import { requestResult } from "../core/api/http";

export type AutomationSchedule =
  | { kind: "once"; local_at: string }
  | { kind: "daily"; time: string }
  | { kind: "weekly"; time: string; weekdays: number[] };
export interface AutomationInput {
  expected_revision?: number;
  name: string; workflow_graph_id: string; workspace_id: string;
  target?: { kind: "workflow"; graph_id: string } | { kind: "agent_turn"; mission_id: string };
  timezone: string; schedule: AutomationSchedule; enabled: boolean;
}
export interface Automation extends AutomationInput {
  id: string; revision: number; next_run_at: number | null; created_at: number; updated_at: number;
}
export interface AutomationOccurrence {
  id: string; automation_id: string; automation_revision: number; timezone: string; occurrence_key: string;
  trigger: "manual" | "scheduled"; scheduled_at: number;
  workspace_id: string; workflow_graph_id: string; run_id: string | null;
  robot_run_id?: string | null;
  robot_channel_id?: string | null;
  conversation_id?: string | null;
  status: string; error: string | null; created_at: number; updated_at: number;
}
async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const result = await requestResult<T>(method, path, body);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}
const path = (id: string) => `/api/automations/${encodeURIComponent(id)}`;
export const listAutomations = () => call<{ automations: Automation[] }>("GET", "/api/automations");
export const createAutomation = (input: AutomationInput) => call<{ automation: Automation }>("POST", "/api/automations", input);
export const updateAutomation = (id: string, input: AutomationInput) => call<{ automation: Automation }>("PUT", path(id), input);
export const deleteAutomation = (id: string) => call<{ deleted: boolean }>("DELETE", path(id));
export const setAutomationEnabled = (id: string, enabled: boolean, expected_revision: number) => call<{ automation: Automation }>("POST", `${path(id)}/enabled`, { enabled, expected_revision });
export const runAutomation = (id: string) => call<{ occurrence: AutomationOccurrence }>("POST", `${path(id)}/run`);
export const getAutomationHistory = (id: string) => call<{ occurrences: AutomationOccurrence[] }>("GET", `${path(id)}/history`);
