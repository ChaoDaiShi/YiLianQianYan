import type { ApiError, ApiResult, CanvasView } from "../../api/taskWorld";

type CanvasViewUpdate = (current: CanvasView) => CanvasView;

interface CanvasViewWriteQueueOptions {
  readCurrent: () => CanvasView | null;
  acceptCurrent: (view: CanvasView) => void;
  save: (view: CanvasView) => Promise<ApiResult<CanvasView>>;
  loadLatest?: () => Promise<ApiResult<CanvasView>>;
  onError: (error: ApiError) => Promise<void> | void;
  onSaved?: () => void;
}

export interface CanvasViewWriteQueue {
  enqueue: (update: CanvasViewUpdate) => Promise<void>;
  retry: () => Promise<void>;
}

export function createCanvasViewWriteQueue({
  readCurrent,
  acceptCurrent,
  save,
  loadLatest,
  onError,
  onSaved,
}: CanvasViewWriteQueueOptions): CanvasViewWriteQueue {
  let tail = Promise.resolve();
  const pending: CanvasViewUpdate[] = [];
  let blocked = false;

  async function fail(error: ApiError) {
    blocked = true;
    await onError(error.code === "stale_view_revision"
      ? { ...error, message: "画布布局保存发生冲突，请重试" } : error);
  }

  async function flush() {
    while (pending.length && !blocked) {
      const current = readCurrent();
      if (!current) return;
      const update = pending[0];
      const next = update(current);
      if (next === current) { pending.shift(); continue; }
      let result = await save(next);
      if (!result.ok && result.error.code === "stale_view_revision" && loadLatest) {
        const latest = await loadLatest();
        if (!latest.ok) { await fail(latest.error); return; }
        acceptCurrent(latest.data);
        // A newer SSE view may already have won while this fetch was pending.
        const base = readCurrent() ?? latest.data;
        result = await save(update(base));
      }
      if (!result.ok) { await fail(result.error); return; }
      acceptCurrent(result.data);
      pending.shift();
    }
    if (!pending.length) onSaved?.();
  }

  function schedule() {
    const write = tail.then(flush).catch(async () => {
      await fail({ status: 0, code: "canvas_save_failed", message: "画布保存失败，本地修改已保留，请重试" });
    });
    tail = write.catch(() => undefined);
    return write;
  }

  return {
    enqueue(update) {
      pending.push(update);
      return schedule();
    },
    retry() {
      blocked = false;
      return schedule();
    },
  };
}
