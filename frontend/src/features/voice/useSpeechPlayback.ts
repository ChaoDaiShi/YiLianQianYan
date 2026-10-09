import { useCallback, useEffect, useRef, useState } from "react";
import { readPlaybackOverlap, type PlaybackOverlap } from "./echoEvidence";
import {
  markVoiceSpeechFinished,
  markVoiceSpeechStarted,
  synthesizeSpeech,
  type SpeechPlayback,
} from "../../api/voice";

export type PlaybackStatus =
  | "idle"
  | "synthesizing"
  | "playing"
  | "paused"
  | "ended"
  | "error";

export interface PlaybackState {
  status: PlaybackStatus;
  muted: boolean;
}

export type PlaybackAction =
  | "pause"
  | "replay"
  | "mute"
  | "interrupt"
  | "ended"
  | "provider_unavailable";

export function reducePlaybackState(state: PlaybackState, action: PlaybackAction): PlaybackState {
  switch (action) {
    case "pause":
      return state.status === "playing" ? { ...state, status: "paused" } : state;
    case "replay":
      return state.status === "paused" || state.status === "ended"
        ? { ...state, status: "playing" }
        : state;
    case "mute":
      return { ...state, muted: !state.muted };
    case "interrupt":
      return { ...state, status: "idle" };
    case "ended":
      return { ...state, status: "ended" };
    case "provider_unavailable":
      return { ...state, status: "error" };
  }
}

export function shouldAcceptSpeechGeneration(
  activeGeneration: number,
  responseGeneration: number | undefined,
): boolean {
  return responseGeneration === undefined || responseGeneration === activeGeneration;
}

export function shouldDispatchFinalTranscript(phase: "partial" | "final"): boolean {
  return phase === "final";
}

/** Stop the native audio element before a new capture or speech request starts. */
export function interruptAudioElement(audio: HTMLAudioElement | null): void {
  if (!audio) return;
  audio.pause();
  audio.currentTime = 0;
  audio.removeAttribute("src");
  audio.load();
}

export async function replayAudioIfCurrent(
  audio: HTMLAudioElement,
  isCurrent: () => boolean,
  reportStarted: () => Promise<boolean>,
  releaseRejected: () => void = () => undefined,
): Promise<boolean> {
  try {
    if (isCurrent()) {
      await audio.play();
      if (isCurrent() && await reportStarted() && isCurrent()) return true;
    }
  } catch {
    // An unacknowledged replay must never continue producing audio.
  }
  try {
    interruptAudioElement(audio);
  } finally {
    releaseRejected();
  }
  return false;
}

export interface VoiceCleanupOptions {
  sessionId: string;
  generation: number;
  cancelCapture: () => void;
  interruptPlayback: () => void;
  endSession: (sessionId: string, generation: number) => Promise<unknown> | unknown;
}

export async function cleanupVoiceSession(options: VoiceCleanupOptions): Promise<void> {
  options.cancelCapture();
  options.interruptPlayback();
  await options.endSession(options.sessionId, options.generation);
}

export interface UseSpeechPlaybackOptions {
  sessionId?: string | null;
  generation?: number | null;
  onError?: (message: string) => void;
}

export interface UseSpeechPlaybackResult {
  state: PlaybackState;
  speak: (text: string) => Promise<void>;
  pause: () => void;
  replay: () => void;
  toggleMute: () => void;
  interrupt: () => void;
  captureOverlap: () => PlaybackOverlap | null;
}

function playbackErrorMessage(error: unknown): string {
  if (error instanceof DOMException && error.name === "NotAllowedError") {
    return "浏览器阻止了自动播放，请点击播放按钮重试。";
  }
  return "语音播放暂不可用，请检查语音服务配置。";
}

export function useSpeechPlayback({
  sessionId = null,
  generation = null,
  onError,
}: UseSpeechPlaybackOptions = {}): UseSpeechPlaybackResult {
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const objectUrlRef = useRef<string | null>(null);
  const requestControllerRef = useRef<AbortController | null>(null);
  const spokenRef = useRef<{ text: string; sessionId: string; generation: number } | null>(null);
  const latestRef = useRef({ sessionId, generation });
  const [state, setState] = useState<PlaybackState>({ status: "idle", muted: false });

  useEffect(() => {
    latestRef.current = { sessionId, generation };
  }, [generation, sessionId]);

  const releaseAudio = useCallback(() => {
    interruptAudioElement(audioRef.current);
    audioRef.current = null;
    spokenRef.current = null;
    if (objectUrlRef.current) {
      URL.revokeObjectURL(objectUrlRef.current);
      objectUrlRef.current = null;
    }
  }, []);

  useEffect(() => {
    requestControllerRef.current?.abort();
    requestControllerRef.current = null;
    releaseAudio();
    setState((current) =>
      current.status === "synthesizing" || current.status === "playing" || current.status === "paused"
        ? reducePlaybackState(current, "interrupt")
        : current,
    );
  }, [generation, releaseAudio, sessionId]);

  const interrupt = useCallback(() => {
    requestControllerRef.current?.abort();
    requestControllerRef.current = null;
    releaseAudio();
    setState((current) => reducePlaybackState(current, "interrupt"));
  }, [releaseAudio]);

  const speak = useCallback(
    async (text: string) => {
      const trimmed = text.trim();
      if (!trimmed || !sessionId || generation === null) return;

      requestControllerRef.current?.abort();
      releaseAudio();
      const controller = new AbortController();
      requestControllerRef.current = controller;
      const requestGeneration = generation;
      setState((current) => ({ ...current, status: "synthesizing" }));

      let result: SpeechPlayback | null;
      try {
        result = await synthesizeSpeech(
          { text: trimmed, session_id: sessionId, generation: requestGeneration },
          controller.signal,
        );
      } catch (error) {
        if (controller.signal.aborted) return;
        setState((current) => ({ ...current, status: "error" }));
        onError?.(playbackErrorMessage(error));
        return;
      }

      const latest = latestRef.current;
      if (
        controller.signal.aborted ||
        latest.sessionId !== sessionId ||
        latest.generation !== requestGeneration
      ) {
        return;
      }
      if (!result) {
        setState((current) => reducePlaybackState(current, "provider_unavailable"));
        onError?.(playbackErrorMessage(null));
        return;
      }
      if (!shouldAcceptSpeechGeneration(requestGeneration, result.generation)) return;

      const objectUrl = URL.createObjectURL(new Blob([result.audio], { type: result.mediaType }));
      objectUrlRef.current = objectUrl;
      const audio = new Audio(objectUrl);
      audio.muted = state.muted;
      let lifecycleStarted = false;
      const reportFinished = async () => {
        if (audioRef.current !== audio || !lifecycleStarted) return;
        const current = latestRef.current;
        if (current.sessionId !== sessionId || current.generation !== requestGeneration) return;
        setState((playback) => reducePlaybackState(playback, "ended"));
        const finished = await markVoiceSpeechFinished(
          sessionId,
          requestGeneration,
          controller.signal,
        );
        if (!finished && !controller.signal.aborted && audioRef.current === audio) {
          onError?.("播放已结束，但语音会话状态未能同步。");
        }
      };
      audio.onended = () => {
        void reportFinished();
      };
      audio.onerror = () => {
        if (audioRef.current !== audio) return;
        setState((current) => ({ ...current, status: "error" }));
        onError?.("语音播放失败，请重试。");
      };
      audioRef.current = audio;
      try {
        await audio.play();
        const current = latestRef.current;
        if (
          controller.signal.aborted ||
          audioRef.current !== audio ||
          current.sessionId !== sessionId ||
          current.generation !== requestGeneration
        ) {
          interruptAudioElement(audio);
          return;
        }
        spokenRef.current = { text: trimmed, sessionId, generation: requestGeneration };
        const started = await markVoiceSpeechStarted(
          sessionId,
          requestGeneration,
          controller.signal,
        );
        if (!started || controller.signal.aborted || audioRef.current !== audio) {
          interruptAudioElement(audio);
          if (!controller.signal.aborted) {
            setState((playback) => ({ ...playback, status: "error" }));
            onError?.("音频已经启动，但语音会话未确认播放状态。");
          }
          return;
        }
        lifecycleStarted = true;
        if (audio.ended) {
          await reportFinished();
        } else {
          setState((playback) => ({ ...playback, status: "playing" }));
        }
      } catch (error) {
        if (!controller.signal.aborted) {
          setState((current) => ({ ...current, status: "paused" }));
          onError?.(playbackErrorMessage(error));
        }
      }
    },
    [generation, onError, releaseAudio, sessionId, state.muted],
  );

  const pause = useCallback(() => {
    audioRef.current?.pause();
    setState((current) => reducePlaybackState(current, "pause"));
  }, []);

  const replay = useCallback(() => {
    const audio = audioRef.current;
    if (!audio) return;
    const identity = latestRef.current;
    const controller = requestControllerRef.current;
    if (!identity.sessionId || identity.generation === null) return;
    const replaySessionId = identity.sessionId;
    const replayGeneration = identity.generation;
    audio.currentTime = 0;
    void replayAudioIfCurrent(audio,
      () => audioRef.current === audio && !controller?.signal.aborted
        && latestRef.current.sessionId === replaySessionId && latestRef.current.generation === replayGeneration,
      async () => Boolean(await markVoiceSpeechStarted(replaySessionId, replayGeneration, controller?.signal)),
      () => {
        if (audioRef.current !== audio) return;
        releaseAudio();
        if (latestRef.current.sessionId !== replaySessionId || latestRef.current.generation !== replayGeneration) return;
        setState((playback) => ({ ...playback, status: "error" }));
        onError?.("重播未获语音会话确认，已停止音频。请重试。");
      },
    ).then((started) => {
        if (started) {
          setState((playback) => reducePlaybackState(playback, "replay"));
        }
      })
      .catch(() => undefined);
  }, [onError, releaseAudio]);

  const toggleMute = useCallback(() => {
    const nextMuted = !audioRef.current?.muted;
    if (audioRef.current) audioRef.current.muted = nextMuted;
    setState((current) => reducePlaybackState(current, "mute"));
  }, []);

  useEffect(() => {
    return () => {
      requestControllerRef.current?.abort();
      requestControllerRef.current = null;
      releaseAudio();
    };
  }, [releaseAudio]);

  const captureOverlap = useCallback((): PlaybackOverlap | null => {
    return readPlaybackOverlap(audioRef.current, spokenRef.current, latestRef.current, performance.now());
  }, []);

  return { state, speak, pause, replay, toggleMute, interrupt, captureOverlap };
}
