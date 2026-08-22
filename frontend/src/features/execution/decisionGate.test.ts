import { describe, expect, it, vi } from "vitest";
import { createDecisionGate } from "./decisionGate";

describe("createDecisionGate", () => {
  it("deduplicates an in-flight approval decision", async () => {
    let release!: () => void;
    const run = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        })
    );
    const gate = createDecisionGate(run);
    const first = gate.submit("approval-1", "approve");
    const second = gate.submit("approval-1", "approve");
    expect(run).toHaveBeenCalledTimes(1);
    release();
    await Promise.all([first, second]);
  });

  it("prevents conflicting decisions while one decision is in flight", async () => {
    let release!: () => void;
    const run = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        })
    );
    const gate = createDecisionGate(run);
    const approve = gate.submit("approval-1", "approve");
    const reject = gate.submit("approval-1", "reject");
    expect(run).toHaveBeenCalledTimes(1);
    release();
    await Promise.all([approve, reject]);
  });

  it("allows retry after a failed request", async () => {
    const run = vi
      .fn()
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce(undefined);
    const gate = createDecisionGate(run);
    await expect(gate.submit("approval-1", "reject")).rejects.toThrow(
      "offline"
    );
    await expect(
      gate.submit("approval-1", "reject")
    ).resolves.toBeUndefined();
    expect(run).toHaveBeenCalledTimes(2);
  });
});
