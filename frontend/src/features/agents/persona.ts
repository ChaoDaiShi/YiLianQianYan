import { managementRequest } from "../../api/productManagement";
export interface Persona { display_name: string; description: string; tone: string; language: string; avatar_resource_id: string | null }
export interface AgentProfile { user_prompt: string; persona_enabled: boolean; persona: Persona }
export const emptyProfile = (): AgentProfile => ({ user_prompt: "", persona_enabled: false, persona: { display_name: "", description: "", tone: "", language: "", avatar_resource_id: null } });
export const getAgentProfile = (id: string) => managementRequest<AgentProfile>("GET", `/api/agents/${encodeURIComponent(id)}/profile`);
export const saveAgentProfile = (id: string, profile: AgentProfile) => managementRequest<AgentProfile>("PUT", `/api/agents/${encodeURIComponent(id)}/profile`, profile);
