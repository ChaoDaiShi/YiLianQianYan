import { describe, expect, it, vi } from "vitest";
import type { PointerEvent, MouseEvent } from "react";
import { createFloatingDragHandlers } from "./floatingSurface";

function pointer(x: number, y: number, button = 0) {
  return {
    clientX: x, clientY: y, button, pointerId: 1,
    currentTarget: { setPointerCapture: vi.fn(), releasePointerCapture: vi.fn() },
    preventDefault: vi.fn(),
  } as unknown as PointerEvent<HTMLElement>;
}
function click(detail = 1) {
  return { detail, preventDefault: vi.fn(), stopPropagation: vi.fn() } as unknown as MouseEvent<HTMLElement>;
}
describe("native floating-window gestures", () => {
  it("keeps a click and small pointer jitter as one expand action", () => {
    const activate = vi.fn(); const startDrag = vi.fn().mockResolvedValue(undefined);
    const gesture = createFloatingDragHandlers({ activate, startDrag, onError: vi.fn() });
    gesture.onPointerDown(pointer(20, 20));
    gesture.onPointerMove(pointer(23, 22));
    gesture.onPointerUp(pointer(23, 22));
    gesture.onClick(click());
    expect(startDrag).not.toHaveBeenCalled();
    expect(activate).toHaveBeenCalledTimes(1);
  });
  it("starts native dragging once after movement and suppresses drag-end expansion", () => {
    const activate = vi.fn(); const startDrag = vi.fn().mockResolvedValue(undefined);
    const gesture = createFloatingDragHandlers({ activate, startDrag, onError: vi.fn() });
    gesture.onPointerDown(pointer(20, 20));
    gesture.onPointerMove(pointer(26, 20));
    gesture.onPointerMove(pointer(40, 30));
    gesture.onPointerUp(pointer(40, 30));
    const releaseClick = click(); gesture.onClick(releaseClick);
    expect(startDrag).toHaveBeenCalledTimes(1);
    expect(activate).not.toHaveBeenCalled();
    expect(releaseClick.preventDefault).toHaveBeenCalled();
    gesture.onPointerDown(pointer(40, 30));
    gesture.onPointerUp(pointer(40, 30));
    gesture.onClick(click());
    expect(activate).toHaveBeenCalledTimes(1);
  });
  it("leaves keyboard activation available after dragging and reports native drag errors", async () => {
    const error = new Error("native drag denied"); const onError = vi.fn(); const activate = vi.fn();
    const gesture = createFloatingDragHandlers({ activate, startDrag: vi.fn().mockRejectedValue(error), onError });
    gesture.onPointerDown(pointer(0, 0)); gesture.onPointerMove(pointer(10, 0));
    await Promise.resolve();
    expect(onError).toHaveBeenCalledWith(error);
    gesture.onClick(click(0)); expect(activate).toHaveBeenCalledTimes(1);
  });
  it("does not drag from a secondary button or a cancelled gesture", () => {
    const startDrag = vi.fn().mockResolvedValue(undefined);
    const gesture = createFloatingDragHandlers({ startDrag, onError: vi.fn() });
    gesture.onPointerDown(pointer(0, 0, 2)); gesture.onPointerMove(pointer(30, 30));
    gesture.onPointerDown(pointer(0, 0)); gesture.onPointerCancel(); gesture.onPointerMove(pointer(30, 30));
    expect(startDrag).not.toHaveBeenCalled();
  });
});
