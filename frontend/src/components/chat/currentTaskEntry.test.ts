import { describe, expect, it, vi } from "vitest";
import { resolveCurrentTaskDestination } from "./currentTaskEntry";

describe("current task avatar destination", () => {
  it("uses the task center when there is no current graph", async () => {
    const load = vi.fn();
    expect(await resolveCurrentTaskDestination(null, load)).toBe("/tasks");
    expect(load).not.toHaveBeenCalled();
  });

  it("opens only the same graph confirmed by the backend", async () => {
    const load = vi.fn().mockResolvedValue({ ok: true, data: { graph_id: "graph-1" } });
    expect(await resolveCurrentTaskDestination("graph-1", load)).toBe("/task-world/graph-1");
  });

  it("falls back instead of navigating a missing, mismatched, or failed graph", async () => {
    expect(await resolveCurrentTaskDestination("gone", async () => ({ ok: false }))).toBe("/tasks");
    expect(await resolveCurrentTaskDestination("stale", async () => ({
      ok: true,
      data: { graph_id: "other" },
    }))).toBe("/tasks");
    expect(await resolveCurrentTaskDestination("error", async () => {
      throw new Error("offline");
    })).toBe("/tasks");
  });
});
