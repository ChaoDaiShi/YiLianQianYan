import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("./controlSession", () => ({
  controlSessionHeaders: () => ({ "X-Yilian-Control-Session": "a".repeat(64) }),
}));

import { API_BASE, createMcpServer, healthCheck } from "./client";

const healthyResponse = {
  status: "healthy",
  service: "yilian-backend",
  version: "0.3.1",
  database: "healthy",
  policy_version: "security-rbac-v3",
} as const;

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("healthCheck", () => {
  it("reads the public health endpoint without a control-session header", async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify(healthyResponse), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    await expect(healthCheck()).resolves.toEqual(healthyResponse);
    expect(fetchMock).toHaveBeenCalledWith(`${API_BASE}/api/health`, {
      method: "GET",
      headers: { Accept: "application/json" },
    });
  });

  it("returns null when the public health endpoint is unavailable", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    vi.stubGlobal("fetch", vi.fn(async () => new Response("", { status: 503 })));

    await expect(healthCheck()).resolves.toBeNull();
  });

  it("returns null when the health request cannot connect", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    vi.stubGlobal("fetch", vi.fn(async () => Promise.reject(new Error("offline"))));

    await expect(healthCheck()).resolves.toBeNull();
  });
});

describe("MCP mutations", () => {
  it("returns the backend validation message instead of silently succeeding", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response("Stdio 服务必须填写启动命令。", { status: 400 })));

    await expect(createMcpServer({ name: "Broken", transport: "stdio" })).resolves.toEqual({
      ok: false,
      status: 400,
      error: "Stdio 服务必须填写启动命令。",
    });
  });
});
