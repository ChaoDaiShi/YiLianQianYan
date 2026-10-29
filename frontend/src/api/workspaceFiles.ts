import { requestResult } from "../core/api/http";
import { API_BASE } from "./transport";
import { controlSessionHeaders } from "./controlSession";
import type { ResourcePreview } from "./resources";

export interface WorkspaceFileEntry {
  name: string;
  path: string;
  kind: "directory" | "file" | "link" | "other";
  size: number;
}

export interface WorkspaceDirectory {
  path: string;
  entries: WorkspaceFileEntry[];
}

export interface WorkspaceTextFile {
  name: string;
  path: string;
  size: number;
  content: string;
}

export function listWorkspaceFiles(workspaceId: string, path = "") {
  const query = new URLSearchParams({ path });
  return requestResult<WorkspaceDirectory>("GET", `/api/workspaces/${encodeURIComponent(workspaceId)}/files?${query}`);
}

export function readWorkspaceFile(workspaceId: string, path: string) {
  const query = new URLSearchParams({ path });
  return requestResult<WorkspaceTextFile>("GET", `/api/workspaces/${encodeURIComponent(workspaceId)}/file?${query}`);
}

export async function getWorkspaceFilePreview(workspaceId: string, path: string, signal?: AbortSignal): Promise<ResourcePreview> {
  const query = new URLSearchParams({ path });
  const response = await fetch(`${API_BASE}/api/workspaces/${encodeURIComponent(workspaceId)}/file/preview?${query}`, { headers: controlSessionHeaders(), signal });
  if (!response.ok) throw new Error("工作空间文件预览暂不可用，请重试");
  return response.json() as Promise<ResourcePreview>;
}

export async function getWorkspaceFileContent(workspaceId: string, path: string, signal?: AbortSignal): Promise<Blob> {
  const query = new URLSearchParams({ path });
  const response = await fetch(`${API_BASE}/api/workspaces/${encodeURIComponent(workspaceId)}/file/content?${query}`, { headers: controlSessionHeaders(), signal });
  if (!response.ok) throw new Error("工作空间文件内容暂不可用，请重试");
  return response.blob();
}
