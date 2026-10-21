import { afterEach, describe, expect, it, vi } from "vitest";
vi.mock("./controlSession", () => ({ controlSessionHeaders: () => ({ "X-Yilian-Control-Session": "a".repeat(64) }) }));
import { getToolDiagnostics } from "./system";
afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); });
describe("tool diagnostic API", () => {
  it("requests persisted failures with an explicit status and result count", async () => {
    const payload = { entries: [], total: 0 };
    const fetchMock = vi.fn(async (_input: RequestInfo | URL) => new Response(JSON.stringify(payload), { status: 200, headers: { "Content-Type": "application/json" } }));
    vi.stubGlobal("fetch", fetchMock);
    await expect(getToolDiagnostics({ status: "error", count: 200 })).resolves.toEqual(payload);
    expect(String(fetchMock.mock.calls[0]?.[0])).toContain("/api/logs/tools?status=error&count=200");
  });
});
