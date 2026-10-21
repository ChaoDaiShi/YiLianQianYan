// Plugins — MCP servers, subagents and skills
//
// Transport is `core/api/http.ts`; this module owns these calls rather than
// borrowing them from the legacy bundle.
import { request } from "../core/api/http";
import { requestResult } from "../core/api/http";

// ── Skills ──
export interface SkillCompatibility {
  status: "portable" | "adapted" | "external_host";
  runtime_ready: boolean;
  required_tools: string[];
  missing_tools: string[];
  tool_mappings: { from: string; to: string; instructions: string }[];
  reason: string;
}

export interface SkillSummary {
  name: string;
  description: string;
  path: string;
  editable: boolean;
  display_name?: string;
  source_id?: string;
  source?: string;
  source_kind?: string;
  source_root?: string;
  compatibility?: SkillCompatibility;
}

export interface SkillDetail extends SkillSummary {
  content: string;
  root_dir: string;
}

export async function listSkills() {
  return requestResult<SkillSummary[]>("GET", "/api/skills");
}

export async function loadSkill(name: string) {
  return requestResult<SkillDetail>("GET", `/api/skills/${encodeURIComponent(name)}`);
}

export async function createSkill(data: { name: string; content: string }) {
  return requestResult<SkillSummary>("POST", "/api/skills", data);
}

export async function updateSkill(
  currentName: string,
  data: { name: string; content: string }
) {
  return requestResult<SkillSummary>(
    "PUT",
    `/api/skills/${encodeURIComponent(currentName)}`,
    data
  );
}

export async function deleteSkill(name: string) {
  return requestResult<{ status: string }>(
    "DELETE",
    `/api/skills/${encodeURIComponent(name)}`
  );
}

// ── Plugins / MCP ──

export interface McpServer {
  id: string;
  name: string;
  transport: string;
  command?: string | null;
  args?: string[];
  url?: string | null;
  env?: Record<string, string>;
  enabled: boolean;
  created_at: number;
  updated_at: number;
  runtime_status?: string;
  protocol_version?: string | null;
  tools_count?: number;
  resources_count?: number;
  resource_templates_count?: number;
  prompts_count?: number;
  last_refresh?: number | null;
  safe_error?: string | null;
}

export interface PluginListResponse {
  builtin: Array<{ name: string; description: string; parameters: Record<string, unknown> }>;
  mcp: McpServer[];
  mcp_runtime_ready: boolean;
}

export async function listPlugins() {
  return request<PluginListResponse>("GET", "/api/plugins");
}

export async function createMcpServer(data: Partial<McpServer>) {
  return requestResult<McpServer>("POST", "/api/plugins/mcp", data);
}

export async function updateMcpServer(id: string, data: Partial<McpServer>) {
  return requestResult<McpServer>("PUT", `/api/plugins/mcp/${id}`, data);
}

export async function deleteMcpServer(id: string) {
  return requestResult<{ status: string }>("DELETE", `/api/plugins/mcp/${id}`);
}

export async function toggleMcpServer(id: string) {
  return requestResult<McpServer>("POST", `/api/plugins/mcp/${id}/toggle`);
}

export async function testMcpServer(id: string) {
  return requestResult<{ ok: boolean; message: string }>("POST", `/api/plugins/mcp/${id}/test`);
}

export interface SubagentMetadata {
  name: string;
  description: string;
  allowed_tools: string[];
  model?: string | null;
  workdir?: string | null;
  runtime_ready: boolean;
}

export async function listSubagents() {
  return request<SubagentMetadata[]>("GET", "/api/subagents");
}

export interface PluginFeatureEntry {
  plugin_id: string;
  plugin_name: string;
  id: string;
  title: string;
  description: string;
  resource_id: string | null;
  target: { kind: "skill" | "workflow" | "agent"; contribution: string };
  content: string;
}
export async function listPluginFeatures(): Promise<PluginFeatureEntry[]> {
  const result = await requestResult<{ features: PluginFeatureEntry[] }>("GET", "/api/plugins/features");
  if (!result.ok) throw new Error(result.error);
  return result.data.features;
}
