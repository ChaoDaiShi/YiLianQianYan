import { describe, expect, it, vi } from "vitest";
import { resolveCurrentTaskDestination } from "./currentTaskEntry";

describe("current task entry", () => {
  it("opens the task center when there is no bound task", async () => {
    const load = vi.fn();
    expect(await resolveCurrentTaskDestination(null, load)).toBe("/capabilities?category=workflow&workflow_tab=taskgraphs");
    expect(load).not.toHaveBeenCalled();
  });
  it("opens the persisted bound graph using an encoded route", async () => {
    const load = vi.fn().mockResolvedValue({ ok: true, data: { graph_id: "graph/one" } });
    expect(await resolveCurrentTaskDestination("graph/one", load)).toBe("/task-world/graph%2Fone");
    expect(load).toHaveBeenCalledWith("graph/one");
  });
  it("returns to the task center when the graph no longer exists", async () => {
    expect(await resolveCurrentTaskDestination("deleted", vi.fn().mockResolvedValue({ ok: false }))).toBe("/capabilities?category=workflow&workflow_tab=taskgraphs");
  });
  it("does not open a different graph returned by the lookup", async () => {
    expect(await resolveCurrentTaskDestination("one", vi.fn().mockResolvedValue({ ok: true, data: { graph_id: "two" } }))).toBe("/capabilities?category=workflow&workflow_tab=taskgraphs");
  });
});
