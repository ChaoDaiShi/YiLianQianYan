import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("./controlSession", () => ({
  controlSessionHeaders: () => ({ "X-Yilian-Control-Session": "a".repeat(64) }),
}));

import { executeCommand } from "./commands";
import { parseProductEventData } from "./events";

afterEach(() => vi.unstubAllGlobals());

describe("shared spine API", () => {
  it("preserves unknown additive event fields", () => {
    const event = parseProductEventData(JSON.stringify({
      id: "evt-1",
      namespace: "resource",
      type: "resource.created",
      source: "test",
      timestamp: 1,
      schema_version: 1,
      payload: { id: "res-1" },
      future_field: "preserved",
    }));
    expect(event?.type).toBe("resource.created");
    expect(event?.future_field).toBe("preserved");
  });

  it("posts a structured command with control-session authentication", async () => {
    const fetchMock = vi.fn(async () => new Response(JSON.stringify({
      request_id: "req-1",
      status: "succeeded",
      result: { simulated: true, provider: "mock", executed: false },
      schema_version: 1,
    }), { status: 200, headers: { "Content-Type": "application/json" } }));
    vi.stubGlobal("fetch", fetchMock);

    const result = await executeCommand("desktop.app.open", { app: "notepad" }, "req-1");

    expect(result?.result?.simulated).toBe(true);
    expect(fetchMock).toHaveBeenCalledWith(expect.stringContaining("/api/commands"), expect.objectContaining({
      method: "POST",
      headers: expect.objectContaining({ "X-Yilian-Control-Session": "a".repeat(64) }),
    }));
  });
});
