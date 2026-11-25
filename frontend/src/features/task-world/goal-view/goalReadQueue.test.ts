import { describe, expect, it, vi } from "vitest";
import { createGoalReadQueue } from "./goalReadQueue";

describe("goal observation read queue", () => {
  it("coalesces events received during a slow read into one fresh read", async () => {
    let release!: () => void;
    const gate = new Promise<void>(resolve => { release = resolve; });
    const read = vi.fn().mockImplementationOnce(() => gate).mockResolvedValue(undefined);
    const queue = createGoalReadQueue(read);
    const first = queue.refresh();
    void queue.refresh(); // a polling tick does not duplicate the current request
    void queue.refresh(false, true); void queue.refresh(false, true);
    expect(read).toHaveBeenCalledTimes(1);
    release(); await first;
    expect(read).toHaveBeenCalledTimes(2);
  });
  it("preserves an explicit revalidation queued behind an existing read", async () => {
    let release!: () => void;
    const read = vi.fn().mockImplementationOnce(() => new Promise<void>(resolve => { release = resolve; })).mockResolvedValue(undefined);
    const queue = createGoalReadQueue(read); const first = queue.refresh();
    void queue.refresh(true); release(); await first;
    expect(read.mock.calls.map(call => call[0])).toEqual([false, true]);
  });
  it("aborts observation on close and never follows up with another request", async () => {
    let release!: () => void;
    const read = vi.fn().mockImplementation(() => new Promise<void>(resolve => { release = resolve; }));
    const queue = createGoalReadQueue(read); const first = queue.refresh();
    void queue.refresh(false, true); queue.close(); release(); await first; await queue.refresh(true);
    expect(read).toHaveBeenCalledTimes(1); expect(read.mock.calls[0][1].aborted).toBe(true);
  });
});
