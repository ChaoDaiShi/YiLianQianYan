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

function resourceIngestUrl(name: string) {
  const query = new URLSearchParams({ name });
  return `${API_BASE}/api/resources/ingest?${query}`;
}

async function postResource(name: string, mimeType: string, body: BodyInit, signal?: AbortSignal): Promise<Resource | null> {
  const response = await fetch(resourceIngestUrl(name), {
    method: "POST",
    headers: {
      "Content-Type": mimeType || "application/octet-stream",
      ...controlSessionHeaders(),
    },
    body,
    signal,
  });
  if (!response.ok) return null;
  return (await response.json()) as Resource;
}

export function ingestResourceBytes(name: string, mimeType: string, bytes: Uint8Array, signal?: AbortSignal): Promise<Resource | null> {
  return postResource(name, mimeType, bytes.slice().buffer as ArrayBuffer, signal);
}

export async function ingestResource(file: File, signal?: AbortSignal): Promise<Resource | null> {
  return postResource(file.name, file.type, file, signal);
}

export class ResourceUploadError extends Error {
  constructor(message: string, readonly retryable: boolean) {
    super(message);
  }
}

/** Uploads the original file and reports bytes actually transferred. */
export function uploadResource(file: File, signal: AbortSignal, onProgress: (loaded: number) => void): Promise<Resource> {
  const aborted = () => new DOMException("上传已取消", "AbortError");
  if (signal.aborted) return Promise.reject(aborted());
  return new Promise((resolve, reject) => {
    const request = new XMLHttpRequest();
    const abort = () => request.abort();
    request.open("POST", resourceIngestUrl(file.name));
    request.responseType = "json";
    request.setRequestHeader("Content-Type", file.type || "application/octet-stream");
    for (const [name, value] of Object.entries(controlSessionHeaders())) request.setRequestHeader(name, value);
    request.upload.onprogress = event => onProgress(event.loaded);
    request.onload = () => {
      if (request.status >= 200 && request.status < 300 && typeof request.response?.id === "string") {
        resolve(request.response as Resource);
        return;
      }
      const status = request.status;
      const inputError = request.response?.error === "resource_input_error";
      const message = inputError && typeof request.response.message === "string" ? request.response.message
        : status === 413 ? "附件超过 25 MiB 上传上限"
          : status === 401 || status === 403 ? "附件上传未获授权，请重新打开应用后重试"
            : "附件上传失败，请重试";
      reject(new ResourceUploadError(message, status < 400 || status === 408 || status === 429 || status >= 500));
    };
    request.onerror = () => reject(new ResourceUploadError("连接中断，附件未上传完成", true));
    request.onabort = () => reject(aborted());
    request.onloadend = () => signal.removeEventListener("abort", abort);
    signal.addEventListener("abort", abort, { once: true });
    request.send(file);
  });
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
  presentation?: PreviewPresentation | null;
}

export interface PreviewBlock {
  kind: "paragraph" | "heading" | "table" | "image";
  text?: string;
  level?: number;
  runs?: { text: string; bold?: boolean; italic?: boolean; underline?: boolean }[];
  rows?: string[][];
  src?: string;
  alt?: string;
}
export interface PreviewPresentation {
  blocks: PreviewBlock[];
  sheets: { name: string; rows: string[][] }[];
  slides: { number: number; blocks: PreviewBlock[] }[];
  entries: { name: string; size: number; directory: boolean }[];
  page_count?: number;
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
  if (!response.ok) throw new ResourceUploadError("资源解析结果暂不可用，请重试", response.status === 408 || response.status === 429 || response.status >= 500);
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
