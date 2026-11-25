import { afterEach, describe, expect, it, vi } from "vitest";
import { subscribeToEvents } from "./events";
vi.mock("./controlSession", () => ({ controlSessionHeaders: () => ({}) }));
const event = { id: "one", namespace: "task", type: "task.goal.changed", source: "db", timestamp: 1, schema_version: 1, payload: { graph_id: "g" } };
afterEach(() => { vi.unstubAllGlobals(); vi.useRealTimers(); });

describe("product event connection lifecycle", () => {
  it("reconnects after EOF and reports a new connected state", async () => {
    vi.useFakeTimers();
    const receive = vi.fn(), states = vi.fn();
    const fetcher = vi.fn().mockResolvedValueOnce(new Response(`data: ${JSON.stringify(event)}\n\n`))
      .mockResolvedValue(new Response(new ReadableStream()));
    vi.stubGlobal("fetch", fetcher);
    const controller = subscribeToEvents(receive, vi.fn(), states);
    await vi.advanceTimersByTimeAsync(1200);
    expect(receive).toHaveBeenCalledWith(event);
    expect(fetcher).toHaveBeenCalledTimes(2);
    expect(fetcher.mock.calls[1][0]).toMatch(/\/api\/events\?after=one$/);
    expect(states.mock.calls.filter(([state]) => state === "connected")).toHaveLength(2);
    controller.abort();
  });
  it("stops reconnecting when its owner unmounts", async () => {
    vi.useFakeTimers();
    const fetcher = vi.fn().mockRejectedValue(new Error("offline")); vi.stubGlobal("fetch", fetcher);
    const controller = subscribeToEvents(vi.fn());
    await vi.advanceTimersByTimeAsync(0); controller.abort();
    await vi.advanceTimersByTimeAsync(20000);
    expect(fetcher).toHaveBeenCalledTimes(1);
  });
  it("reports permission failure without retrying the denied request", async () => {
    vi.useFakeTimers();
    const fetcher = vi.fn().mockResolvedValue(new Response("", { status: 403 })); vi.stubGlobal("fetch", fetcher);
    const error = vi.fn(), states = vi.fn(); const controller = subscribeToEvents(vi.fn(), error, states);
    await vi.advanceTimersByTimeAsync(20000);
    expect(fetcher).toHaveBeenCalledTimes(1); expect(error).toHaveBeenCalled();
    expect(states).toHaveBeenCalledWith("unavailable"); controller.abort();
  });
});
