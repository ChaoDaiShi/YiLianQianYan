import { API_BASE } from "./transport";
import { controlSessionHeaders } from "./controlSession";

export interface OutputOrigin {
  kind: string; id: string; source_workspace_id: string | null; destination_workspace_id: string | null;
  conversation_id: string | null; automation_id: string | null; graph_id: string | null;
  node_id: string | null; execution_id: string | null; tool_call_id: string | null;
}
export interface OutputRef {
  id: string; kind: "artifact" | "conversation" | "personal_file"; title: string;
  mime_type: string | null; source_workspace_id: string | null; destination_workspace_id: string | null;
  source_href: string; source: OutputOrigin; execution_status: string; verification_status: string;
  created_at: number; updated_at: number; preview_available: boolean; download_url: string | null;
}
export interface OutputCatalogue { outputs: OutputRef[]; issues?: Array<{ kind: string; id: string; error: string }>; limit: number; truncated: boolean }
export interface OutputPreview {
  output: OutputRef; text: string | null; truncated: boolean;
  resource_preview?: { status: string; kind: string; mime_type: string; text: string | null; truncated: boolean; error?: string | null };
}
async function read<T>(path: string): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, { headers: controlSessionHeaders() });
  const body = await response.json().catch(() => null) as { message?: string } | null;
  if (!response.ok) throw new Error(body?.message || `成果读取失败 (${response.status})`);
  return body as T;
}
export function listOutputs(workspaceId?: string, limit = 50): Promise<OutputCatalogue> {
  const query = new URLSearchParams({ limit: String(limit) });
  if (workspaceId) query.set("workspace_id", workspaceId);
  return read(`/api/outputs?${query}`);
}
export function previewOutput(output: OutputRef, workspaceId?: string): Promise<OutputPreview> {
  const query = new URLSearchParams();
  if (output.kind === "conversation") query.set("source_id", output.source.conversation_id || "");
  if (output.kind === "personal_file") query.set("source_id", output.source.automation_id || "");
  if (workspaceId) query.set("workspace_id", workspaceId);
  return read(`/api/outputs/${output.kind}/${encodeURIComponent(output.id)}/preview?${query}`);
}
export async function downloadOutput(output: OutputRef, workspaceId?: string): Promise<Blob> {
  const query = new URLSearchParams();
  if (output.kind === "conversation") query.set("source_id", output.source.conversation_id || "");
  if (workspaceId) query.set("workspace_id", workspaceId);
  const response = await fetch(`${API_BASE}/api/outputs/${output.kind}/${encodeURIComponent(output.id)}/download?${query}`, { headers: controlSessionHeaders() });
  if (!response.ok) { const body = await response.json().catch(() => null) as { message?: string } | null; throw new Error(body?.message || "成果下载失败"); }
  return response.blob();
}
