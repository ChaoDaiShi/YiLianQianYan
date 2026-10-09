import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("./controlSession", () => ({
  controlSessionHeaders: () => ({ "X-Yilian-Control-Session": "a".repeat(64) }),
}));

import { getProviderReadiness, verifyProviderConnection } from "./providerConnection";
import chatSource from "../components/chat/ChatView.tsx?raw";
import taskCenterSource from "../pages/TaskCenterPage.tsx?raw";
import taskWorldSource from "../features/task-world/TaskWorldPage.tsx?raw";

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

  it("reads the shared readiness projection from redacted settings", async () => {
    const readiness = {
      model: { configured: false, available: false, provider: "openai", model: "" },
      stt: { configured: false, available: false, provider: "minimax", model: "asr-1.0" },
      tts: { configured: true, available: true, provider: "minimax", model: "speech-02-hd" },
    };
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({
      agent: {}, model: {}, voice: {}, permissions: {}, sandbox: {}, skills: {}, subagents: {}, compaction: {},
      provider_readiness: readiness,
    }), { status: 200, headers: { "Content-Type": "application/json" } })));

    await expect(getProviderReadiness()).resolves.toEqual(readiness);
  });

  it("guides chat, planning and AI review to model settings before invoking an unavailable model", () => {
    for (const source of [chatSource, taskCenterSource, taskWorldSource]) {
      expect(source).toContain("getProviderReadiness");
      expect(source).toContain("还没有配置可用的模型服务");
      expect(source).toContain("前往模型设置");
    }
  });
});
