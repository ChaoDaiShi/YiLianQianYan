import { expect, it, vi } from "vitest";
import { bindCaptureOperation } from "./captureOperation";

it("binds stop/cancel to its capture operation and lease generation", () => {
  let operation = 1;
  let generation = 3;
  const controls = { stop: vi.fn(), cancel: vi.fn() };
  const handle = bindCaptureOperation(() => operation === 1 && generation === 3, () => controls);
  handle.stop();
  expect(controls.stop).toHaveBeenCalledOnce();
  operation = 2;
  handle.stop(); handle.cancel();
  expect(controls.stop).toHaveBeenCalledOnce();
  expect(controls.cancel).not.toHaveBeenCalled();
  operation = 1; generation = 4;
  handle.cancel();
  expect(controls.cancel).not.toHaveBeenCalled();
});
