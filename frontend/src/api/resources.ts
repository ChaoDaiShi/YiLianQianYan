import { controlSessionHeaders } from "./controlSession";
import { API_BASE } from "./transport";

export interface Resource {
  id: string;
  source: string;
  name: string;
  mime_type: string;
  size: number;
  hash: string;
  storage_path: string;
  metadata: Record<string, unknown>;
  created_at: number;
  updated_at: number;
  [key: string]: unknown;
}

export async function ingestResourceBytes(
  name: string,
  mimeType: string,
  bytes: Uint8Array,
  signal?: AbortSignal,
): Promise<Resource | null> {
  const query = new URLSearchParams({ name });
  const response = await fetch(`${API_BASE}/api/resources/ingest?${query}`, {
    method: "POST",
    headers: {
      "Content-Type": mimeType || "application/octet-stream",
      ...controlSessionHeaders(),
    },
    body: bytes.slice().buffer as ArrayBuffer,
    signal,
  });
  if (!response.ok) return null;
  return (await response.json()) as Resource;
}

export async function ingestResource(file: File, signal?: AbortSignal): Promise<Resource | null> {
  return ingestResourceBytes(
    file.name,
    file.type || "application/octet-stream",
    new Uint8Array(await file.arrayBuffer()),
    signal,
  );
}

export interface ResourcePreview {
  status: "ready" | "failed" | "unsupported";
  kind: string;
  mime_type: string;
  text: string | null;
  truncated: boolean;
  width: number | null;
  height: number | null;
  error?: string;
}

export type ResourceTarget = { kind: "conversation"; conversation_id: string } | { kind: "task"; task_id: string } | { kind: "graph"; graph_id: string } | { kind: "node"; graph_id: string; node_id: string };
export interface ResourceBinding { id: string; resource_id: string; target: ResourceTarget; created_at: number }

export async function bindResources(target: ResourceTarget, resourceIds: string[]): Promise<ResourceBinding[]> {
  const response = await fetch(`${API_BASE}/api/resource-bindings`, { method: "POST", headers: { "Content-Type": "application/json", ...controlSessionHeaders() }, body: JSON.stringify({ target, resource_ids: resourceIds }) });
  if (!response.ok) throw new Error("资源绑定失败，请检查目标与附件数量后重试");
  return ((await response.json()) as { bindings: ResourceBinding[] }).bindings;
}
export async function listResourceBindings(target: ResourceTarget): Promise<ResourceBinding[]> {
  const response = await fetch(`${API_BASE}/api/resource-bindings?${new URLSearchParams({ target: JSON.stringify(target) })}`, { headers: controlSessionHeaders() });
  if (!response.ok) throw new Error("无法读取资源绑定");
  return ((await response.json()) as { bindings: ResourceBinding[] }).bindings;
}
export async function unbindResource(bindingId: string): Promise<void> {
  const response = await fetch(`${API_BASE}/api/resource-bindings/${encodeURIComponent(bindingId)}`, { method: "DELETE", headers: controlSessionHeaders() });
  if (!response.ok) throw new Error("解除绑定失败");
}

export async function getResourcePreview(id: string, signal?: AbortSignal): Promise<ResourcePreview> {
  const response = await fetch(`${API_BASE}/api/resources/${encodeURIComponent(id)}/preview`, { headers: controlSessionHeaders(), signal });
  if (!response.ok) throw new Error("资源解析结果暂不可用，请重试");
  return response.json() as Promise<ResourcePreview>;
}

export async function getResourceContent(id: string, signal?: AbortSignal): Promise<Blob> {
  const response = await fetch(`${API_BASE}/api/resources/${encodeURIComponent(id)}/content`, { headers: controlSessionHeaders(), signal });
  if (!response.ok) throw new Error("资源内容暂不可用，请重试");
  return response.blob();
}

export async function listResources(limit = 50, offset = 0): Promise<Resource[]> {
  const query = new URLSearchParams({ limit: String(limit), offset: String(offset) });
  const response = await fetch(`${API_BASE}/api/resources?${query}`, {
    headers: controlSessionHeaders(),
  });
  if (!response.ok) return [];
  const body = (await response.json()) as { resources: Resource[] };
  return body.resources;
}

export async function getResource(id: string): Promise<Resource | null> {
  const response = await fetch(`${API_BASE}/api/resources/${encodeURIComponent(id)}`, {
    headers: controlSessionHeaders(),
  });
  if (!response.ok) return null;
  return (await response.json()) as Resource;
}
