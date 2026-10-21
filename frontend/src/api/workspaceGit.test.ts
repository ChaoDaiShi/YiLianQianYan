import { beforeEach, describe, expect, it, vi } from "vitest";
import { getWorkspaceGit, getWorkspaceGitDiff } from "./workspaceGit";
const request = vi.hoisted(() => vi.fn());
vi.mock("../core/api/http", () => ({ requestResult: request }));
describe("read-only workspace Git client", () => {
  beforeEach(() => request.mockReset());
  it("requires and sends the explicitly selected registered workspace identifier", async () => {
    request.mockResolvedValue({ ok: true, data: { is_repository: false, files: [] } });
    await getWorkspaceGit("工作空间 1");
    expect(request).toHaveBeenLastCalledWith("GET", `/api/workspace-git?${new URLSearchParams({workspace_id:"工作空间 1"})}`);
  });
  it("encodes a literal Unicode file path and staged choice without accepting client root paths", async () => {
    request.mockResolvedValue({ ok: false, status: 400, error: "该文件不在当前 Git 变更列表中" });
    const result = await getWorkspaceGitDiff("目录/文件 & #.txt", true, "workspace-1");
    const call = request.mock.calls[0]; const url = new URL(call[1], "http://local");
    expect(call[0]).toBe("GET"); expect(url.pathname).toBe("/api/workspace-git/diff");
    expect(url.searchParams.get("path")).toBe("目录/文件 & #.txt");
    expect(url.searchParams.get("staged")).toBe("true");
    expect(url.searchParams.get("workspace_id")).toBe("workspace-1");
    expect(url.searchParams.has("root")).toBe(false); expect(result.ok).toBe(false);
  });
});
