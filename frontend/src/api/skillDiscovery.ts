import { requestResult } from "../core/api/http";

export interface SkillDiscoverySource {
  id: string;
  label: string;
  path: string;
  kind: "configured" | "workspace" | "user" | "plugin";
  exists: boolean;
  skill_count: number;
  error: string | null;
}
export interface SkillDiscoveryReport {
  skill_count: number;
  sources: SkillDiscoverySource[];
}
export function getSkillDiscovery() {
  return requestResult<SkillDiscoveryReport>("GET", "/api/skills/discovery");
}
export function refreshSkillDiscovery() {
  return requestResult<SkillDiscoveryReport>("POST", "/api/skills/discovery");
}
