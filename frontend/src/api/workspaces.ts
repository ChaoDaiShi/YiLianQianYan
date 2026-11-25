// Workspaces — the multi-root container a task belongs to.
//
// Every call returns an `ApiResult`, so a caller that must explain a refusal
// (a duplicate name, a bad path) has the status and the backend's message.
// `listWorkspaces` unwraps the `{workspaces}` envelope and passes a failure
// through untouched.
import { requestResult } from "../core/api/http";
import type { ApiResult } from "../core/api/http";

export type WorkspaceStatus = "active" | "archived";
export const WORKSPACE_PROJECT_STAGES = {
  planning: "规划中",
  in_progress: "开发中",
  testing: "测试中",
  released: "已发布",
  on_hold: "已暂停",
} as const;

export interface WorkspaceProject {
  stage: keyof typeof WORKSPACE_PROJECT_STAGES | null;
  frontend: string;
  backend: string;
}

export interface Workspace {
  id: string;
  name: string;
  description: string;
  root_path?: string | null;
  status: WorkspaceStatus;
  created_at: number;
  updated_at: number;
  active_tasks?: number;
  is_default?: boolean;
  project?: WorkspaceProject;
}

export async function listWorkspaces(): Promise<ApiResult<Workspace[]>> {
  const res = await requestResult<{ workspaces: Workspace[] }>("GET", "/api/workspaces");
  return res.ok ? { ok: true, data: res.data.workspaces } : res;
}

export async function createWorkspace(data: {
  name: string;
  description?: string;
  root_path?: string;
  project?: WorkspaceProject;
}) {
  return requestResult<Workspace>("POST", "/api/workspaces", data);
}

export async function getWorkspace(id: string) {
  return requestResult<Workspace>("GET", `/api/workspaces/${id}`);
}

export async function updateWorkspace(
  id: string,
  data: { name?: string; description?: string; root_path?: string; project?: WorkspaceProject }
) {
  return requestResult<Workspace>("PUT", `/api/workspaces/${id}`, data);
}

export async function deleteWorkspace(id: string) {
  return requestResult<{ status: string }>("DELETE", `/api/workspaces/${id}`);
}

export async function restoreWorkspace(id: string) {
  return requestResult<{ status: "active" }>("POST", `/api/workspaces/${encodeURIComponent(id)}/restore`);
}

export async function removeWorkspaceRegistration(id: string) {
  return requestResult<{ status: "deleted" }>("DELETE", `/api/workspaces/${encodeURIComponent(id)}/registration`);
}

export async function setDefaultWorkspace(id: string | null) {
  return requestResult<{ workspace_id: string | null }>("PUT", "/api/workspaces/default", { workspace_id: id });
}
