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
): Promise<Resource | null> {
  const query = new URLSearchParams({ name });
  const response = await fetch(`${API_BASE}/api/resources/ingest?${query}`, {
    method: "POST",
    headers: {
      "Content-Type": mimeType || "application/octet-stream",
      ...controlSessionHeaders(),
    },
    body: bytes.slice().buffer as ArrayBuffer,
  });
  if (!response.ok) return null;
  return (await response.json()) as Resource;
}

export async function ingestResource(file: File): Promise<Resource | null> {
  return ingestResourceBytes(
    file.name,
    file.type || "application/octet-stream",
    new Uint8Array(await file.arrayBuffer()),
  );
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
