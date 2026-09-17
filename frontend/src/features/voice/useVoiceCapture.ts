import { useCallback, useEffect, useRef, useState } from "react";
import {
  acquireVoiceInputLease,
  transcribeAudio,
  VoiceApiError,
  voiceErrorMessage,
  type GlobalVoiceSession,
  type VoiceInputLease,
} from "../../api/voice";

export type CaptureStatus = "idle" | "acquiring" | "listening" | "transcribing" | "error";

export interface CaptureState {
  status: CaptureStatus;
  error: string | null;
  lease: VoiceInputLease | null;
}

export interface FinalVoiceTranscript {
  text: string;
  sessionId: string;
  generation: number;
  leaseId: string;
}

export interface UseVoiceCaptureOptions {
  session: GlobalVoiceSession | null;
  onBargeIn?: () => Promise<GlobalVoiceSession | void> | GlobalVoiceSession | void;
  onFinal?: (transcript: FinalVoiceTranscript) => void;
  /** Retained for callers that render a partial field; capture never uploads partials. */
  onPartial?: (text: string) => void;
  onError?: (message: string) => void;
}

export const CAPTURE_CHUNK_INTERVAL_MS = 1_000;
export const MAX_CAPTURE_DURATION_MS = 60_000;
export const MAX_CAPTURE_BYTES = 8 * 1024 * 1024;
export const FINAL_TRANSCRIPTION_TIMEOUT_MS = 60_000;

/**
 * These are the low-cost containers allowed by the current ASR contract. The
 * actual browser intersection is selected at runtime; WebM is intentionally
 * not a fallback because it is not in the confirmed provider list.
 */
export const SUPPORTED_CAPTURE_MIME_TYPES = [
  "audio/mp4;codecs=mp4a.40.2",
  "audio/mp4",
  "audio/ogg;codecs=opus",
  "audio/ogg",
] as const;

export interface CaptureBuffer {
  chunks: Blob[];
  bytes: number;
}

export function selectRecorderMimeType(
  isTypeSupported: (mimeType: string) => boolean,
): string | null {
  for (const mimeType of SUPPORTED_CAPTURE_MIME_TYPES) {
    try {
      if (isTypeSupported(mimeType)) return mimeType;
    } catch {
      // A browser may reject an individual codec probe. Try the next one.
    }
  }
  return null;
}

export function appendCaptureChunk(
  buffer: CaptureBuffer,
  chunk: Blob,
  maxBytes: number = MAX_CAPTURE_BYTES,
): boolean {
  if (chunk.size === 0 || buffer.bytes + chunk.size > maxBytes) return false;
  buffer.chunks.push(chunk);
  buffer.bytes += chunk.size;
  return true;
}

export function isCaptureStartAllowed(
  status: CaptureStatus,
  hasRecorder: boolean,
  finalInFlight: boolean,
  acquireInFlight: boolean = false,
): boolean {
  return (
    !hasRecorder &&
    !finalInFlight &&
    !acquireInFlight &&
    status !== "acquiring" &&
    status !== "listening" &&
    status !== "transcribing"
  );
}

export function isCurrentCaptureOperation(
  operation: number,
  currentOperation: number,
  lease: VoiceInputLease,
  activeLease: VoiceInputLease | null,
  session: GlobalVoiceSession | null,
  cancelled: boolean,
): boolean {
  return (
    !cancelled &&
    operation === currentOperation &&
    activeLease?.lease_id === lease.lease_id &&
    activeLease.voice_session_id === lease.voice_session_id &&
    activeLease.generation === lease.generation &&
    session?.voice_session_id === lease.voice_session_id &&
    session.generation === lease.generation
  );
}

export function isExplicitMicrophoneStart(reason: string | undefined): boolean {
  return reason === "user-click" || reason === "user-keyboard";
}

/**
 * A new microphone lease must wait for the server-side interruption to settle.
 * The returned session is used when the interrupt advanced generation.
 */
export async function acquireAfterBargeIn<T>(
  onBargeIn: UseVoiceCaptureOptions["onBargeIn"],
  acquire: (session?: GlobalVoiceSession) => Promise<T>,
): Promise<T> {
  const updatedSession = await onBargeIn?.();
  return acquire(updatedSession && typeof updatedSession === "object" ? updatedSession : undefined);
}

export function describeCaptureError(error: unknown): string {
  if (error instanceof VoiceApiError) return voiceErrorMessage(error);
  const name =
    typeof error === "object" && error !== null && "name" in error
      ? String((error as { name?: unknown }).name)
      : "";
  if (name === "NotAllowedError" || name === "PermissionDeniedError") {
    return "麦克风权限未授予。请在系统设置中允许后重试。";
  }
  if (name === "NotFoundError" || name === "DevicesNotFoundError") {
    return "没有找到可用的麦克风设备。";
  }
  if (name === "AbortError") return "语音识别请求已取消。";
  if (error instanceof TypeError) return "语音识别暂时不可用，请检查网络后重试。";
  return "麦克风暂不可用，请重试。";
}

function isCaptureSupported(): boolean {
  return (
    typeof navigator !== "undefined" &&
    Boolean(navigator.mediaDevices?.getUserMedia) &&
    typeof MediaRecorder !== "undefined"
  );
}

function recorderMimeType(): string | null {
  if (typeof MediaRecorder === "undefined" || typeof MediaRecorder.isTypeSupported !== "function") {
    return null;
  }
  return selectRecorderMimeType((mimeType) => MediaRecorder.isTypeSupported(mimeType));
}

function extractTranscript(response: unknown): string | null {
  if (typeof response !== "object" || response === null) return null;
  const value = response as Record<string, unknown>;
  const transcript = value.transcript;
  if (typeof transcript === "object" && transcript !== null) {
    const text = (transcript as Record<string, unknown>).text;
    if (typeof text === "string" && text.trim()) return text.trim();
  }
  if (typeof value.text === "string" && value.text.trim()) return value.text.trim();
  return null;
}

export function useVoiceCapture({
  session,
  onBargeIn,
  onFinal,
  onError,
}: UseVoiceCaptureOptions): UseVoiceCaptureResult {
  const [state, setState] = useState<CaptureState>({ status: "idle", error: null, lease: null });
  const recorderRef = useRef<MediaRecorder | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const captureBufferRef = useRef<CaptureBuffer>({ chunks: [], bytes: 0 });
  const leaseRef = useRef<VoiceInputLease | null>(null);
  const controllerRef = useRef<AbortController | null>(null);
  const captureOperationRef = useRef(0);
  const acquireInFlightRef = useRef(false);
  const finalInFlightRef = useRef(false);
  const captureTimerRef = useRef<number | null>(null);
  const captureLimitReachedRef = useRef(false);
  const cancelledRef = useRef(false);
  const latestSessionRef = useRef<GlobalVoiceSession | null>(session);
  latestSessionRef.current = session;

  const clearCaptureTimer = useCallback(() => {
    if (captureTimerRef.current !== null) {
      window.clearTimeout(captureTimerRef.current);
      captureTimerRef.current = null;
    }
  }, []);

  const releaseStream = useCallback(() => {
    clearCaptureTimer();
    streamRef.current?.getTracks().forEach((track) => track.stop());
    streamRef.current = null;
    recorderRef.current = null;
    captureBufferRef.current = { chunks: [], bytes: 0 };
    captureLimitReachedRef.current = false;
  }, [clearCaptureTimer]);

  const isCurrentOperation = useCallback((operation: number, lease: VoiceInputLease): boolean => {
    return isCurrentCaptureOperation(
      operation,
      captureOperationRef.current,
      lease,
      leaseRef.current,
      latestSessionRef.current,
      cancelledRef.current,
    );
  }, []);

  const isCurrentOperationId = useCallback((operation: number): boolean => {
    return operation === captureOperationRef.current && !cancelledRef.current;
  }, []);

  const clearAcquireInFlight = useCallback((operation: number): void => {
    if (operation === captureOperationRef.current) acquireInFlightRef.current = false;
  }, []);

  const reset = useCallback(() => {
    leaseRef.current = null;
    setState({ status: "idle", error: null, lease: null });
  }, []);

  const fail = useCallback(
    (error: unknown) => {
      cancelledRef.current = true;
      captureOperationRef.current += 1;
      const message = typeof error === "string" ? error : describeCaptureError(error);
      releaseStream();
      leaseRef.current = null;
      finalInFlightRef.current = false;
      if (controllerRef.current && !controllerRef.current.signal.aborted) {
        controllerRef.current.abort();
      }
      controllerRef.current = null;
      setState({ status: "error", error: message, lease: null });
      onError?.(message);
    },
    [onError, releaseStream],
  );

  const finalizeRecording = useCallback(
    async (
      lease: VoiceInputLease,
      controller: AbortController,
      mediaType: string,
      operation: number,
    ) => {
      if (!isCurrentOperation(operation, lease) || controller.signal.aborted) return;
      finalInFlightRef.current = true;
      const chunks = captureBufferRef.current.chunks.slice();
      const blob = new Blob(chunks, { type: mediaType });
      releaseStream();
      try {
        if (blob.size === 0) {
          fail("没有录到声音，请重试。");
          return;
        }
        setState((current) => ({ ...current, status: "transcribing", error: null }));
        let timedOut = false;
        const timeoutId = window.setTimeout(() => {
          timedOut = true;
          controller.abort();
        }, FINAL_TRANSCRIPTION_TIMEOUT_MS);
        try {
          const response = await transcribeAudio(
            blob,
            lease.voice_session_id,
            lease.generation,
            lease.lease_id,
            controller.signal,
          );
          if (!isCurrentOperation(operation, lease)) return;
          if (controller.signal.aborted) {
            if (timedOut) fail(new VoiceApiError("STT_TIMEOUT"));
            return;
          }
          const text = extractTranscript(response);
          if (!text) {
            fail("语音服务没有返回可用文字，请重试。");
            return;
          }
          onFinal?.({
            text,
            sessionId: lease.voice_session_id,
            generation: lease.generation,
            leaseId: lease.lease_id,
          });
          reset();
        } catch (error) {
          if (!isCurrentOperation(operation, lease)) return;
          if (timedOut) {
            fail(new VoiceApiError("STT_TIMEOUT"));
          } else if (!controller.signal.aborted) {
            fail(error);
          }
        } finally {
          window.clearTimeout(timeoutId);
        }
      } finally {
        if (controllerRef.current === controller) controllerRef.current = null;
        finalInFlightRef.current = false;
      }
    },
    [fail, isCurrentOperation, onFinal, releaseStream, reset],
  );

  const start = useCallback(
    async (reason: string) => {
      if (
        !isCaptureStartAllowed(
          state.status,
          Boolean(recorderRef.current),
          finalInFlightRef.current,
          acquireInFlightRef.current,
        )
      ) {
        return;
      }
      if (!isExplicitMicrophoneStart(reason)) {
        fail("请点击或使用明确的快捷键开始麦克风输入。");
        return;
      }
      if (!session || session.state === "ended") {
        fail("语音会话尚未准备好，请先开始语音会话。");
        return;
      }
      if (!isCaptureSupported()) {
        fail("当前环境不支持麦克风录音。");
        return;
      }
      const mimeType = recorderMimeType();
      if (!mimeType) {
        fail("当前环境不支持语音服务可接受的录音格式。");
        return;
      }

      const operation = captureOperationRef.current + 1;
      captureOperationRef.current = operation;
      acquireInFlightRef.current = true;
      const controller = new AbortController();
      controllerRef.current = controller;
      cancelledRef.current = false;
      captureBufferRef.current = { chunks: [], bytes: 0 };
      captureLimitReachedRef.current = false;
      setState({ status: "acquiring", error: null, lease: null });
      try {
        const lease = await acquireAfterBargeIn(onBargeIn, (interruptedSession) => {
          const leaseSession = interruptedSession ?? session;
          return acquireVoiceInputLease(
            leaseSession.voice_session_id,
            leaseSession.generation,
            "builtin_asr",
            controller.signal,
          );
        });
        if (!lease) {
          clearAcquireInFlight(operation);
          if (isCurrentOperationId(operation)) fail("无法取得麦克风输入租约，请重试。");
          return;
        }
        if (!isCurrentOperationId(operation) || controller.signal.aborted) {
          clearAcquireInFlight(operation);
          return;
        }
        leaseRef.current = lease;
        setState({ status: "acquiring", error: null, lease });
        const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
        if (!isCurrentOperationId(operation) || controller.signal.aborted) {
          clearAcquireInFlight(operation);
          stream.getTracks().forEach((track) => track.stop());
          return;
        }
        streamRef.current = stream;
        const recorder = new MediaRecorder(stream, { mimeType });
        recorderRef.current = recorder;
        captureBufferRef.current = { chunks: [], bytes: 0 };
        recorder.ondataavailable = (event) => {
          if (!isCurrentOperationId(operation) || captureLimitReachedRef.current) return;
          if (event.data.size === 0) return;
          if (!appendCaptureChunk(captureBufferRef.current, event.data)) {
            captureLimitReachedRef.current = true;
            if (recorder.state === "recording") {
              setState((current) => ({ ...current, status: "transcribing", error: null }));
              recorder.stop();
            }
          }
        };
        recorder.onstop = () => {
          void finalizeRecording(lease, controller, recorder.mimeType || mimeType, operation);
        };
        recorder.onerror = () => {
          if (isCurrentOperationId(operation)) fail("麦克风录音失败，请重试。");
        };
        recorder.start(CAPTURE_CHUNK_INTERVAL_MS);
        captureTimerRef.current = window.setTimeout(() => {
          if (!isCurrentOperationId(operation)) return;
          captureLimitReachedRef.current = true;
          if (recorder.state === "recording") {
            setState((current) => ({ ...current, status: "transcribing", error: null }));
            recorder.stop();
          }
        }, MAX_CAPTURE_DURATION_MS);
        clearAcquireInFlight(operation);
        setState({ status: "listening", error: null, lease });
      } catch (error) {
        clearAcquireInFlight(operation);
        if (isCurrentOperationId(operation) && !controller.signal.aborted) fail(error);
      }
    },
    [
      clearAcquireInFlight,
      fail,
      finalizeRecording,
      isCurrentOperationId,
      onBargeIn,
      session,
      state.status,
    ],
  );

  const stop = useCallback(() => {
    const recorder = recorderRef.current;
    if (!recorder || recorder.state === "inactive") return;
    setState((current) => ({ ...current, status: "transcribing", error: null }));
    try {
      recorder.stop();
    } catch (error) {
      fail(error);
    }
  }, [fail]);

  const invalidateCapture = useCallback(() => {
    cancelledRef.current = true;
    captureOperationRef.current += 1;
    controllerRef.current?.abort();
    controllerRef.current = null;
    const recorder = recorderRef.current;
    if (recorder && recorder.state !== "inactive") {
      try {
        recorder.stop();
      } catch {
        // The stream cleanup below is authoritative if stop races with teardown.
      }
    }
    releaseStream();
    leaseRef.current = null;
    acquireInFlightRef.current = false;
    finalInFlightRef.current = false;
  }, [releaseStream]);

  const cancel = useCallback(() => {
    invalidateCapture();
    reset();
  }, [invalidateCapture, reset]);

  useEffect(() => {
    const lease = leaseRef.current;
    if (
      lease &&
      (!session ||
        session.voice_session_id !== lease.voice_session_id ||
        session.generation !== lease.generation)
    ) {
      invalidateCapture();
      reset();
    }
  }, [invalidateCapture, reset, session?.generation, session?.voice_session_id]);

  useEffect(() => {
    return () => {
      invalidateCapture();
    };
  }, [invalidateCapture]);

  return { state, start, stop, cancel };
}

export interface UseVoiceCaptureResult {
  state: CaptureState;
  start: (reason: string) => Promise<void>;
  stop: () => void;
  cancel: () => void;
}
