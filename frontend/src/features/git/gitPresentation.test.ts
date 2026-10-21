import { describe, expect, it } from "vitest";
import { gitChangeLabel, gitTrackingLabel } from "./gitPresentation";
import type { GitFileChange } from "../../api/workspaceGit";
const modified: GitFileChange = { path: "修改.txt", original_path: null, index_status: "M", worktree_status: "M", staged: true, unstaged: true, untracked: false };
describe("Git node labels", () => {
  it("distinguishes staged, unstaged, renamed, untracked and conflicted changes", () => {
    expect(gitChangeLabel(modified, true)).toBe("已修改");
    expect(gitChangeLabel({ ...modified, index_status: "R", original_path: "old.txt" }, true)).toBe("已重命名");
    expect(gitChangeLabel({ ...modified, index_status: "?", worktree_status: "?", untracked: true }, false)).toBe("未跟踪");
    expect(gitChangeLabel({ ...modified, index_status: "U", worktree_status: "U" }, false)).toBe("存在冲突");
    expect(gitChangeLabel({ ...modified, index_status: "D", worktree_status: "D" }, true)).toBe("存在冲突");
  });
  it("does not invent upstream synchronization when no tracking branch exists", () => {
    expect(gitTrackingLabel(null, null)).toBe("未设置上游分支");
    expect(gitTrackingLabel(0, 0)).toBe("与本地上游引用一致");
    expect(gitTrackingLabel(2, 1)).toBe("领先 2 · 落后 1");
  });
});
