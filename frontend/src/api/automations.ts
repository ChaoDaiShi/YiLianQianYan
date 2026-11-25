import { requestResult } from "../core/api/http";
import { API_BASE } from "./client";
import { controlSessionHeaders } from "./controlSession";

export type WatchSource =
  | { kind: "file"; path: string }
  | { kind: "http"; url: string; send_preview: boolean };
export interface WatchSchedule {
  kind: "watch"; source: WatchSource; min_interval_secs: number;
  expires_at: number; max_checks: number; max_retries: number;
}
export interface AttentionState {
  status: "waiting" | "checking" | "deferred" | "unchanged" | "action_queued" | "proposed" | "conflict" | "blocked" | "unknown" | "stopped";
  reason: string; decision: "defer" | "stop" | "act" | "propose" | null;
  checks_used: number; retries_used: number; last_checked_at: number | null;
  last_observed_at: number | null; observation_id: string | null;
  proposal: { goal: string; observation_id: string } | null;
}
export interface SourceObservation {
  id: string; automation_id: string; automation_revision: number; workspace_id: string;
  source_kind: "file" | "http"; source: string; authority: string;
  source_at: number | null; observed_at: number; fingerprint: string | null;
  status: "captured" | "conflict" | "unknown"; previous_observation_id: string | null;
  http_status?: number;
}
export type AutomationSchedule =
  | { kind: "file_changed"; path: string }
  | WatchSchedule
  | { kind: "once"; local_at: string }
  | { kind: "daily"; time: string }
  | { kind: "weekly"; time: string; weekdays: number[] };
export interface AutomationInput {
  expected_revision?: number;
  name: string; workflow_graph_id: string; workspace_id: string;
  target?: { kind: "workflow"; graph_id: string } | { kind: "agent_turn"; mission_id: string }
    | { kind: "personal_assistant"; goal: string; conversation_id?: string; root_path?: string; allow_file_writes: boolean; allow_file_discovery?: boolean; max_runs: number };
  timezone: string; schedule: AutomationSchedule; enabled: boolean;
}
export interface Automation extends AutomationInput {
  attention?: AttentionState;
  archived?: boolean;
  id: string; revision: number; next_run_at: number | null; created_at: number; updated_at: number;
  assistant_activity?: { latest: AutomationOccurrence | null; used_runs: number; source_error?: string | null };
  runtime?: AutomationRuntime;
}
export interface AutomationRuntime {
  phase: "suspended" | "waking" | "running" | "waiting_approval" | "waiting_input";
  latest_status: string | null;
  last_woke_at: number | null;
  trigger: string | null;
}
export interface PersonalIssue { code: string; message: string; severity: "error" | "warning" }
export interface PersonalExecution {
  tool_call_id: string; name: string; path: string | null; arguments_preview: string;
  execution_status: string; approval_status: string; verification_status: string;
  verification_reason: string | null; result: string | null; started_at: number | null; finished_at: number | null;
}
export interface PersonalFile { tool_call_id: string; path: string; relative_path: string; execution_status: string; verification_status: string; created_at: number | null }
export interface PersonalManagement {
  automation: Automation;
  activity: NonNullable<Automation["assistant_activity"]>;
  actions: { can_start: boolean; can_edit: boolean; can_stop: boolean; requires_review: boolean; review_occurrence_id: string | null };
  readiness: { issues: PersonalIssue[]; can_start: boolean };
  conversation: { id: string; run_status: string; run_error: string | null };
  executions: PersonalExecution[];
  pending_inputs: { question_id: string; title: string; status: string }[];
  pending_approvals: PersonalExecution[];
  files: PersonalFile[]; history_total: number;
}
export interface PersonalHistoryPage { occurrences: AutomationOccurrence[]; total: number; has_more: boolean; next_cursor: string | null }
export interface PersonalStartInput { expected_revision: number; request_id: string; reviewed_occurrence_id?: string; acknowledged?: boolean }
export interface AutomationOccurrence {
  id: string; automation_id: string; automation_revision: number; timezone: string; occurrence_key: string;
  trigger: "manual" | "scheduled" | "file_changed" | "attention"; scheduled_at: number;
  workspace_id: string; workflow_graph_id: string; run_id: string | null;
  robot_run_id?: string | null;
  robot_channel_id?: string | null;
  conversation_id?: string | null;
  assistant_turn_id?: string | null;
  report?: string | null;
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
export const getAutomationObservations = (id: string) => call<{ observations: SourceObservation[] }>("GET", `${path(id)}/observations`);
export const stopAutomation = (id: string) => call<{ automation: Automation }>("POST", `${path(id)}/stop`);
export const stopPersonalAssistant = stopAutomation;
export const getPersonalManagement = (id: string) => call<PersonalManagement>("GET", `${path(id)}/personal`);
export async function startPersonalAssistant(id: string, input: PersonalStartInput): Promise<{ occurrence: AutomationOccurrence }> {
  const response = await fetch(`${API_BASE}${path(id)}/personal/start`, { method: "POST", headers: { "Content-Type": "application/json", ...controlSessionHeaders() }, body: JSON.stringify(input) });
  const value = await response.json().catch(() => null);
  if (!response.ok) {
    const error = new Error(typeof value?.error === "string" ? value.error : `启动请求失败（HTTP ${response.status}），请先核对上次请求结果`) as Error & { definitiveRejected?: boolean };
    error.definitiveRejected = response.status >= 400 && response.status < 500 && value?.request_id === input.request_id && value?.definitive_rejected === true;
    throw error;
  }
  if (!value?.occurrence) throw new Error("启动响应不完整，请核对上次请求结果");
  return value;
}
export const archivePersonalAssistant = (id: string) => call<{ automation: Automation }>("POST", `${path(id)}/personal/archive`);
export const restorePersonalAssistant = (id: string, expected_revision: number) => call<{ automation: Automation }>("POST", `${path(id)}/personal/restore`, { expected_revision });
export const getPersonalHistory = (id: string, cursor?: string, status?: string) => call<PersonalHistoryPage>("GET", `${path(id)}/personal/history?${new URLSearchParams({ ...(cursor ? { cursor } : {}), ...(status ? { status } : {}) }).toString()}`);
export const previewPersonalFile = (id: string, toolCallId: string) => call<{ path: string; content: string; size: number; truncated: boolean }>("GET", `${path(id)}/personal/file?tool_call_id=${encodeURIComponent(toolCallId)}`);
