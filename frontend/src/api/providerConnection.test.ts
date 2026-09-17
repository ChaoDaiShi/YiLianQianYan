import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("./controlSession", () => ({
  controlSessionHeaders: () => ({ "X-Yilian-Control-Session": "a".repeat(64) }),
}));

import { verifyProviderConnection } from "./providerConnection";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("provider connection API", () => {
  it("uses the protected provider leaf route and exposes only normalized errors", async () => {
    const fetchMock = vi.fn(async () => new Response(JSON.stringify({ error: "INVALID_CREDENTIAL" }), {
      status: 400,
      headers: { "Content-Type": "application/json" },
    }));
    vi.stubGlobal("fetch", fetchMock);

    await expect(verifyProviderConnection("stt")).resolves.toEqual({
      ok: false,
      error: "INVALID_CREDENTIAL",
    });
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringMatching(/\/api\/providers\/stt\/verify$/),
      expect.anything(),
    );
  });

  it("rejects an unrecognized backend error instead of rendering provider details", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ error: "vendor body" }), {
      status: 502,
      headers: { "Content-Type": "application/json" },
    })));

    await expect(verifyProviderConnection("tts")).resolves.toEqual({
      ok: false,
      error: "INVALID_CONFIGURATION",
    });
  });
});
