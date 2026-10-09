export interface CaptureOperationHandle {
  lease?: { sessionId: string; generation: number; leaseId: string };
  stop: () => void;
  cancel: () => void;
}

export function bindCaptureOperation(
  isCurrent: () => boolean,
  controls: () => CaptureOperationHandle,
): CaptureOperationHandle {
  return {
    stop: () => { if (isCurrent()) controls().stop(); },
    cancel: () => { if (isCurrent()) controls().cancel(); },
  };
}
