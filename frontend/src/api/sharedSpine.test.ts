import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("./controlSession", () => ({
  controlSessionHeaders: () => ({ "X-Yilian-Control-Session": "a".repeat(64) }),
}));

import { executeCommand } from "./commands";
import { parseProductEventData, subscribeToEvents } from "./events";

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

  it("receives a backend-formatted SSE product event", async () => {
    const productEvent = {
      id: "evt-sse-1",
      namespace: "resource",
      type: "resource.created",
      source: "resource-core",
      timestamp: 1,
      schema_version: 1,
      payload: { id: "res-sse-1" },
    };
    vi.stubGlobal("fetch", vi.fn(async () => new Response(
      `id: evt-sse-1\nevent: resource.created\ndata: ${JSON.stringify(productEvent)}\n\n`,
      { status: 200, headers: { "Content-Type": "text/event-stream" } },
    )));

    const received = await new Promise<string>((resolve, reject) => {
      const controller = subscribeToEvents(
        (event) => {
          controller.abort();
          resolve(event.payload.id as string);
        },
        reject,
      );
    });

    expect(received).toBe("res-sse-1");
  });
});
