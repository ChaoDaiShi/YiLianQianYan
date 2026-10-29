import { afterEach, describe, expect, it, vi } from "vitest";
import { getWorkspaceFileContent, getWorkspaceFilePreview } from "./workspaceFiles";

vi.mock("./controlSession", () => ({ controlSessionHeaders: () => ({ "X-Yilian-Control-Session": "test-session" }) }));
afterEach(() => vi.unstubAllGlobals());

describe("workspace file preview endpoints", () => {
  it("encodes workspace and path and returns the shared preview contract", async () => {
    const preview = { status: "ready", kind: "docx", presentation: { blocks: [], sheets: [], slides: [], entries: [] } };
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(preview), { status: 200 })); vi.stubGlobal("fetch", fetchMock);
    const controller = new AbortController();
    expect(await getWorkspaceFilePreview("space/1", "资料/报告 1.docx", controller.signal)).toEqual(preview);
    expect(fetchMock).toHaveBeenCalledWith(expect.stringContaining("/api/workspaces/space%2F1/file/preview?path=%E8%B5%84%E6%96%99%2F%E6%8A%A5%E5%91%8A+1.docx"), expect.objectContaining({ headers: { "X-Yilian-Control-Session": "test-session" }, signal: controller.signal }));
  });
  it("returns exact content bytes and reports failed reads", async () => {
    const bytes = new Uint8Array([0, 255, 42]);
    const fetchMock = vi.fn().mockResolvedValueOnce(new Response(bytes, { headers: { "Content-Type": "application/pdf" } })).mockResolvedValueOnce(new Response("missing", { status: 404 })); vi.stubGlobal("fetch", fetchMock);
    const result = await getWorkspaceFileContent("workspace", "file.pdf");
    expect(result.type).toBe("application/pdf"); expect([...new Uint8Array(await result.arrayBuffer())]).toEqual([...bytes]);
    await expect(getWorkspaceFileContent("workspace", "missing.pdf")).rejects.toThrow("工作空间文件内容暂不可用");
  });
});
