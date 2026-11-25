import { requestResult } from "../core/api/http";

export type HarnessLevel = "system" | "user";
export interface HarnessToolGroup { id: string; name: string; level: HarnessLevel; tools: string[] }
export interface HarnessProfile {
  id: string;
  name: string;
  description?: string;
  level: HarnessLevel;
  tool_group_ids: string[];
  skill_ids?: string[] | null;
  workflow_ids: string[];
  plugin_ids: string[];
  agent_id: string | null;
}
export interface HarnessSettings { revision: number; active_id: string; groups: HarnessToolGroup[]; profiles: HarnessProfile[] }
export interface HarnessItem { id: string; name: string }
export interface HarnessSkill extends HarnessItem { plugin_id: string | null }
export interface HarnessWorkflow extends HarnessItem { description: string; plugin_id: string | null; ready: boolean; tool_ids: string[] }
export interface HarnessAgent extends HarnessItem { plugin_id: string | null; allowed_tools: string[] }
export interface HarnessSnapshot {
  settings: HarnessSettings;
  system_groups: HarnessToolGroup[];
  catalog: {
    tools: HarnessItem[];
    plugins: HarnessItem[];
    skills: HarnessSkill[];
    workflows: HarnessWorkflow[];
    agents: HarnessAgent[];
  };
}
export const getHarnesses = () => requestResult<HarnessSnapshot>("GET", "/api/harnesses");
export const saveHarnesses = (settings: HarnessSettings) => requestResult<HarnessSettings>("PUT", "/api/harnesses", settings);
export const activateHarness = (id: string, revision: number) => requestResult<HarnessSettings>("POST", "/api/harnesses/active", { id, revision });
