import { requestResult } from "../../core/api/http";

export interface WorkspaceCreation {
  id: string;
  workspace_id: string;
  title: string;
  current_version_id: string;
  revision: number;
  created_at: number;
  updated_at: number;
}
export interface WorkspaceCreationVersion {
  id: string;
  creation_id: string;
  revision: number;
  title: string;
  resource_id: string;
  parent_version_id: string | null;
  author_kind: "user" | "agent";
  run_id: string | null;
  created_at: number;
  current: boolean;
}
export interface CreationDetail {
  creation: WorkspaceCreation;
  content: string;
  versions: WorkspaceCreationVersion[];
}
export interface CreationHistory { version: WorkspaceCreationVersion; content: string }
async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const result = await requestResult<T>(method, path, body);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}
const base = (workspace: string) => `/api/workspaces/${encodeURIComponent(workspace)}/creations`;
export const listCreations = (workspace: string, offset = 0) => call<{ creations: WorkspaceCreation[]; has_more: boolean }>("GET", `${base(workspace)}?limit=50&offset=${offset}`);
export const createCreation = (workspace: string, title: string, content: string) => call<CreationDetail>("POST", base(workspace), { title, content });
export const getCreation = (workspace: string, id: string) => call<CreationDetail>("GET", `${base(workspace)}/${encodeURIComponent(id)}`);
export const saveCreation = (workspace: string, id: string, title: string, content: string, expected_revision: number) => call<CreationDetail>("PUT", `${base(workspace)}/${encodeURIComponent(id)}`, { title, content, expected_revision });
export const getCreationVersion = (workspace: string, id: string, version: string) => call<CreationHistory>("GET", `${base(workspace)}/${encodeURIComponent(id)}/versions/${encodeURIComponent(version)}`);
