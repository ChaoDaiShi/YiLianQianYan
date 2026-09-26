// Unified Capability Registry — the read side of the capability catalogue.
//
// One descriptor shape covers tools, MCP tools, subagents, agents, workflows
// and skills, so a caller can present "everything this machine can do" without
// switching on the provider. `status` answers whether it is usable right now;
// `metadata.runtime_ready` is the provider's own narrower claim.
//
// `listCapabilities` fails through untouched when the backend refuses; a
// successful response carries `{capabilities, total}` because the registry is
// paginated.
import { requestResult } from "../core/api/http";

export type CapabilityKind =
  | "tool"
  | "mcp_tool"
  | "subagent"
  | "agent"
  | "workflow"
  | "skill";

export type CapabilityProviderKind =
  | "builtin"
  | "mcp"
  | "subagent"
  | "agent_runtime"
  | "workflow_runtime"
  | "skill_runtime";

export type CapabilityRuntimeStatus =
  | "ready"
  | "unavailable"
  | "disabled"
  | "misconfigured"
  | "degraded"
  | "unknown";

export type CapabilityRisk = "low" | "medium" | "high" | "dynamic";

export interface CapabilityPermission {
  permission: string;
  required: boolean;
}

export interface CapabilityMetadata {
  source_id?: string | null;
  source_name?: string | null;
  version?: string | null;
  tags: string[];
  runtime_ready: boolean;
  extra: Record<string, unknown>;
}

export interface CapabilityDescriptor {
  id: string;
  kind: CapabilityKind;
  provider: CapabilityProviderKind;
  name: string;
  description: string;
  input_schema?: Record<string, unknown> | null;
  risk: CapabilityRisk;
  permissions: CapabilityPermission[];
  status: CapabilityRuntimeStatus;
  enabled: boolean;
  metadata: CapabilityMetadata;
}

export interface CapabilityRefreshReport {
  discovered: number;
  ready: number;
  unavailable: number;
  duplicates: number;
  provider_failures: number;
}

export async function listCapabilities(query: {
  kind?: CapabilityKind;
  provider?: CapabilityProviderKind;
  status?: CapabilityRuntimeStatus;
  q?: string;
  limit?: number;
  offset?: number;
} = {}) {
  const params = new URLSearchParams();
  if (query.kind) params.set("kind", query.kind);
  if (query.provider) params.set("provider", query.provider);
  if (query.status) params.set("status", query.status);
  if (query.q) params.set("q", query.q);
  if (query.limit !== undefined) params.set("limit", String(query.limit));
  if (query.offset !== undefined) params.set("offset", String(query.offset));
  const qs = params.toString();
  const res = await requestResult<{ capabilities: CapabilityDescriptor[]; total: number }>(
    "GET",
    `/api/capabilities${qs ? `?${qs}` : ""}`
  );
  return res.ok ? ({ ok: true, data: res.data } as const) : res;
}

export async function getCapability(id: string) {
  return requestResult<CapabilityDescriptor>("GET", `/api/capabilities/${id}`);
}

export async function refreshCapabilities() {
  return requestResult<CapabilityRefreshReport>("POST", "/api/capabilities/refresh");
}