import { requestResult } from "../core/api/http";

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
