import { describe, expect, it, vi } from "vitest";
import type { ApiError, ApiResult, CanvasView } from "../../api/taskWorld";
import { createCanvasViewWriteQueue } from "./canvasViewWriter";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => { resolve = next; });
  return { promise, resolve };
}

const initialView: CanvasView = {
  schema_version: 1,
  graph_id: "graph-1",
  view_revision: 1,
  graph_revision_seen: 1,
  viewport: { x: 0, y: 0, zoom: 1 },
  node_layouts: [],
  selection: [],
  groups: [],
  updated_at: 1,
};

describe("canvas view write queue", () => {
  const stale: ApiResult<CanvasView> = { ok: false, error: { status: 409, code: "stale_view_revision", message: "conflict" } };
  const move = (view: CanvasView): CanvasView => ({ ...view, node_layouts: [{ node_id: "A", x: 500, y: 40, width: 240, height: 128 }] });

  it("rebases pending x=500 on the latest x=100 document and retries exactly once", async () => {
    let current = initialView;
    const latest = { ...initialView, view_revision: 8, selection: ["B"], node_layouts: [{ node_id: "A", x: 100, y: 40, width: 240, height: 128 }] };
    const writes: CanvasView[] = [];
    const errors: ApiError[] = [];
    const queue = createCanvasViewWriteQueue({
      readCurrent: () => current, acceptCurrent: (view) => { current = view; },
      loadLatest: async () => ({ ok: true, data: latest }),
      save: async (view) => { writes.push(view); return writes.length === 1 ? stale : { ok: true, data: { ...view, view_revision: 9 } }; },
      onError: (error) => { errors.push(error); },
    });
    await queue.enqueue(move);
    expect(writes).toHaveLength(2);
    expect(writes[1]).toMatchObject({ view_revision: 8, selection: ["B"], node_layouts: [{ node_id: "A", x: 500 }] });
    expect(current.node_layouts[0].x).toBe(500);
    expect(errors).toEqual([]);
  });

  it("retains failed mutations and later writes until an explicit retry", async () => {
    let current = initialView;
    let conflict = true;
    const writes: CanvasView[] = [];
    const errors: ApiError[] = [];
    const queue = createCanvasViewWriteQueue({
      readCurrent: () => current, acceptCurrent: (view) => { current = view; },
      loadLatest: async () => ({ ok: true, data: { ...current, view_revision: 8 } }),
      save: async (view) => { writes.push(view); return conflict ? stale : { ok: true, data: { ...view, view_revision: view.view_revision + 1 } }; },
      onError: (error) => { errors.push(error); },
    });
    await queue.enqueue(move);
    expect(writes).toHaveLength(2);
    expect(errors[0].message).toBe("画布布局保存发生冲突，请重试");
    await queue.enqueue((view) => ({ ...view, viewport: { x: 340, y: -120, zoom: 1.35 } }));
    expect(writes).toHaveLength(2);
    conflict = false;
    await queue.retry();
    expect(current.node_layouts[0].x).toBe(500);
    expect(current.viewport).toEqual({ x: 340, y: -120, zoom: 1.35 });
    expect(writes).toHaveLength(4);
  });

  it("surfaces failed latest-document fetch without retrying an obsolete document", async () => {
    const onError = vi.fn();
    const save = vi.fn(async () => stale);
    const queue = createCanvasViewWriteQueue({
      readCurrent: () => initialView, acceptCurrent: () => {}, save,
      loadLatest: async () => ({ ok: false, error: { status: 503, code: "unavailable", message: "offline" } }), onError,
    });
    await queue.enqueue(move);
    expect(save).toHaveBeenCalledTimes(1);
    expect(onError).toHaveBeenCalledWith(expect.objectContaining({ code: "unavailable" }));
  });

  it("does not overwrite a newer SSE document with the stale-fetch response", async () => {
    let current = initialView;
    const writes: CanvasView[] = [];
    const queue = createCanvasViewWriteQueue({
      readCurrent: () => current,
      acceptCurrent: (view) => { if (view.view_revision >= current.view_revision) current = view; },
      loadLatest: async () => { current = { ...initialView, view_revision: 10, selection: ["C"] }; return { ok: true, data: { ...initialView, view_revision: 8 } }; },
      save: async (view) => { writes.push(view); return writes.length === 1 ? stale : { ok: true, data: { ...view, view_revision: 11 } }; },
      onError: () => {},
    });
    await queue.enqueue(move);
    expect(writes[1]).toMatchObject({ view_revision: 10, selection: ["C"] });
  });

  it("serializes updates against the latest accepted view revision", async () => {
    let current = initialView;
    const firstSave = deferred<ApiResult<CanvasView>>();
    const save = vi
      .fn<(view: CanvasView) => Promise<ApiResult<CanvasView>>>()
      .mockReturnValueOnce(firstSave.promise)
      .mockImplementation(async (view) => ({
        ok: true,
        data: { ...view, view_revision: view.view_revision + 1 },
      }));
    const onError = vi.fn<(error: ApiError) => Promise<void>>();
    const queue = createCanvasViewWriteQueue({
      readCurrent: () => current,
      acceptCurrent: (view) => { current = view; },
      save,
      onError,
    });

    const viewportWrite = queue.enqueue((view) => ({
      ...view,
      viewport: { x: 40, y: -20, zoom: 1.2 },
    }));
    const selectionWrite = queue.enqueue((view) => ({
      ...view,
      selection: ["node-1"],
    }));

    await Promise.resolve();
    expect(save).toHaveBeenCalledTimes(1);
    expect(save.mock.calls[0][0].view_revision).toBe(1);
    firstSave.resolve({
      ok: true,
      data: {
        ...initialView,
        view_revision: 2,
        viewport: { x: 40, y: -20, zoom: 1.2 },
      },
    });

    await Promise.all([viewportWrite, selectionWrite]);

    expect(save).toHaveBeenCalledTimes(2);
    expect(save.mock.calls[1][0]).toMatchObject({
      view_revision: 2,
      viewport: { x: 40, y: -20, zoom: 1.2 },
      selection: ["node-1"],
    });
    expect(current.view_revision).toBe(3);
    expect(onError).not.toHaveBeenCalled();
  });
});
