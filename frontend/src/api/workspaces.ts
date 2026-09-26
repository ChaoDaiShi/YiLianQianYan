// Workspaces — the multi-root container a task belongs to.
//
// Every call returns an `ApiResult`, so a caller that must explain a refusal
// (a duplicate name, a bad path) has the status and the backend's message.
// `listWorkspaces` unwraps the `{workspaces}` envelope and passes a failure
// through untouched.
import { requestResult } from "../core/api/http";
import type { ApiResult } from "../core/api/http";

export type WorkspaceStatus = "active" | "archived";

export interface Workspace {
  id: string;
  name: string;
  description: string;
  root_path?: string | null;
  status: WorkspaceStatus;
  created_at: number;
  updated_at: number;
  active_tasks?: number;
}

export async function listWorkspaces(): Promise<ApiResult<Workspace[]>> {
  const res = await requestResult<{ workspaces: Workspace[] }>("GET", "/api/workspaces");
  return res.ok ? { ok: true, data: res.data.workspaces } : res;
}

export async function createWorkspace(data: {
  name: string;
  description?: string;
  root_path?: string;
}) {
  return requestResult<Workspace>("POST", "/api/workspaces", data);
}

export async function getWorkspace(id: string) {
  return requestResult<Workspace>("GET", `/api/workspaces/${id}`);
}

export async function updateWorkspace(
  id: string,
  data: { name?: string; description?: string; root_path?: string }
) {
  return requestResult<Workspace>("PUT", `/api/workspaces/${id}`, data);
}

export async function deleteWorkspace(id: string) {
  return requestResult<{ status: string }>("DELETE", `/api/workspaces/${id}`);
}
