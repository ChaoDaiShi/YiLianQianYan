import { afterEach, describe, expect, it, vi } from "vitest";

import {
  MCP_RUNTIME_STATUS_LABELS,
  mcpTransportRuntimeLabel,
} from "./client";

vi.mock("./controlSession", () => ({
  controlSessionHeaders: () => ({ "X-Control-Session": "test-session" }),
}));

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("MCP runtime product labels", () => {
  it("distinguishes stdio support from unsupported transports", () => {
    expect(mcpTransportRuntimeLabel("stdio")).toBe("Runtime supported");
    expect(mcpTransportRuntimeLabel("sse")).toBe("Not supported by current runtime");
    expect(mcpTransportRuntimeLabel("http")).toBe("Not supported by current runtime");
  });

  it("exposes explicit runtime-ready labels", () => {
    expect(MCP_RUNTIME_STATUS_LABELS.ready).toBe("MCP stdio Runtime 已启用");
    expect(MCP_RUNTIME_STATUS_LABELS.unready).toBe("MCP Runtime 未就绪");
  });
});
