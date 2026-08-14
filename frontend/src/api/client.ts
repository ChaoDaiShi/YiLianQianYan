// ============================================================
// API Client — fetch-based HTTP + SSE for backend communication
// ============================================================

import { controlSessionHeaders } from "./controlSession";
import type { AppConfig } from "../types";

export const API_BASE =
  (import.meta.env.VITE_API_BASE as string | undefined)?.replace(/\/$/, "") ||
  "http://127.0.0.1:9420";

async function request<T>(
  method: string,
  path: string,
  body?: unknown
): Promise<T | null> {
  try {
    const opts: RequestInit = {
      method,
      headers: {
        "Content-Type": "application/json",
        ...controlSessionHeaders(),
      },
    };
    if (body !== undefined) {
      opts.body = JSON.stringify(body);
    }
    const res = await fetch(`${API_BASE}${path}`, opts);
    if (!res.ok) {
      console.error(`API ${method} ${path}: ${res.status}`);
      return null;
    }
    if (res.status === 204) return {} as T;
    const text = await res.text();
    if (!text) return {} as T;
    return JSON.parse(text) as T;
  } catch (e) {
    console.error(`API ${method} ${path} failed:`, e);
    return null;
  }
}

// ── Conversations ──

export async function listConversations() {
  return request<any[]>("GET", "/api/conversations");
}

export async function createConversation(title?: string) {
  return request<any>("POST", "/api/conversations", { title });
}

export async function loadConversation(id: string) {
  return request<any>("GET", `/api/conversations/${id}`);
}

export async function deleteConversation(id: string) {
  return request<any>("DELETE", `/api/conversations/${id}`);
}

// ── Settings ──

export async function getSettings() {
  return request<AppConfig>("GET", "/api/settings");
}

export async function updateSettings(config: AppConfig) {
  return request<{ status?: string; error?: string }>("PUT", "/api/settings", config);
}

// ── Tools ──

export async function listTools() {
  return request<any[]>("GET", "/api/tools");
}

// ── Skills ──

export async function listSkills() {
  return request<any[]>("GET", "/api/skills");
}

export async function loadSkill(name: string) {
  return request<any>("GET", `/api/skills/${encodeURIComponent(name)}`);
}

// ── System ──

export async function getSystemInfo() {
  return request<any>("GET", "/api/system");
}

// ── Memories ──

export interface MemoryRecord {
  id: string;
  content: string;
  category: string;
  source: string;
  source_conversation_id?: string;
  metadata?: string;
  created_at: number;
  updated_at: number;
}

export interface MemoryStats {
  total: number;
  by_category: Array<[string, number]>;
  by_source: Array<[string, number]>;
}

export interface MemoryReindexResult {
  ok: boolean;
  requested?: number;
  processed?: number;
  succeeded?: number;
  failed?: number;
  remaining?: number;
  error?: string;
}

export type MemoryRetrievalMode = "lexical" | "hybrid" | "lexical_fallback";

export const MEMORY_RETRIEVAL_MODE_LABELS: Record<MemoryRetrievalMode, string> = {
  lexical: "词法检索",
  hybrid: "混合检索",
  lexical_fallback: "Embedding 失败，已回退词法检索",
};

export interface ScoredMemory extends MemoryRecord {
  score: number;
  lexical_score: number;
  vector_score: number;
}

export interface MemoryRetrieveResponse {
  query: string;
  count: number;
  mode: MemoryRetrievalMode;
  memories: ScoredMemory[];
  ok?: boolean;
  error?: string;
}

export interface MemoryQuery {
  category?: string;
  source?: string;
  q?: string;
  limit?: number;
  offset?: number;
}

export async function listMemories(query?: MemoryQuery) {
  const params = new URLSearchParams();
  if (query?.category) params.set("category", query.category);
  if (query?.source) params.set("source", query.source);
  if (query?.q) params.set("q", query.q);
  if (query?.limit) params.set("limit", String(query.limit));
  if (query?.offset) params.set("offset", String(query.offset));
  const qs = params.toString();
  return request<MemoryRecord[]>("GET", `/api/memories${qs ? `?${qs}` : ""}`);
}

export async function getMemory(id: string) {
  return request<MemoryRecord>("GET", `/api/memories/${id}`);
}

export async function createMemory(data: {
  content: string;
  category?: string;
  source?: string;
  metadata?: string;
}) {
  return request<MemoryRecord>("POST", "/api/memories", data);
}

export async function updateMemory(
  id: string,
  data: { content?: string; category?: string; metadata?: string }
) {
  return request<MemoryRecord>("PUT", `/api/memories/${id}`, data);
}

export async function deleteMemory(id: string) {
  return request<{ status: string }>("DELETE", `/api/memories/${id}`);
}

export async function getMemoryStats() {
  return request<MemoryStats>("GET", "/api/memories/stats");
}

export async function extractMemories(conversationId?: string) {
  return request<{ status: string; message: string }>("POST", "/api/memories/extract", {
    conversation_id: conversationId,
  });
}

export async function batchImportMemories(items: Array<{ content: string; category?: string }>) {
  return request<any>("POST", "/api/memories/batch-import", { items });
}

export async function batchDeleteMemories(ids: string[]) {
  return request<any>("POST", "/api/memories/batch-delete", { ids });
}

export async function exportMemories() {
  return request<any>("GET", "/api/memories/export");
}

export async function mergeMemories(ids: string[]) {
  return request<any>("POST", "/api/memories/merge", { ids });
}

export async function reindexMemories() {
  return request<MemoryReindexResult>("POST", "/api/memories/reindex", { limit: 50 });
}

export async function retrieveMemories(q: string, topK = 10, category?: string) {
  const params = new URLSearchParams({ q, top_k: String(topK) });
  if (category) params.set("category", category);
  return request<MemoryRetrieveResponse>("GET", `/api/memories/retrieve?${params.toString()}`);
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
}

export const MCP_RUNTIME_STATUS_LABELS = {
  ready: "MCP stdio Runtime 已启用",
  unready: "MCP Runtime 未就绪",
} as const;

export function mcpTransportRuntimeLabel(transport: string): string {
  return transport === "stdio" ? "Runtime supported" : "Not supported by current runtime";
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
  return request<McpServer>("POST", "/api/plugins/mcp", data);
}

export async function updateMcpServer(id: string, data: Partial<McpServer>) {
  return request<McpServer>("PUT", `/api/plugins/mcp/${id}`, data);
}

export async function deleteMcpServer(id: string) {
  return request<{ status: string }>("DELETE", `/api/plugins/mcp/${id}`);
}

export async function toggleMcpServer(id: string) {
  return request<McpServer>("POST", `/api/plugins/mcp/${id}/toggle`);
}

export async function testMcpServer(id: string) {
  return request<{ ok: boolean; message: string }>("POST", `/api/plugins/mcp/${id}/test`);
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

// ── Workflows ──

export interface Workflow {
  id: string;
  name: string;
  description: string;
  nodes: string[];
  tags: string[];
  system_prompt_extra?: string;
  is_builtin: boolean;
  created_at: number;
  updated_at: number;
}

export async function listWorkflows() {
  return request<{ workflows: Workflow[]; active_id: string | null }>("GET", "/api/workflows");
}

export async function getWorkflow(id: string) {
  return request<Workflow>("GET", `/api/workflows/${id}`);
}

export async function createWorkflow(data: Partial<Workflow>) {
  return request<Workflow>("POST", "/api/workflows", data);
}

export async function updateWorkflow(id: string, data: Partial<Workflow>) {
  return request<Workflow>("PUT", `/api/workflows/${id}`, data);
}

export async function deleteWorkflow(id: string) {
  return request<{ status: string }>("DELETE", `/api/workflows/${id}`);
}

export async function activateWorkflow(id: string) {
  return request<{ active_id: string }>("POST", `/api/workflows/${id}/activate`);
}

// ── Health ──

export interface RuntimeHealth {
  status: "healthy" | "degraded";
  service: string;
  version: string;
  database: "healthy" | "unavailable";
  policy_version: string;
}

export async function healthCheck(): Promise<RuntimeHealth | null> {
  try {
    const response = await fetch(`${API_BASE}/api/health`, {
      method: "GET",
      headers: { Accept: "application/json" },
    });
    if (!response.ok) {
      console.error(`API GET /api/health: ${response.status}`);
      return null;
    }
    return (await response.json()) as RuntimeHealth;
  } catch (error) {
    console.error("API GET /api/health failed:", error);
    return null;
  }
}

export async function isServerAvailable(): Promise<boolean> {
  const result = await healthCheck();
  return result !== null;
}

// ============================================================
// SSE Streaming (for chat)
// ============================================================

export interface AgentEvent {
  type: string;
  conversation_id: string;
  token?: string;
  tool_call_id?: string;
  tool_name?: string;
  args?: Record<string, unknown>;
  result?: string;
  status?: string;
  error?: string;
  message_id?: string;
  risk_level?: string;
  reason?: string;
  approval_id?: string;
  verification_success?: boolean;
  verification_reason?: string;
  should_replan?: boolean;
}

export type EventHandler = (event: AgentEvent) => void;

export function sendMessage(
  message: string,
  conversationId: string | null,
  onEvent: EventHandler,
  workflowId?: string | null
): AbortController {
  const controller = new AbortController();

  fetch(`${API_BASE}/api/chat`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...controlSessionHeaders(),
    },
    body: JSON.stringify({
      conversation_id: conversationId,
      message,
      workflow_id: workflowId || undefined,
    }),
    signal: controller.signal,
  })
    .then(async (res) => {
      if (!res.ok) {
        onEvent({
          type: "error",
          conversation_id: conversationId || "",
          error: `HTTP ${res.status}`,
        });
        return;
      }

      const reader = res.body?.getReader();
      if (!reader) return;

      const decoder = new TextDecoder();
      let buffer = "";
      let currentEvent = "";

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() || "";

        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed) {
            currentEvent = "";
            continue;
          }

          if (trimmed.startsWith("event:")) {
            currentEvent = trimmed.slice(6).trim();
            continue;
          }

          if (trimmed.startsWith("data:")) {
            const data = trimmed.slice(5).trim();
            if (data === "ping" || data === "[DONE]") continue;

            try {
              const parsed = JSON.parse(data);

              if (currentEvent === "connected") {
                onEvent({
                  type: "connected",
                  conversation_id: parsed.conversation_id || "",
                });
              } else if (parsed.type) {
                onEvent(parsed as AgentEvent);
              }
            } catch {
              console.warn("SSE parse error for:", data);
            }
          }
        }
      }

      // Stream ended without an explicit terminal event (e.g. agent paused
      // for approval). Let the handler clear transient loading state.
      onEvent({ type: "stream_end", conversation_id: conversationId || "" });
    })
    .catch((err) => {
      if (err.name !== "AbortError") {
        onEvent({
          type: "error",
          conversation_id: conversationId || "",
          error: String(err),
        });
      }
    });

  return controller;
}

export async function stopGeneration(conversationId: string) {
  return request<any>("POST", "/api/chat/stop", { conversation_id: conversationId });
}

// ── Logs ──

export interface LogEntry {
  timestamp: number;
  level: string;
  source: string;
  message: string;
}

export interface LogsResponse {
  entries: LogEntry[];
  total: number;
}

export async function getLogs(params?: {
  count?: number;
  level?: string;
  source?: string;
  drain?: boolean;
}): Promise<LogsResponse | null> {
  const qs = new URLSearchParams();
  if (params?.count) qs.set("count", String(params.count));
  if (params?.level) qs.set("level", params.level);
  if (params?.source) qs.set("source", params.source);
  if (params?.drain) qs.set("drain", "true");
  const query = qs.toString();
  return request<LogsResponse>("GET", `/api/logs${query ? `?${query}` : ""}`);
}

export async function pushLog(level: string, source: string, message: string) {
  return request<any>("POST", "/api/logs", { level, source, message });
}
