import { describe, expect, it } from "vitest";
import { messageEdgeHandles } from "./messageGeometry";

describe("message card connection geometry", () => {
  it.each([
    [{ x: 0, y: 192 }, "out-bottom", "in-top"],
    [{ x: 0, y: -192 }, "out-top", "in-bottom"],
    [{ x: 324, y: 192 }, "out-right", "in-left"],
    [{ x: -324, y: 192 }, "out-left", "in-right"],
    [{ x: 180, y: 192 }, "out-bottom", "in-top"],
    [{ x: 324, y: 960 }, "out-bottom", "in-top"],
  ] as const)("uses facing edges for a child at %j", (target, sourceHandle, targetHandle) => {
    expect(messageEdgeHandles({ x: 0, y: 0 }, target)).toEqual({ sourceHandle, targetHandle });
  });
});
