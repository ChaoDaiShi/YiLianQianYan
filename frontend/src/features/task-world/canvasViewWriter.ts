import type { ApiError, ApiResult, CanvasView } from "../../api/taskWorld";

type CanvasViewUpdate = (current: CanvasView) => CanvasView;

interface CanvasViewWriteQueueOptions {
  readCurrent: () => CanvasView | null;
  acceptCurrent: (view: CanvasView) => void;
  save: (view: CanvasView) => Promise<ApiResult<CanvasView>>;
  onError: (error: ApiError) => Promise<void> | void;
}

export interface CanvasViewWriteQueue {
  enqueue: (update: CanvasViewUpdate) => Promise<void>;
}

export function createCanvasViewWriteQueue({
  readCurrent,
  acceptCurrent,
  save,
  onError,
}: CanvasViewWriteQueueOptions): CanvasViewWriteQueue {
  let tail = Promise.resolve();

  return {
    enqueue(update) {
      const write = tail.then(async () => {
        const current = readCurrent();
        if (!current) return;
        const next = update(current);
        if (next === current) return;

        const result = await save(next);
        if (result.ok) acceptCurrent(result.data);
        else await onError(result.error);
      });
      // Keep subsequent writes live even if an unexpected callback failure is
      // surfaced to the caller of this specific enqueue operation.
      tail = write.catch(() => undefined);
      return write;
    },
  };
}
