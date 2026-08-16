// ============================================================
// MCP Runtime API client — read-only runtime surface (Types + API).
//
// Mirrors the backend `/api/mcp/servers` read-only routes. There is
// intentionally NO tool-execution API here: MCP tools execute only through
// the Security Execution Gateway, never from the frontend.
// ============================================================

import { request } from "./client";

// ── Types (match backend serde output) ──

export type McpRuntimeStatus =
  | "disconnected"
  | "connecting"
  | "ready"
  | "degraded"
  | "unavailable"
  | "misconfigured"
  | "disabled";

export type McpProtocolVersion = "2025-11-25" | "2026-07-28";

export interface McpServerCapabilities {
  tools: boolean;
  resources: boolean;
  prompts: boolean;
  listChanged?: boolean;
  extensions?: unknown;
}

export interface McpServerRuntimeSummary {
  id: string;
  name: string;
  transport: string;
  protocol_version: McpProtocolVersion;
  status: McpRuntimeStatus;
  capabilities: McpServerCapabilities;
  tools_count: number;
  resources_count: number;
  resource_templates_count: number;
  prompts_count: number;
  last_refresh?: number | null;
  safe_error?: string | null;
}

export interface McpRuntimeTool {
  name: string;
  title?: string | null;
  description?: string | null;
  inputSchema?: Record<string, unknown> | null;
}

export interface McpResourceDescriptor {
  uri: string;
  name: string;
  title?: string | null;
  description?: string | null;
  mimeType?: string | null;
  size?: number | null;
  annotations?: unknown;
}

export interface McpResourceTemplate {
  uriTemplate: string;
  name: string;
  title?: string | null;
  description?: string | null;
  mimeType?: string | null;
}

/** Wire shape for a single resource content item (tagged `kind`). */
export type McpResourceContent =
  | { kind: "text"; uri: string; mime_type?: string | null; text: string }
  | { kind: "blob"; uri: string; mime_type?: string | null; blob_base64: string };

export interface McpInputRequired {
  prompt: string;
  inputSchema?: Record<string, unknown> | null;
}

export type McpResourceReadResponse =
  | { contents: McpResourceContent[] }
  | { input_required: McpInputRequired };

export interface McpPromptArgument {
  name: string;
  description?: string | null;
  required?: boolean;
}

export interface McpPromptDescriptor {
  name: string;
  title?: string | null;
  description?: string | null;
  arguments: McpPromptArgument[];
}

export type McpPromptMessageContent =
  | { type: "text"; text: string }
  | { type: "image"; data: string; mime_type: string }
  | { type: "audio"; data: string; mime_type: string }
  | { type: "resource_link"; uri: string; name: string }
  | { type: "embedded_resource"; resource: McpResourceContent };

export interface McpPromptMessage {
  role: "user" | "assistant";
  content: McpPromptMessageContent;
}

export interface McpPromptResult {
  description?: string | null;
  messages: McpPromptMessage[];
}

export type McpPromptGetResponse = McpPromptResult | { input_required: McpInputRequired };

// ── Labels / tones (pure) ──

export const MCP_RUNTIME_STATUS_LABELS: Record<McpRuntimeStatus, string> = {
  disconnected: "未连接",
  connecting: "连接中",
  ready: "就绪",
  degraded: "降级",
  unavailable: "不可用",
  misconfigured: "配置错误",
  disabled: "已禁用",
};

export type McpRuntimeTone = "default" | "success" | "warning" | "danger" | "info";

export const MCP_RUNTIME_STATUS_TONES: Record<McpRuntimeStatus, McpRuntimeTone> = {
  disconnected: "default",
  connecting: "info",
  ready: "success",
  degraded: "warning",
  unavailable: "danger",
  misconfigured: "danger",
  disabled: "default",
};

/**
 * Canonical transport label. Streamable HTTP (and its legacy `http`/`sse`
 * aliases) is a first-class, supported transport — no transport is described
 * as "not supported" anymore.
 */
export function mcpTransportRuntimeLabel(transport: string): string {
  switch (transport) {
    case "stdio":
      return "Stdio";
    case "streamable_http":
    case "http":
      return "Streamable HTTP";
    case "sse":
      return "SSE (legacy)";
    default:
      return transport || "未知";
  }
}

// ── Preview helpers (pure, display-bound, secret-safe) ──

export const EXTERNAL_PROMPT_WARNING = "来自外部 MCP Server 的内容，仅供预览。";

export const RESOURCE_TEXT_PREVIEW_MAX = 8000;

export type ResourcePreview =
  | { kind: "text"; text: string; truncated: boolean }
  | { kind: "blob"; mimeType: string | null; size: number };

/**
 * Reduce a resource content item to a safe, bounded preview.
 *
 * Text is length-capped for display. Blob content is NEVER surfaced: only the
 * MIME type and the encoded byte count are exposed — no base64, no decode, no
 * download, no execution.
 */
export function resourcePreview(content: McpResourceContent): ResourcePreview {
  if (content.kind === "blob") {
    return {
      kind: "blob",
      mimeType: content.mime_type ?? null,
      size: content.blob_base64.length,
    };
  }
  const truncated = content.text.length > RESOURCE_TEXT_PREVIEW_MAX;
  const text = truncated
    ? content.text.slice(0, RESOURCE_TEXT_PREVIEW_MAX) + "…"
    : content.text;
  return { kind: "text", text, truncated };
}

/** Render a prompt message as bounded plain text for preview. */
export function promptMessageText(message: McpPromptMessage): string {
  const content = message.content;
  switch (content.type) {
    case "text":
      return content.text;
    case "resource_link":
      return `[资源链接] ${content.uri}`;
    case "image":
      return "[图片] 未展开";
    case "audio":
      return "[音频] 未展开";
    case "embedded_resource": {
      const preview = resourcePreview(content.resource);
      return preview.kind === "text" ? preview.text : "[二进制资源] 未展开";
    }
  }
}

// ── API ──

export async function listMcpRuntimeServers(): Promise<McpServerRuntimeSummary[] | null> {
  const res = await request<{ servers: McpServerRuntimeSummary[] }>("GET", "/api/mcp/servers");
  return res?.servers ?? null;
}

export async function getMcpRuntimeServer(id: string) {
  return request<McpServerRuntimeSummary>("GET", `/api/mcp/servers/${id}`);
}

export async function refreshMcpRuntimeServer(id: string) {
  return request<McpServerRuntimeSummary>("POST", `/api/mcp/servers/${id}/refresh`);
}

export async function listMcpRuntimeTools(id: string): Promise<McpRuntimeTool[] | null> {
  const res = await request<{ tools: McpRuntimeTool[] }>("GET", `/api/mcp/servers/${id}/tools`);
  return res?.tools ?? null;
}

export async function listMcpResources(id: string): Promise<McpResourceDescriptor[] | null> {
  const res = await request<{ resources: McpResourceDescriptor[] }>(
    "GET",
    `/api/mcp/servers/${id}/resources`
  );
  return res?.resources ?? null;
}

export async function listMcpResourceTemplates(
  id: string
): Promise<McpResourceTemplate[] | null> {
  const res = await request<{ templates: McpResourceTemplate[] }>(
    "GET",
    `/api/mcp/servers/${id}/resource-templates`
  );
  return res?.templates ?? null;
}

export async function readMcpResource(id: string, uri: string) {
  return request<McpResourceReadResponse>("POST", `/api/mcp/servers/${id}/resources/read`, {
    uri,
  });
}

export async function listMcpPrompts(id: string): Promise<McpPromptDescriptor[] | null> {
  const res = await request<{ prompts: McpPromptDescriptor[] }>(
    "GET",
    `/api/mcp/servers/${id}/prompts`
  );
  return res?.prompts ?? null;
}

export async function getMcpPrompt(
  id: string,
  name: string,
  args: Record<string, unknown>
) {
  return request<McpPromptGetResponse>("POST", `/api/mcp/servers/${id}/prompts/get`, {
    name,
    arguments: args,
  });
}
