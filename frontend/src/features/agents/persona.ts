import { managementRequest } from "../../api/productManagement";
export type ReplyVariation = "default" | "consistent" | "flexible" | "playful";
export interface GenerationOverrides {
  temperature?: number | null;
  top_p?: number | null;
  max_output_tokens?: number | null;
}
export interface GenerationOptions {
  model: string;
  temperature: boolean;
  top_p: boolean;
  max_output_tokens: number | null;
  default_temperature: number | null;
}
export interface Persona {
  display_name: string;
  description: string;
  tone: string;
  language: string;
  scenario: string;
  reply_examples: string;
  reply_variation: ReplyVariation;
  generation?: GenerationOverrides;
  avatar_resource_id: string | null;
}
export interface AgentProfile { user_prompt: string; persona_enabled: boolean; persona: Persona }
export const emptyProfile = (): AgentProfile => ({
  user_prompt: "",
  persona_enabled: false,
  persona: {
    display_name: "",
    description: "",
    tone: "",
    language: "",
    scenario: "",
    reply_examples: "",
    reply_variation: "default",
    avatar_resource_id: null,
  },
});
export const getAgentProfile = (id: string) => managementRequest<AgentProfile>("GET", `/api/agents/${encodeURIComponent(id)}/profile`);
export const saveAgentProfile = (id: string, profile: AgentProfile) => managementRequest<AgentProfile>("PUT", `/api/agents/${encodeURIComponent(id)}/profile`, profile);
export const resetAgentProfile = (id: string) => managementRequest<AgentProfile>("DELETE", `/api/agents/${encodeURIComponent(id)}/profile`);
export const getAgentGenerationOptions = (id: string) => managementRequest<GenerationOptions>("GET", `/api/agents/${encodeURIComponent(id)}/generation-options`);
