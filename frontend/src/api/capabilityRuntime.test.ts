import { afterEach, describe, expect, it, vi } from "vitest";

import { API_BASE, listCapabilities, refreshCapabilities } from "./client";

vi.mock("./controlSession", () => ({
  controlSessionHeaders: () => ({ "X-Control-Session": "test-session" }),
}));

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

describe("capability runtime API client", () => {
  it("listCapabilities maps the envelope and filters by kind", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      jsonResponse({
        capabilities: [{ id: "builtin.tool.read_file", kind: "tool", status: "ready" }],
        total: 1,
      })
    );
    vi.stubGlobal("fetch", fetchMock);
    const res = await listCapabilities({ kind: "tool" });
    expect(res.ok).toBe(true);
    const [url] = fetchMock.mock.calls[0] as [string];
    expect(url).toContain("kind=tool");
    if (res.ok) {
      expect(res.data.capabilities[0].id).toBe("builtin.tool.read_file");
    }
  });

  it("refreshCapabilities returns a refresh report", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        jsonResponse({ discovered: 3, ready: 3, unavailable: 0, duplicates: 0, provider_failures: 0 })
      )
    );
    const res = await refreshCapabilities();
    expect(res.ok).toBe(true);
    if (res.ok) expect(res.data.discovered).toBe(3);
  });

  it("surfaces a non-ok status through ApiResult", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(jsonResponse({}, 401)));
    const res = await listCapabilities();
    expect(res.ok).toBe(false);
    if (!res.ok) expect(res.status).toBe(401);
  });
});
