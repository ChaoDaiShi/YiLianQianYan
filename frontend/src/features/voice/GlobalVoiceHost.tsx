import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { ReactNode } from "react";
import {
  dispatchVoiceTurn,
  endVoiceSession,
  getPresence,
  getVoiceSession,
  interruptVoiceSession,
  startVoiceSession,
  type ConversationalAnchor,
  type FocusedSurface,
  type GlobalVoiceSession,
  type PresenceSnapshot,
  type VoiceRuntimeSnapshot,
  type VoiceSessionState,
} from "../../api/voice";
import { useSpeechPlayback, cleanupVoiceSession } from "./useSpeechPlayback";
import { useVoiceCapture, type FinalVoiceTranscript } from "./useVoiceCapture";
import { runVoiceContinuation } from "./voiceContinuation";
import { useHandsFree } from "./useHandsFree";
import { shouldSuppressCaptureEcho, type CaptureEchoEvidence } from "./echoEvidence";
import { synchronizeVoiceContext } from "./voiceContextSync";
import "./globalVoice.css";

export interface GlobalVoiceHostProps {
  children: ReactNode;
  /** Preview data keeps the host deterministic in focused UI tests. */
  initialSnapshot?: VoiceRuntimeSnapshot | null;
  initialPresence?: PresenceSnapshot | null;
  initialExpanded?: boolean;
  /** Route/surface publishers use this thin bridge to update the active voice context. */
  contextPatch?: GlobalVoiceContextPatch;
}

export interface GlobalVoiceContextSnapshot {
  focused_surface: FocusedSurface;
  conversational_anchor: ConversationalAnchor | null;
  active_task: string | null;
}

export type GlobalVoiceContextPatch = Partial<GlobalVoiceContextSnapshot> & {
  /** Only conversation lifecycle publishers may explicitly clear the anchor. */
  anchor_action?: "replace";
};

export interface ConversationRefreshSignal {
  conversation_id: string;
  revision: number;
}

interface GlobalVoiceContextBridgeValue {
  context: GlobalVoiceContextSnapshot;
  conversationRefresh: ConversationRefreshSignal | null;
  session: GlobalVoiceSession | null;
  updateContext: (patch: GlobalVoiceContextPatch) => void;
}

const GlobalVoiceContextBridge = createContext<GlobalVoiceContextBridgeValue | null>(null);

export function useGlobalVoiceContext(): GlobalVoiceContextBridgeValue {
  const value = useContext(GlobalVoiceContextBridge);
  if (!value) {
    throw new Error("useGlobalVoiceContext must be used inside GlobalVoiceHost");
  }
  return value;
}

export function mergeVoiceContext(
  current: GlobalVoiceContextSnapshot,
  patch: GlobalVoiceContextPatch,
): GlobalVoiceContextSnapshot {
  const next = { ...current };
  for (const key of [
    "focused_surface",
    "conversational_anchor",
    "active_task",
  ] as const) {
    if (key === "conversational_anchor" && patch[key] === null && patch.anchor_action !== "replace") continue;
    if (patch[key] !== undefined) next[key] = patch[key] as never;
  }
  return next;
}

export function nextConversationRefresh(
  current: ConversationRefreshSignal | null,
  conversationId: string,
): ConversationRefreshSignal {
  const conversation_id = conversationId.trim();
  if (!conversation_id) throw new Error("conversation identity must not be empty");
  return {
    conversation_id,
    revision: current?.conversation_id === conversation_id ? current.revision + 1 : 1,
  };
}

export type VoiceSurfaceHostMode = "standalone" | "desktop-skeleton";

export function resolveVoiceContextForRoute(
  pathname: string,
  mode: VoiceSurfaceHostMode,
): GlobalVoiceContextPatch {
  if (mode === "desktop-skeleton") return { focused_surface: "workspace" };
  if (pathname === "/chat" || pathname.startsWith("/chat/")) {
    return {
      focused_surface: "conversation",
    };
  }
  if (
    pathname === "/tasks" ||
    pathname === "/task-world" ||
    pathname.startsWith("/task-world/")
  ) {
    return {
      focused_surface: "task_canvas",
    };
  }
  if (pathname === "/memory" || pathname.startsWith("/memory/")) {
    return {
      focused_surface: "memory",
    };
  }
  if (pathname === "/capabilities" || pathname.startsWith("/capabilities/")) {
    return {
      focused_surface: "capability_center",
    };
  }
  if (pathname === "/system" || pathname === "/logs" || pathname === "/settings") {
    return {
      focused_surface: "system",
    };
  }
  return {
    focused_surface: "workspace",
  };
}

const EMPTY_PRESENCE: PresenceSnapshot = {
  activity: "idle",
  interaction: "none",
  attention: "none",
  source: "global-voice-runtime",
  updated_at: 0,
};

export function getVoiceSessionKey(snapshot: VoiceRuntimeSnapshot | null): string {
  const session = snapshot?.session;
  return session ? `${session.voice_session_id}:${session.generation}` : "voice:none";
}

export function isFinalTranscriptForSession(
  result: FinalVoiceTranscript,
  session: GlobalVoiceSession | null,
): boolean {
  return Boolean(
    session &&
      session.voice_session_id === result.sessionId &&
      session.generation === result.generation,
  );
}

export function voiceStatusLabel(
  status: VoiceSessionState | "acquiring" | "transcribing" | "playing" | "paused" | "error" | "idle",
): string {
  switch (status) {
    case "listening":
      return "正在听取";
    case "processing":
    case "transcribing":
      return "正在处理";
    case "speaking":
    case "playing":
      return "正在播报";
    case "acquiring":
      return "正在准备麦克风";
    case "paused":
      return "播报已暂停";
    case "interrupted":
      return "已打断";
    case "error":
      return "语音错误";
    case "ended":
      return "会话已结束";
    case "stopped":
      return "已停止";
    case "cancelled":
      return "已取消";
    case "idle":
    default:
      return "未启动";
  }
}

function normalizeSnapshot(value: VoiceRuntimeSnapshot | null): VoiceRuntimeSnapshot | null {
  if (!value) return null;
  const candidate = value as unknown as { snapshot?: VoiceRuntimeSnapshot };
  return candidate.snapshot ?? value;
}

function contextFromSession(session: GlobalVoiceSession | null): GlobalVoiceContextSnapshot {
  return {
    focused_surface: session?.focused_surface ?? "conversation",
    conversational_anchor: session?.conversational_anchor ?? null,
    active_task: session?.active_task ?? null,
  };
}

function contextMatchesSession(
  context: GlobalVoiceContextSnapshot,
  session: GlobalVoiceSession,
): boolean {
  return (
    context.focused_surface === session.focused_surface &&
    context.active_task === (session.active_task ?? null) &&
    context.conversational_anchor?.conversation_id === session.conversational_anchor?.conversation_id &&
    context.conversational_anchor?.title === session.conversational_anchor?.title &&
    context.conversational_anchor?.updated_at === session.conversational_anchor?.updated_at
  );
}

function snapshotWithSession(
  previous: VoiceRuntimeSnapshot | null,
  session: GlobalVoiceSession,
): VoiceRuntimeSnapshot {
  return {
    session,
    lease: null,
    partial_transcript: null,
    final_transcript: previous?.final_transcript ?? null,
    turns: previous?.turns ?? [],
    presence: previous?.presence ?? EMPTY_PRESENCE,
  };
}

function transcriptFromTurn(snapshot: VoiceRuntimeSnapshot | null): string {
  const lastTurn = snapshot?.turns[snapshot.turns.length - 1];
  return snapshot?.final_transcript ?? lastTurn?.final_transcript ?? "";
}

export default function GlobalVoiceHost({
  children,
  initialSnapshot = null,
  initialPresence = null,
  initialExpanded = false,
  contextPatch,
}: GlobalVoiceHostProps) {
  const [snapshot, setSnapshot] = useState<VoiceRuntimeSnapshot | null>(initialSnapshot);
  const [presence, setPresence] = useState<PresenceSnapshot>(
    initialPresence ?? initialSnapshot?.presence ?? EMPTY_PRESENCE,
  );
  const [expanded, setExpanded] = useState(initialExpanded);
  const [notice, setNotice] = useState<string | null>(null);
  const [targetDescription, setTargetDescription] = useState("尚未解析");
  const [intentDescription, setIntentDescription] = useState("会话对话");
  const [handsFreeEnabled, setHandsFreeEnabled] = useState(false);
  const [conversationRefresh, setConversationRefresh] = useState<ConversationRefreshSignal | null>(null);
  const captureEchoRef = useRef<CaptureEchoEvidence | null>(null);
  const captureStartEpochRef = useRef(0);
  const invalidateContextRef = useRef<() => void>(() => undefined);
  const localContextTouchedRef = useRef(false);
  const contextIdentityEpochRef = useRef(0);
  const dispatchEpochRef = useRef(0);
  const continuationControllerRef = useRef<AbortController | null>(null);
  const latestSessionRef = useRef<GlobalVoiceSession | null>(initialSnapshot?.session ?? null);
  const [voiceContext, setVoiceContext] = useState<GlobalVoiceContextSnapshot>(() =>
    contextFromSession(initialSnapshot?.session ?? null),
  );
  const latestContextRef = useRef(voiceContext);
  latestContextRef.current = voiceContext;

  const updateContext = useCallback((patch: GlobalVoiceContextPatch) => {
    localContextTouchedRef.current = true;
    const next = mergeVoiceContext(latestContextRef.current, patch);
    if (next.conversational_anchor?.conversation_id !== latestContextRef.current.conversational_anchor?.conversation_id) {
      contextIdentityEpochRef.current += 1;
      captureStartEpochRef.current += 1;
      captureEchoRef.current = null;
      dispatchEpochRef.current += 1;
      continuationControllerRef.current?.abort();
      continuationControllerRef.current = null;
      invalidateContextRef.current();
    }
    latestContextRef.current = next;
    setVoiceContext(next);
  }, []);

  const session = snapshot?.session ?? null;
  latestSessionRef.current = session;
  const finalTranscript = transcriptFromTurn(snapshot);

  useEffect(() => {
    if (initialSnapshot) setSnapshot(initialSnapshot);
    if (initialPresence) setPresence(initialPresence);
    if (initialSnapshot?.session && !localContextTouchedRef.current) {
      setVoiceContext(contextFromSession(initialSnapshot.session));
    }
  }, [initialPresence, initialSnapshot]);

  useEffect(() => {
    if (!contextPatch) return;
    updateContext(contextPatch);
  }, [
    contextPatch?.active_task,
    contextPatch?.conversational_anchor,
    contextPatch?.focused_surface,
    updateContext,
  ]);

  useEffect(() => {
    const controller = new AbortController();
    let cancelled = false;
    void Promise.all([getVoiceSession(controller.signal), getPresence(controller.signal)]).then(
      ([remoteSnapshot, remotePresence]) => {
        if (cancelled) return;
        const normalized = normalizeSnapshot(remoteSnapshot);
        if (normalized) {
          setSnapshot(normalized);
          if (normalized.session && !localContextTouchedRef.current) {
            setVoiceContext(contextFromSession(normalized.session));
          }
        }
        if (remotePresence) setPresence(remotePresence);
      },
    ).catch(() => {
      // A missing backend should leave the host visible with an honest idle state.
    });
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, []);

  useEffect(() => {
    if (!session || contextMatchesSession(voiceContext, session)) return;
    const controller = new AbortController();
    const epoch = dispatchEpochRef.current;
    void synchronizeVoiceContext({
      session,
      signal: controller.signal,
      getContext: () => latestContextRef.current,
      isCurrent: () => dispatchEpochRef.current === epoch
        && latestSessionRef.current?.voice_session_id === session.voice_session_id
        && latestSessionRef.current?.generation === session.generation,
    }).then((updated) => {
      if (!updated || controller.signal.aborted) return;
      setSnapshot((previous) => {
        if (
          controller.signal.aborted || dispatchEpochRef.current !== epoch
          || !contextMatchesSession(latestContextRef.current, updated) ||
          !previous?.session ||
          previous.session.voice_session_id !== session.voice_session_id ||
          previous.session.generation !== session.generation
        ) {
          return previous;
        }
        return { ...previous, session: updated };
      });
    }).catch((error) => {
      if (controller.signal.aborted || dispatchEpochRef.current !== epoch) return;
      setNotice(error instanceof Error ? error.message : "语音上下文同步失败。");
    });
    return () => controller.abort();
  }, [
    session,
    voiceContext,
  ]);

  const contextBridge = useMemo(
    () => ({ context: voiceContext, conversationRefresh, session, updateContext }),
    [conversationRefresh, session, updateContext, voiceContext],
  );

  const setVoiceError = useCallback((message: string) => {
    setNotice(message);
  }, []);

  const playback = useSpeechPlayback({
    sessionId: session?.voice_session_id,
    generation: session?.generation,
    onError: setVoiceError,
  });

  const playbackRef = useRef(playback);
  playbackRef.current = playback;
  useEffect(() => () => {
    dispatchEpochRef.current += 1;
    captureStartEpochRef.current += 1;
    captureEchoRef.current = null;
    continuationControllerRef.current?.abort();
  }, []);

  const handleFinalTranscript = useCallback(
    async (result: FinalVoiceTranscript) => {
      const current = latestSessionRef.current;
      const dispatchEpoch = dispatchEpochRef.current;
      if (!current || !isFinalTranscriptForSession(result, current)
        || current.conversational_anchor?.conversation_id !== latestContextRef.current.conversational_anchor?.conversation_id) {
        setNotice("这段语音来自旧会话，已安全丢弃。");
        return;
      }
      const echoEvidence = captureEchoRef.current;
      captureEchoRef.current = null;
      if (shouldSuppressCaptureEcho(result, echoEvidence, contextIdentityEpochRef.current, performance.now())) {
        setNotice("已忽略疑似播报回声，请重新说话。");
        return;
      }
      setSnapshot((previous) =>
        previous
          ? {
              ...previous,
              lease: null,
              partial_transcript: null,
              final_transcript: result.text,
              session: { ...current, state: "processing", updated_at: Date.now() },
            }
          : previous,
      );
      const routed = await dispatchVoiceTurn({
        session_id: result.sessionId,
        generation: result.generation,
        lease_id: result.leaseId,
        final_transcript: result.text,
      });
      if (!routed) {
        setNotice("最终语音已接收，但交互路由暂不可用。");
        return;
      }
      const latest = latestSessionRef.current;
      if (
        dispatchEpochRef.current !== dispatchEpoch ||
        !latest ||
        latest.voice_session_id !== result.sessionId ||
        latest.generation !== result.generation
      ) {
        return;
      }
      setTargetDescription(
        routed.turn.resolved_target.status === "resolved" ? "服务端已解析" : "服务端未解析",
      );
      setIntentDescription(
        routed.turn.intent.kind === "conversation_turn" ? "会话对话（服务端）" : "服务端意图",
      );
      setSnapshot((previous) =>
        previous
          ? {
              ...previous,
              turns: [...previous.turns, routed.turn],
            }
          : previous,
      );
      let narration = routed.narration ?? null;
      if (routed.continuation) {
        const continuation = routed.continuation;
        continuationControllerRef.current?.abort();
        const continuationController = new AbortController();
        continuationControllerRef.current = continuationController;
        try {
          const continuationResult = await runVoiceContinuation(continuation, {
            signal: continuationController.signal,
            isCurrent: () => dispatchEpochRef.current === dispatchEpoch
              && latestSessionRef.current?.voice_session_id === result.sessionId
              && latestSessionRef.current?.generation === result.generation
              && latestContextRef.current.conversational_anchor?.conversation_id === current.conversational_anchor?.conversation_id,
          });
          narration = continuationResult.narration;
          if (continuation.kind === "conversation") {
            setConversationRefresh((previous) => (
              nextConversationRefresh(previous, continuation.conversation_id)
            ));
          }
        } catch (error) {
          if (!continuationController.signal.aborted) {
            setNotice(error instanceof Error ? error.message : "语音 continuation 执行失败");
          }
          return;
        } finally {
          if (continuationControllerRef.current === continuationController) {
            continuationControllerRef.current = null;
          }
        }
      }
      const afterContinuation = latestSessionRef.current;
      if (
        dispatchEpochRef.current !== dispatchEpoch ||
        !afterContinuation ||
        afterContinuation.voice_session_id !== result.sessionId ||
        afterContinuation.generation !== result.generation
      ) {
        return;
      }
      if (narration && current.attention_mode !== "silent") {
        await playbackRef.current.speak(narration);
      }
    },
    [playback],
  );

  const interruptForBargeIn = useCallback(async (): Promise<GlobalVoiceSession | void> => {
    dispatchEpochRef.current += 1;
    const requestEpoch = dispatchEpochRef.current;
    continuationControllerRef.current?.abort();
    continuationControllerRef.current = null;
    playback.interrupt();
    if (!session) return;
    if (session.conversational_anchor?.conversation_id !== latestContextRef.current.conversational_anchor?.conversation_id) {
      throw new Error("正在同步对话，请稍后重新说话。");
    }
    const interrupted = await interruptVoiceSession(
      session.voice_session_id,
      session.generation,
    );
    if (!interrupted) {
      throw new Error("语音服务未确认打断，未取得新的输入 generation。");
    }
    if (dispatchEpochRef.current !== requestEpoch
      || latestSessionRef.current?.voice_session_id !== session.voice_session_id
      || latestSessionRef.current?.generation !== session.generation) {
      throw new Error("旧语音会话的打断响应已丢弃。");
    }
    latestSessionRef.current = interrupted;
    setSnapshot((previous) =>
      previous
        ? { ...previous, session: interrupted, lease: null, partial_transcript: null }
        : previous,
    );
    setPresence((previous) => ({ ...previous, interaction: "interrupted" }));
    return interrupted;
  }, [playback.interrupt, session]);

  const capture = useVoiceCapture({
    session,
    handsFreeEnabled,
    onBargeIn: interruptForBargeIn,
    onPartial: (text) => {
      setSnapshot((previous) =>
        previous ? { ...previous, partial_transcript: text } : previous,
      );
    },
    onFinal: handleFinalTranscript,
    onError: setVoiceError,
  });
  invalidateContextRef.current = () => { capture.cancel(); playback.interrupt(); };
  useHandsFree({
    enabled: handsFreeEnabled && session?.state !== "ended",
    sessionId: session?.voice_session_id ?? null,
    identityEpoch: contextIdentityEpochRef.current,
    playing: playback.state.status === "playing" || playback.state.status === "synthesizing",
    capture: {
      ...capture,
      start: async (reason, stream) => {
        const operationEpoch = ++captureStartEpochRef.current;
        const identityEpoch = contextIdentityEpochRef.current;
        captureEchoRef.current = null;
        // Snapshot only actual audio playback, before barge-in releases it.
        const overlap = playbackRef.current.captureOverlap();
        const handle = await capture.start(reason, stream);
        if (overlap && handle?.lease && operationEpoch === captureStartEpochRef.current
          && identityEpoch === contextIdentityEpochRef.current
          && overlap.sessionId === handle.lease.sessionId
          && overlap.playbackGeneration + 1 === handle.lease.generation) {
          captureEchoRef.current = { ...overlap, ...handle.lease, identityEpoch };
        }
        return handle;
      },
    },
    onError: (message) => { setHandsFreeEnabled(false); capture.cancel(); setNotice(message); },
  });

  const startSession = useCallback(async () => {
    captureStartEpochRef.current += 1;
    captureEchoRef.current = null;
    dispatchEpochRef.current += 1;
    const requestEpoch = dispatchEpochRef.current;
    continuationControllerRef.current?.abort();
    continuationControllerRef.current = null;
    playback.interrupt();
    const started = await startVoiceSession(voiceContext.focused_surface);
    if (dispatchEpochRef.current !== requestEpoch) return;
    if (!started) {
      setNotice("语音会话未能启动，请检查后端与语音配置。");
      return;
    }
    setNotice(null);
    setSnapshot((previous) => snapshotWithSession(previous, started));
    setPresence((previous) => ({ ...previous, activity: "working", interaction: "listening" }));
  }, [playback, voiceContext.focused_surface]);

  const endSession = useCallback(async () => {
    if (!session) return;
    captureStartEpochRef.current += 1;
    captureEchoRef.current = null;
    setHandsFreeEnabled(false);
    dispatchEpochRef.current += 1;
    const requestEpoch = dispatchEpochRef.current;
    continuationControllerRef.current?.abort();
    continuationControllerRef.current = null;
    await cleanupVoiceSession({
      sessionId: session.voice_session_id,
      generation: session.generation,
      cancelCapture: capture.cancel,
      interruptPlayback: playback.interrupt,
      endSession: async (sessionId, generation) => {
        const ended = await endVoiceSession(sessionId, generation);
        if (ended && dispatchEpochRef.current === requestEpoch) {
          setSnapshot((previous) =>
            previous
              ? {
                  ...previous,
                  session: ended,
                  lease: null,
                  partial_transcript: null,
                  final_transcript: null,
                }
              : snapshotWithSession(null, ended),
          );
          setPresence((previous) => ({ ...previous, activity: "idle", interaction: "none" }));
        }
      },
    });
    setNotice(null);
  }, [capture.cancel, playback.interrupt, session]);

  const interruptSession = useCallback(async () => {
    if (!session) return;
    capture.cancel();
    try {
      await interruptForBargeIn();
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "语音服务未确认打断，请重试。");
    }
  }, [capture.cancel, interruptForBargeIn, session]);

  const startCapture = useCallback(async () => {
    const operationEpoch = ++captureStartEpochRef.current;
    const identityEpoch = contextIdentityEpochRef.current;
    captureEchoRef.current = null;
    const overlap = playbackRef.current.captureOverlap();
    const handle = await capture.start("user-click");
    if (overlap && handle?.lease && operationEpoch === captureStartEpochRef.current
      && identityEpoch === contextIdentityEpochRef.current
      && overlap.sessionId === handle.lease.sessionId
      && overlap.playbackGeneration + 1 === handle.lease.generation) {
      captureEchoRef.current = { ...overlap, ...handle.lease, identityEpoch };
    }
  }, [capture]);

  const speakLastTranscript = useCallback(() => {
    if (!finalTranscript) {
      setNotice("还没有可播报的最终文字。");
      return;
    }
    void playback.speak(finalTranscript);
  }, [finalTranscript, playback]);

  const currentStatus = useMemo(() => {
    if (capture.state.status === "listening" || capture.state.status === "transcribing") {
      return capture.state.status;
    }
    if (playback.state.status === "playing") return "playing";
    if (playback.state.status === "paused") return "paused";
    if (playback.state.status === "error") return "error";
    return session?.state ?? "idle";
  }, [capture.state.status, playback.state.status, session?.state]);

  useEffect(() => {
    if (playback.state.status === "playing") {
      setPresence((previous) => ({ ...previous, activity: "working", interaction: "speaking" }));
    } else if (capture.state.status === "listening" || capture.state.status === "transcribing") {
      setPresence((previous) => ({ ...previous, activity: "working", interaction: "listening" }));
    }
  }, [capture.state.status, playback.state.status]);

  return (
    <GlobalVoiceContextBridge.Provider value={contextBridge}>
      <div className="global-voice-host">
        {children}
        <aside className="global-voice-dock" aria-label="全局语音会话">
        <div className="global-voice-pill" data-testid="voice-pill">
          <span className="global-voice-dot" data-state={currentStatus} aria-hidden="true" />
          <div className="global-voice-pill-copy">
            <strong>{voiceStatusLabel(currentStatus)}</strong>
            <span>{session ? `会话 ${session.voice_session_id}` : "未建立语音会话"}</span>
          </div>
          <button
            type="button"
            className="global-voice-expand"
            aria-expanded={expanded}
            aria-controls="global-voice-panel"
            onClick={() => setExpanded((value) => !value)}
          >
            {expanded ? "收起" : "展开"}
          </button>
        </div>

        {expanded ? (
          <section className="global-voice-panel" id="global-voice-panel" data-testid="voice-panel">
            <div className="global-voice-panel-heading">
              <div>
                <span className="global-voice-eyebrow">GLOBAL VOICE</span>
                <h2>小涟语音控制台</h2>
              </div>
              <span className="global-voice-presence" data-presence={presence.interaction}>
                {voiceStatusLabel(currentStatus)}
              </span>
            </div>

            <dl className="global-voice-context-grid">
              <div>
                <dt>当前状态</dt>
                <dd>{voiceStatusLabel(currentStatus)}</dd>
              </div>
              <div>
                <dt>当前解析目标</dt>
                <dd>{targetDescription}</dd>
              </div>
              <div>
                <dt>当前意图</dt>
                <dd>{intentDescription}</dd>
              </div>
              <div>
                <dt>当前 Anchor</dt>
                <dd>{session?.conversational_anchor?.title ?? "无"}</dd>
              </div>
              <div>
                <dt>Active Task</dt>
                <dd>{session?.active_task ?? "无"}</dd>
              </div>
            </dl>

            <div className="global-voice-transcript" aria-live="polite">
              <div>
                <span>实时 transcript</span>
                <p>{snapshot?.partial_transcript ?? "等待新的语音输入"}</p>
              </div>
              <div>
                <span>最终 transcript</span>
                <p>{finalTranscript || "尚无最终文字"}</p>
              </div>
            </div>

            <div className="global-voice-controls">
              {session && session.state !== "ended" ? (
                <>
                  <button
                    type="button"
                    className="global-voice-primary"
                    data-testid="voice-mic"
                    onClick={capture.state.status === "listening" ? capture.stop : () => void startCapture()}
                    disabled={capture.state.status === "acquiring" || capture.state.status === "transcribing"}
                  >
                    {capture.state.status === "listening" ? "停止录音" : "开始说话"}
                  </button>
                  <button type="button" onClick={() => void interruptSession()}>
                    打断
                  </button>
                  <button type="button" aria-pressed={handsFreeEnabled} onClick={() => {
                    if (handsFreeEnabled) capture.cancel();
                    setHandsFreeEnabled(!handsFreeEnabled);
                  }}>
                    {handsFreeEnabled ? "关闭免手持" : "开启免手持"}
                  </button>
                  {handsFreeEnabled ? <span role="status">麦克风持续开启；说话可打断播报，停顿后提交。建议佩戴耳机。</span> : null}
                </>
              ) : (
                <button type="button" className="global-voice-primary" onClick={() => void startSession()}>
                  开始会话
                </button>
              )}
              <button
                type="button"
                onClick={speakLastTranscript}
                disabled={!finalTranscript || !session || session.state === "ended"}
              >
                播报
              </button>
              <button type="button" onClick={playback.pause} disabled={playback.state.status !== "playing"}>
                暂停 TTS
              </button>
              <button type="button" onClick={playback.replay} disabled={playback.state.status !== "paused" && playback.state.status !== "ended"}>
                重播
              </button>
              <button type="button" onClick={playback.toggleMute}>
                {playback.state.muted ? "取消静音" : "静音 TTS"}
              </button>
              <button type="button" className="global-voice-end" onClick={() => void endSession()} disabled={!session}>
                结束会话
              </button>
            </div>

            {capture.state.error || notice ? (
              <p className="global-voice-notice" role="status">
                {capture.state.error ?? notice}
              </p>
            ) : null}
          </section>
        ) : null}
        </aside>
      </div>
    </GlobalVoiceContextBridge.Provider>
  );
}

export { GlobalVoiceHost };
