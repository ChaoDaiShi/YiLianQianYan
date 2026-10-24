import { requestResult } from "../core/api/http";

export type RobotPeerType = "private" | "group";
export type RobotActionPeerType = RobotPeerType | "account" | "friends" | "groups" | "recent" | "message";
export type RobotSendMode = "automatic" | "manual";
export type RobotRunStatus = "queued" | "running" | "awaiting_approval" | "completed" | "stopped" | "failed";

export interface RobotPolicy {
  channel_id: string;
  account_id: string;
  enabled: boolean;
  owner_ids: string[];
  read_friends: boolean;
  read_groups: boolean;
  send_mode: RobotSendMode;
  send_private_ids: string[];
  send_group_ids: string[];
  send_all_private: boolean;
  send_all_groups: boolean;
  updated_at: number;
}

export type RobotPolicyInput = Omit<RobotPolicy, "channel_id" | "account_id" | "updated_at">;
export interface RobotDirectoryItem { id: string; name: string; peer_type: RobotPeerType }
export type RobotDirectoryKind = "friends" | "groups" | "recent";
export interface RobotHistoryInput { peer_type: RobotPeerType; peer_id: string; count: number }
export interface RobotHistoryResult { data: unknown }
export interface RobotSendInput { request_id: string; account_id: string; peer_type: RobotPeerType; peer_id: string; text: string; approved: boolean }

export interface RobotAction {
  id: string;
  run_id: string | null;
  channel_id: string;
  account_id: string;
  tool_name: string;
  peer_type: RobotActionPeerType | null;
  peer_id: string | null;
  arguments: unknown;
  result: unknown | null;
  status: string;
  platform_message_id: string | null;
  error: string | null;
  created_at: number;
  updated_at: number;
}

export interface RobotMission {
  id: string;
  channel_id: string;
  name: string;
  goal: string;
  preset_id: string | null;
  workspace_id: string | null;
  created_at: number;
  updated_at: number;
}
export type RobotMissionInput = Pick<RobotMission, "name" | "goal" | "preset_id" | "workspace_id">;

export interface RobotRun {
  account_id: string;
  id: string;
  channel_id: string;
  mission_id: string | null;
  conversation_id: string;
  user_message_id: string | null;
  origin: "desktop" | "owner" | "automation" | string;
  owner_id: string | null;
  goal: string;
  preset_id: string | null;
  status: RobotRunStatus | string;
  answer: string | null;
  error: string | null;
  created_at: number;
  updated_at: number;
}
export interface RobotRunDetails { run: RobotRun; actions: RobotAction[] }

export type RobotAutomationSchedule =
  | { kind: "once"; local_at: string }
  | { kind: "daily"; time: string }
  | { kind: "weekly"; time: string; weekdays: number[] };
export type RobotAutomationTarget = { kind: "workflow"; graph_id: string } | { kind: "agent_turn"; mission_id: string };
export interface RobotAutomationInput {
  expected_revision: null;
  name: string;
  workflow_graph_id: "";
  target: { kind: "agent_turn"; mission_id: string };
  workspace_id: string;
  timezone: string;
  schedule: RobotAutomationSchedule;
  enabled: true;
}
export interface RobotAutomation {
  id: string;
  revision: number;
  name: string;
  workflow_graph_id: string;
  target: RobotAutomationTarget | null;
  workspace_id: string;
  timezone: string;
  schedule: RobotAutomationSchedule;
  enabled: boolean;
  next_run_at: number | null;
  created_at: number;
  updated_at: number;
}

async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const result = await requestResult<T>(method, path, body);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

const robotPath = (channelId: string) => `/api/channels/${encodeURIComponent(channelId)}/robot`;
const missionPath = (channelId: string, missionId: string) => `${robotPath(channelId)}/missions/${encodeURIComponent(missionId)}`;
const runPath = (channelId: string, runId: string) => `${robotPath(channelId)}/runs/${encodeURIComponent(runId)}`;

export const getRobotPolicy = (channelId: string) => call<{ policy: RobotPolicy }>("GET", `${robotPath(channelId)}/policy`);
export const updateRobotPolicy = (channelId: string, policy: RobotPolicyInput) => call<{ policy: RobotPolicy }>("PUT", `${robotPath(channelId)}/policy`, policy);
export const listRobotDirectory = (channelId: string, kind: RobotDirectoryKind) => call<{ items: RobotDirectoryItem[] }>("GET", `${robotPath(channelId)}/directory?kind=${kind}`);
export const readRobotHistory = (channelId: string, input: RobotHistoryInput) => call<RobotHistoryResult>("POST", `${robotPath(channelId)}/history`, input);
export const sendRobotMessage = (channelId: string, input: RobotSendInput) => call<{ action: RobotAction }>("POST", `${robotPath(channelId)}/send`, input);
export const listRobotMissions = (channelId: string) => call<{ missions: RobotMission[] }>("GET", `${robotPath(channelId)}/missions`);
export const createRobotMission = (channelId: string, input: RobotMissionInput) => call<{ mission: RobotMission }>("POST", `${robotPath(channelId)}/missions`, input);
export const updateRobotMission = (channelId: string, missionId: string, input: RobotMissionInput) => call<{ mission: RobotMission }>("PUT", missionPath(channelId, missionId), input);
export const deleteRobotMission = (channelId: string, missionId: string) => call<{ deleted: boolean }>("DELETE", missionPath(channelId, missionId));
export const startRobotMission = (channelId: string, missionId: string) => call<{ run: RobotRun }>("POST", `${missionPath(channelId, missionId)}/run`);
export const listRobotRuns = (channelId: string) => call<{ runs: RobotRun[] }>("GET", `${robotPath(channelId)}/runs`);
export const listRobotSends = (channelId: string) => call<{ actions: RobotAction[] }>("GET", `${robotPath(channelId)}/sends`);
export const getRobotRun = (channelId: string, runId: string) => call<RobotRunDetails>("GET", runPath(channelId, runId));
export const stopRobotRun = (channelId: string, runId: string) => call<{ run: RobotRun }>("POST", `${runPath(channelId, runId)}/stop`);
export const createRobotAutomation = (input: RobotAutomationInput) => call<{ automation: RobotAutomation }>("POST", "/api/automations", input);
export const listRobotAutomations = () => call<{ automations: RobotAutomation[] }>("GET", "/api/automations");
export const setRobotAutomationEnabled = (id: string, enabled: boolean, expectedRevision: number) =>
  call<{ automation: RobotAutomation }>("POST", `/api/automations/${encodeURIComponent(id)}/enabled`, { enabled, expected_revision: expectedRevision });
