import { expect, it, vi } from "vitest";
import { RefreshGate } from "./refreshGate";
it("does not poll hidden, overlapping or too-frequent background work", async () => {
  const gate = new RefreshGate();
  let finish!: () => void;
  const refresh = vi.fn(() => new Promise<void>((resolve) => { finish = resolve; }));
  expect(await gate.run(0, false, refresh)).toBe(false);
  const pending = gate.run(0, true, refresh);
  expect(await gate.run(30_000, true, refresh)).toBe(false);
  finish(); await pending;
  expect(await gate.run(10_000, true, refresh)).toBe(false);
  expect(refresh).toHaveBeenCalledOnce();
});
