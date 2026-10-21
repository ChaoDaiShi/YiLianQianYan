import { requestResult } from "../core/api/http";

export interface GitFileChange {
  path: string;
  original_path: string | null;
  index_status: string;
  worktree_status: string;
  staged: boolean;
  unstaged: boolean;
  untracked: boolean;
}
export interface WorkspaceGitStatus {
  is_repository: boolean;
  root: string;
  branch: string | null;
  head: string | null;
  ahead: number | null;
  behind: number | null;
  files: GitFileChange[];
}
export interface WorkspaceGitDiff {
  path: string;
  staged: boolean;
  untracked: boolean;
  content: string;
  binary: boolean;
  truncated: boolean;
}
export function getWorkspaceGit(workspaceId: string) {
  const query = `?${new URLSearchParams({ workspace_id: workspaceId })}`;
  return requestResult<WorkspaceGitStatus>("GET", `/api/workspace-git${query}`);
}
export function getWorkspaceGitDiff(path: string, staged: boolean, workspaceId: string) {
  const query = new URLSearchParams({ path, staged: String(staged), workspace_id: workspaceId });
  return requestResult<WorkspaceGitDiff>("GET", `/api/workspace-git/diff?${query}`);
}
