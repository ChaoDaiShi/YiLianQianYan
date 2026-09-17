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
  updated_at: 1,
};

describe("canvas view write queue", () => {
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
