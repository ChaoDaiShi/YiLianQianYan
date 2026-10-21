import type { GitFileChange } from "../../api/workspaceGit";

export function gitChangeLabel(file: GitFileChange, staged: boolean): string {
  if (file.untracked) return "未跟踪";
  if (file.index_status === "U" || file.worktree_status === "U" ||
      (file.index_status === "D" && file.worktree_status === "D") ||
      (file.index_status === "A" && file.worktree_status === "A")) return "存在冲突";
  const code = staged ? file.index_status : file.worktree_status;
  return ({ M: "已修改", A: "已新增", D: "已删除", R: "已重命名", C: "已复制", T: "类型变更" } as Record<string, string>)[code] ?? code;
}
export function gitTrackingLabel(ahead: number | null, behind: number | null): string {
  if (ahead === null || behind === null) return "未设置上游分支";
  return ahead === 0 && behind === 0 ? "与本地上游引用一致" : `领先 ${ahead} · 落后 ${behind}`;
}
