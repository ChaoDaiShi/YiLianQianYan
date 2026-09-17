import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { VoiceRuntimeSnapshot } from "../../api/voice";
import hostSource from "./GlobalVoiceHost.tsx?raw";
import captureSource from "./useVoiceCapture.ts?raw";
import playbackSource from "./useSpeechPlayback.ts?raw";
import {
  GlobalVoiceHost,
  getVoiceSessionKey,
  isFinalTranscriptForSession,
  mergeVoiceContext,
  resolveVoiceContextForRoute,
  voiceStatusLabel,
} from "./GlobalVoiceHost";
import {
  describeCaptureError,
  acquireAfterBargeIn,
  isExplicitMicrophoneStart,
} from "./useVoiceCapture";
import {
  cleanupVoiceSession,
  replayAudioIfCurrent,
  interruptAudioElement,
  reducePlaybackState,
  shouldAcceptSpeechGeneration,
  shouldDispatchFinalTranscript,
  type PlaybackState,
} from "./useSpeechPlayback";

const snapshot: VoiceRuntimeSnapshot = {
  session: {
    voice_session_id: "voice-session-6",
    generation: 4,
    state: "listening",
    input_owner: "builtin_asr",
    focused_surface: "conversation",
    conversational_anchor: {
      conversation_id: "conversation-7",
      title: "Rust 练习",
      updated_at: 1726000000000,
    },
    active_task: "task-42",
    attention_mode: "balanced",
    started_at: 1726000000000,
    updated_at: 1726000001000,
    schema_version: 1,
  },
  lease: {
    lease_id: "lease-9",
    voice_session_id: "voice-session-6",
    generation: 4,
    owner: "builtin_asr",
    acquired_at: 1726000000000,
  },
  partial_transcript: "先打开编辑器",
  final_transcript: "请继续 Rust 练习",
  turns: [],
  presence: {
    activity: "working",
    interaction: "listening",
    attention: "none",
    source: "voice",
    updated_at: 1726000001000,
  },
};

describe("GlobalVoiceHost contract", () => {
  it("does not report or revive playback when replay resolves after interruption", async () => {
    let resolvePlay!: () => void;
    const play = new Promise<void>((resolve) => { resolvePlay = resolve; });
    let current = true;
    const audio = { play: () => play, pause: vi.fn(), currentTime: 3, removeAttribute: vi.fn(), load: vi.fn() } as unknown as HTMLAudioElement;
    const report = vi.fn().mockResolvedValue(true);
    const replay = replayAudioIfCurrent(audio, () => current, report);
    current = false;
    resolvePlay();
    expect(await replay).toBe(false);
    expect(report).not.toHaveBeenCalled();
    expect(audio.pause).toHaveBeenCalledOnce();
  });
  it("keeps one visible pill and one session key while the surface content changes", () => {
    const first = renderToStaticMarkup(
      <GlobalVoiceHost initialSnapshot={snapshot}>
        <main data-surface="conversation">conversation</main>
      </GlobalVoiceHost>,
    );
    const second = renderToStaticMarkup(
      <GlobalVoiceHost initialSnapshot={snapshot}>
        <main data-surface="desktop">desktop</main>
      </GlobalVoiceHost>,
    );

    expect(first).toContain('data-testid="voice-pill"');
    expect(second).toContain('data-testid="voice-pill"');
    expect(first).toContain("voice-session-6");
    expect(second).toContain("voice-session-6");
    expect(getVoiceSessionKey(snapshot)).toBe(getVoiceSessionKey(snapshot));
  });

  it("renders the expanded v1 context and transcript fields without sensitive handles", () => {
    const html = renderToStaticMarkup(
      <GlobalVoiceHost initialSnapshot={snapshot} initialExpanded>
        <main>surface</main>
      </GlobalVoiceHost>,
    );

    expect(html).toContain("Rust 练习");
    expect(html).toContain("task-42");
    expect(html).toContain("先打开编辑器");
    expect(html).toContain("请继续 Rust 练习");
    expect(html).not.toMatch(/\b(?:hwnd|window_id|pid|process_path)\b/);
    expect(voiceStatusLabel("listening")).toBe("正在听取");
  });

  it("requires an explicit user gesture before microphone capture", () => {
    expect(isExplicitMicrophoneStart("user-click")).toBe(true);
    expect(isExplicitMicrophoneStart("user-keyboard")).toBe(true);
    expect(isExplicitMicrophoneStart("mount")).toBe(false);
    expect(isExplicitMicrophoneStart(undefined)).toBe(false);
  });

  it("waits for the server barge-in acknowledgement before acquiring a lease", async () => {
    const calls: string[] = [];
    const result = await acquireAfterBargeIn(
      async () => {
        calls.push("interrupt:start");
        await Promise.resolve();
        calls.push("interrupt:done");
      },
      async () => {
        calls.push("lease");
        return "lease-ready";
      },
    );

    expect(result).toBe("lease-ready");
    expect(calls).toEqual(["interrupt:start", "interrupt:done", "lease"]);
  });

  it("turns a denied device into an actionable, non-secret message", () => {
    expect(describeCaptureError(new DOMException("Permission denied", "NotAllowedError"))).toBe(
      "麦克风权限未授予。请在系统设置中允许后重试。",
    );
    expect(describeCaptureError(new Error("device unavailable"))).toBe("麦克风暂不可用，请重试。");
  });

  it("ends capture and playback before ending the server session", async () => {
    const calls: string[] = [];
    await cleanupVoiceSession({
      sessionId: "voice-session-6",
      generation: 4,
      cancelCapture: () => calls.push("capture"),
      interruptPlayback: () => calls.push("playback"),
      endSession: async (sessionId, generation) => {
        calls.push(`end:${sessionId}:${generation}`);
      },
    });
    expect(calls).toEqual(["capture", "playback", "end:voice-session-6:4"]);
  });

  it("supports pause, replay, and mute as explicit playback state transitions", () => {
    const playing: PlaybackState = { status: "playing", muted: false };
    expect(reducePlaybackState(playing, "pause")).toEqual({ status: "paused", muted: false });
    expect(reducePlaybackState({ status: "paused", muted: false }, "replay")).toEqual({
      status: "playing",
      muted: false,
    });
    expect(reducePlaybackState(playing, "mute")).toEqual({ status: "playing", muted: true });
    expect(reducePlaybackState(playing, "provider_unavailable")).toEqual({
      status: "error",
      muted: false,
    });
  });

  it("bridges route context without inventing identity or changing session ownership", () => {
    const current = {
      focused_surface: "conversation" as const,
      conversational_anchor: snapshot.session?.conversational_anchor ?? null,
      active_task: "task-42",
    };
    expect(mergeVoiceContext(current, {
      focused_surface: "task_canvas",
      active_task: "graph-7",
    })).toEqual({
      ...current,
      focused_surface: "task_canvas",
      active_task: "graph-7",
    });
    expect(resolveVoiceContextForRoute("/chat/conversation-7", "standalone")).toEqual({
      focused_surface: "conversation",
      active_task: null,
    });
    expect(resolveVoiceContextForRoute("/task-world/graph-7", "standalone")).toEqual({
      focused_surface: "task_canvas",
    });
    expect(resolveVoiceContextForRoute("/chat/conversation-7", "desktop-skeleton")).toEqual({
      focused_surface: "workspace",
    });
  });

  it("preserves the conversation anchor through every ordinary workspace route", () => {
    const current = {
      focused_surface: "conversation" as const,
      conversational_anchor: snapshot.session!.conversational_anchor!,
      active_task: null,
    };
    for (const route of ["/tasks", "/task-world/graph-7", "/capabilities", "/system", "/settings"]) {
      expect(mergeVoiceContext(current, resolveVoiceContextForRoute(route, "standalone"))
        .conversational_anchor).toEqual(current.conversational_anchor);
    }
    expect(mergeVoiceContext(current, { active_task: "graph-7", conversational_anchor: null })
      .conversational_anchor).toEqual(current.conversational_anchor);
    expect(mergeVoiceContext(current, { conversational_anchor: null, anchor_action: "replace" })
      .conversational_anchor).toBeNull();
  });

  it("dispatches only a final transcript and accepts speech from the active generation", () => {
    expect(shouldDispatchFinalTranscript("partial")).toBe(false);
    expect(shouldDispatchFinalTranscript("final")).toBe(true);
    expect(shouldAcceptSpeechGeneration(4, 4)).toBe(true);
    expect(shouldAcceptSpeechGeneration(4, 3)).toBe(false);
  });

  it("accepts a final transcript against the latest barge-in generation", () => {
    const staleRenderSession = snapshot.session!;
    const interruptedSession = { ...staleRenderSession, generation: 5 };
    const finalTranscript = {
      text: "小莲，测试一下语音识别。",
      sessionId: interruptedSession.voice_session_id,
      generation: interruptedSession.generation,
      leaseId: "lease-after-barge-in",
    };

    expect(isFinalTranscriptForSession(finalTranscript, interruptedSession)).toBe(true);
    expect(isFinalTranscriptForSession(finalTranscript, staleRenderSession)).toBe(false);
  });

  it("reports speaking only after real playback starts and finishes on audio ended", () => {
    expect(playbackSource).toContain('"synthesizing"');
    expect(playbackSource).toContain("markVoiceSpeechStarted");
    expect(playbackSource).toContain("markVoiceSpeechFinished");
    expect(playbackSource.indexOf("await audio.play()"))
      .toBeLessThan(playbackSource.indexOf("await markVoiceSpeechStarted"));
    expect(playbackSource.indexOf("shouldAcceptSpeechGeneration"))
      .toBeLessThan(playbackSource.indexOf("await markVoiceSpeechStarted"));
    expect(playbackSource).toContain("await markVoiceSpeechFinished");
    expect(playbackSource).toContain("audio.onended = () => {");
    expect(playbackSource).toContain("void reportFinished();");
  });

  it("keeps capture final-first and does not submit cumulative partial audio", () => {
    expect(captureSource).toContain("transcribeAudio(");
    expect(captureSource).not.toContain("PARTIAL_TRANSCRIPT_INTERVAL_MS");
    expect(captureSource).not.toContain("transcribePartialAudio");
    expect(captureSource).toContain("finalInFlightRef");
  });

  it("routes trusted continuations before speaking and keeps the dispatch epoch guard", () => {
    expect(hostSource).toContain("runVoiceContinuation");
    expect(hostSource).toContain("routed.continuation");
    expect(hostSource).toContain("dispatchEpochRef.current !== dispatchEpoch");
    expect(hostSource).toContain("continuationResult.narration");
  });

  it("interrupts the active audio object before capture can enter listening", () => {
    const audio = {
      pause: vi.fn(),
      currentTime: 12,
      removeAttribute: vi.fn(),
      load: vi.fn(),
    } as unknown as HTMLAudioElement;

    interruptAudioElement(audio);

    expect(audio.pause).toHaveBeenCalledOnce();
    expect(audio.currentTime).toBe(0);
    expect(audio.removeAttribute).toHaveBeenCalledWith("src");
    expect(audio.load).toHaveBeenCalledOnce();
  });
});
