import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("./controlSession", () => ({
  controlSessionHeaders: () => ({ "X-Yilian-Control-Session": "a".repeat(64) }),
}));

import { ingestResourceBytes } from "./resources";

afterEach(() => vi.unstubAllGlobals());

describe("resource API", () => {
  it("uploads bytes without sending an original absolute path", async () => {
    const fetchMock = vi.fn(async () => new Response(JSON.stringify({ id: "res-1", name: "notes.txt" }), {
      status: 201,
      headers: { "Content-Type": "application/json" },
    }));
    vi.stubGlobal("fetch", fetchMock);

    await ingestResourceBytes("notes.txt", "text/plain", new Uint8Array([1, 2, 3]));

    const [url, options] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toContain("/api/resources/ingest?name=notes.txt");
    expect(url).not.toContain("Users");
    expect(options.headers).toMatchObject({ "Content-Type": "text/plain" });
    expect(options.body).toBeInstanceOf(ArrayBuffer);
  });
});
