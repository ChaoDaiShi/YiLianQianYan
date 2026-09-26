import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  dispatchVoiceTurn,
  endVoiceSession,
  getPresence,
  getVoiceSession,
  interruptVoiceSession,
  startVoiceSession,
  type GlobalVoiceSession,
  type PresenceSnapshot,
  type VoiceRuntimeSnapshot,
} from "../../api/voice";
import { GlobalVoiceContextBridge, mergeVoiceContext } from "./context/VoiceContext";
import {
  EMPTY_PRESENCE,
  contextFromSession,
  contextMatchesSession,
  isFinalTranscriptForSession,
  nextConversationRefresh,
  normalizeSnapshot,
  snapshotWithSession,
  transcriptFromTurn,
} from "./model/projection";
import type {
  ConversationRefreshSignal,
  GlobalVoiceContextBridgeValue,
  GlobalVoiceContextPatch,
  GlobalVoiceContextSnapshot,
  GlobalVoiceHostProps,
} from "./model/types";
import { useSpeechPlayback, cleanupVoiceSession } from "./useSpeechPlayback";
import { useVoiceCapture, type FinalVoiceTranscript } from "./useVoiceCapture";
import { runVoiceContinuation } from "./voiceContinuation";
import { runVoiceBargeIn } from "./bargeIn";
import { useHandsFree } from "./useHandsFree";
import { shouldSuppressCaptureEcho, type CaptureEchoEvidence } from "./echoEvidence";
import { synchronizeVoiceContext } from "./voiceContextSync";
import { type ChatVoiceControls } from "./ChatVoiceInput";
import { GlobalVoiceLeaf, type GlobalVoiceProductState } from "./GlobalVoiceLeaf";
import "./globalVoice.css";

// `./GlobalVoiceHost` stays the module's public entry. The context bridge,
// the pure projections and the route policy now live in `context/` and
// `model/`; they are re-exported here so every existing import keeps working.
export {
  getVoiceSessionKey,
  isFinalTranscriptForSession,
  nextConversationRefresh,
  voiceStatusLabel,
} from "./model/projection";
export { mergeVoiceContext, useGlobalVoiceContext } from "./context/VoiceContext";
export { resolveVoiceContextForRoute } from "./model/routing";
export type {
  ConversationRefreshSignal,
  GlobalVoiceContextPatch,
  GlobalVoiceContextSnapshot,
  GlobalVoiceHostProps,
  VoiceSurfaceHostMode,
} from "./model/types";

export default function GlobalVoiceHost({
  children,
  initialSnapshot = null,
  initialPresence = null,
  initialExpanded = false,
  contextPatch,
}: GlobalVoiceHostProps) {
  const [snapshot, setSnapshot] = useState<VoiceRuntimeSnapshot | null>(initialSnapshot);
  const [, setPresence] = useState<PresenceSnapshot>(
    initialPresence ?? initialSnapshot?.presence ?? EMPTY_PRESENCE,
  );
  const [expanded, setExpanded] = useState(initialExpanded);
  const [notice, setNotice] = useState<string | null>(null);
  const [targetDescription, setTargetDescription] = useState("尚未解析");
  const [intentDescription, setIntentDescription] = useState("会话对话");
  const [handsFreeEnabled, setHandsFreeEnabled] = useState(false);
  const [globalConversationEnabled, setGlobalConversationEnabled] = useState(
    Boolean(initialSnapshot?.session && initialSnapshot.session.state !== "ended"),
  );
  const [awaitingConfirmation, setAwaitingConfirmation] = useState(false);
  const [conversationRefresh, setConversationRefresh] = useState<ConversationRefreshSignal | null>(null);
  const captureEchoRef = useRef<CaptureEchoEvidence | null>(null);
  const chatDraftCaptureRef = useRef(false);
  const chatOwnedSessionRef = useRef<string | null>(null);
  const pendingChatCaptureRef = useRef(false);
  const chatFinalSubscribersRef = useRef(new Set<(text: string) => void>());
  const pendingManualSpeechRef = useRef<string | null>(null);
  const manualSpeechOwnedSessionRef = useRef<string | null>(null);
  const manualSpeechStartedRef = useRef(false);
  const endSessionRef = useRef<() => Promise<void>>(async () => undefined);
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
    if (initialSnapshot?.session && initialSnapshot.session.state !== "ended") {
      setGlobalConversationEnabled(true);
    }
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
          if (normalized.session && normalized.session.state !== "ended") {
            setGlobalConversationEnabled(true);
          }
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
    chatDraftCaptureRef.current = false;
    pendingChatCaptureRef.current = false;
    chatFinalSubscribersRef.current.clear();
    pendingManualSpeechRef.current = null;
    continuationControllerRef.current?.abort();
  }, []);

  const finishChatOwnedSession = useCallback(() => {
    chatDraftCaptureRef.current = false;
    pendingChatCaptureRef.current = false;
    if (!chatOwnedSessionRef.current) return;
    chatOwnedSessionRef.current = null;
    void endSessionRef.current();
  }, []);

  const handleFinalTranscript = useCallback(
    async (result: FinalVoiceTranscript) => {
      const current = latestSessionRef.current;
      const dispatchEpoch = dispatchEpochRef.current;
      if (!current || !isFinalTranscriptForSession(result, current)
        || current.conversational_anchor?.conversation_id !== latestContextRef.current.conversational_anchor?.conversation_id) {
        setNotice("这段语音来自旧会话，已安全丢弃。");
        finishChatOwnedSession();
        return;
      }
      const echoEvidence = captureEchoRef.current;
      captureEchoRef.current = null;
      if (shouldSuppressCaptureEcho(result, echoEvidence, contextIdentityEpochRef.current, performance.now())) {
        setNotice("已忽略疑似播报回声，请重新说话。");
        finishChatOwnedSession();
        return;
      }
      if (chatDraftCaptureRef.current) {
        setSnapshot((previous) =>
          previous
            ? {
                ...previous,
                lease: null,
                partial_transcript: null,
                final_transcript: result.text,
                session: { ...current, state: "listening", updated_at: Date.now() },
              }
            : previous,
        );
        for (const listener of chatFinalSubscribersRef.current) listener(result.text);
        finishChatOwnedSession();
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
        setAwaitingConfirmation(continuation.kind === "approval");
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
          setAwaitingConfirmation(false);
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
    [finishChatOwnedSession, playback],
  );

  const interruptForBargeIn = useCallback(async (): Promise<GlobalVoiceSession | void> => {
    return runVoiceBargeIn(session, {
      bumpEpoch: () => ++dispatchEpochRef.current,
      readEpoch: () => dispatchEpochRef.current,
      readLatestSession: () => latestSessionRef.current,
      readContextAnchorId: () => latestContextRef.current.conversational_anchor?.conversation_id,
      abortContinuation: () => {
        continuationControllerRef.current?.abort();
        continuationControllerRef.current = null;
      },
      interruptPlayback: playback.interrupt,
      interruptVoiceSession,
      commitSession: (interrupted) => {
        latestSessionRef.current = interrupted;
        setSnapshot((previous) =>
          previous
            ? { ...previous, session: interrupted, lease: null, partial_transcript: null }
            : previous,
        );
        setPresence((previous) => ({ ...previous, interaction: "interrupted" }));
      },
    });
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
        chatDraftCaptureRef.current = false;
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

  const startSession = useCallback(async (showGlobalConversation = true): Promise<GlobalVoiceSession | null> => {
    captureStartEpochRef.current += 1;
    captureEchoRef.current = null;
    dispatchEpochRef.current += 1;
    const requestEpoch = dispatchEpochRef.current;
    continuationControllerRef.current?.abort();
    continuationControllerRef.current = null;
    playback.interrupt();
    const started = await startVoiceSession(voiceContext.focused_surface);
    if (dispatchEpochRef.current !== requestEpoch) return null;
    if (!started) {
      setNotice("语音会话未能启动，请检查后端与语音配置。");
      return null;
    }
    setNotice(null);
    if (showGlobalConversation) setGlobalConversationEnabled(true);
    latestSessionRef.current = started;
    setSnapshot((previous) => snapshotWithSession(previous, started));
    setPresence((previous) => ({ ...previous, activity: "working", interaction: "listening" }));
    return started;
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
    chatDraftCaptureRef.current = false;
    pendingChatCaptureRef.current = false;
    chatOwnedSessionRef.current = null;
    pendingManualSpeechRef.current = null;
    manualSpeechOwnedSessionRef.current = null;
    manualSpeechStartedRef.current = false;
    setGlobalConversationEnabled(false);
    setNotice(null);
  }, [capture.cancel, playback.interrupt, session]);
  endSessionRef.current = endSession;

  const startCapture = useCallback(async (chatDraft = false) => {
    const operationEpoch = ++captureStartEpochRef.current;
    const identityEpoch = contextIdentityEpochRef.current;
    chatDraftCaptureRef.current = chatDraft;
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

  const subscribeChatFinal = useCallback((listener: (text: string) => void) => {
    chatFinalSubscribersRef.current.add(listener);
    return () => chatFinalSubscribersRef.current.delete(listener);
  }, []);

  const cancelChatCapture = useCallback(() => {
    capture.cancel();
    finishChatOwnedSession();
  }, [capture.cancel, finishChatOwnedSession]);

  const startChatCapture = useCallback(async () => {
    if (capture.state.status !== "idle" && capture.state.status !== "error") return;
    chatDraftCaptureRef.current = true;
    setNotice(null);
    if (session && session.state !== "ended") {
      await startCapture(true);
      return;
    }
    pendingChatCaptureRef.current = true;
    const started = await startSession(false);
    if (!started) {
      pendingChatCaptureRef.current = false;
      chatDraftCaptureRef.current = false;
      return;
    }
    chatOwnedSessionRef.current = started.voice_session_id;
  }, [capture.state.status, session, startCapture, startSession]);

  useEffect(() => {
    if (!pendingChatCaptureRef.current || !session || session.state === "ended") return;
    pendingChatCaptureRef.current = false;
    void startCapture(true);
  }, [session?.generation, session?.state, session?.voice_session_id, startCapture]);

  const chatVoiceControls = useMemo<ChatVoiceControls>(() => ({
    status: capture.state.status,
    volume: capture.state.volume,
    start: () => { void startChatCapture(); },
    stop: capture.stop,
    cancel: cancelChatCapture,
    subscribeFinal: subscribeChatFinal,
  }), [
    cancelChatCapture,
    capture.state.status,
    capture.state.volume,
    capture.stop,
    startChatCapture,
    subscribeChatFinal,
  ]);

  const requestGlobalVoiceSession = useCallback(() => {
    setGlobalConversationEnabled(true);
    setExpanded(true);
    setHandsFreeEnabled(true);
    if (!session || session.state === "ended") void startSession(true);
  }, [session, startSession]);

  const speakAssistantMessage = useCallback((text: string) => {
    const message = text.trim();
    if (!message) return;
    if (session && session.state !== "ended") {
      void playbackRef.current.speak(message);
      return;
    }
    pendingManualSpeechRef.current = message;
    manualSpeechStartedRef.current = false;
    void startSession(false).then((started) => {
      if (started) manualSpeechOwnedSessionRef.current = started.voice_session_id;
      else pendingManualSpeechRef.current = null;
    });
  }, [session, startSession]);

  useEffect(() => {
    const message = pendingManualSpeechRef.current;
    if (!message || !session || session.state === "ended") return;
    pendingManualSpeechRef.current = null;
    manualSpeechStartedRef.current = true;
    void playbackRef.current.speak(message);
  }, [session?.generation, session?.state, session?.voice_session_id]);

  useEffect(() => {
    if (!manualSpeechOwnedSessionRef.current || !manualSpeechStartedRef.current) return;
    if (playback.state.status !== "ended" && playback.state.status !== "error") return;
    manualSpeechOwnedSessionRef.current = null;
    manualSpeechStartedRef.current = false;
    void endSessionRef.current();
  }, [playback.state.status]);

  useEffect(() => {
    if (playback.state.status === "playing") {
      setPresence((previous) => ({ ...previous, activity: "working", interaction: "speaking" }));
    } else if (capture.state.status === "listening" || capture.state.status === "transcribing") {
      setPresence((previous) => ({ ...previous, activity: "working", interaction: "listening" }));
    }
  }, [capture.state.status, playback.state.status]);

  const productState = useMemo<GlobalVoiceProductState>(() => {
    if (capture.state.status === "error" || playback.state.status === "error" || notice) {
      return "error";
    }
    if (awaitingConfirmation) return "confirmation";
    if (
      playback.state.status === "synthesizing" ||
      playback.state.status === "playing" ||
      playback.state.status === "paused"
    ) {
      return "speaking";
    }
    if (
      capture.state.status === "acquiring" ||
      capture.state.status === "transcribing" ||
      session?.state === "processing"
    ) {
      return "understanding";
    }
    return "listening";
  }, [
    awaitingConfirmation,
    capture.state.status,
    notice,
    playback.state.status,
    session?.state,
  ]);

  const currentContextDescription = [
    voiceContext.active_task ? `当前任务：${voiceContext.active_task}` : "",
    voiceContext.conversational_anchor?.title
      ? `当前对话：${voiceContext.conversational_anchor.title}`
      : "",
  ].filter(Boolean).join(" · ") || "当前页面";
  const captureActive = capture.state.status === "acquiring"
    || capture.state.status === "listening"
    || capture.state.status === "transcribing";

  const contextBridge = useMemo<GlobalVoiceContextBridgeValue>(() => ({
    context: voiceContext,
    conversationRefresh,
    session,
    updateContext,
    chatVoiceControls,
    requestGlobalVoiceSession,
    speakAssistantMessage,
  }), [
    chatVoiceControls,
    conversationRefresh,
    requestGlobalVoiceSession,
    session,
    speakAssistantMessage,
    updateContext,
    voiceContext,
  ]);

  return (
    <GlobalVoiceContextBridge.Provider value={contextBridge}>
      <div className="global-voice-host">
        {children}
        {globalConversationEnabled ? (
          <GlobalVoiceLeaf
            state={productState}
            expanded={expanded}
            heardText={(finalTranscript || snapshot?.partial_transcript) || undefined}
            currentContext={currentContextDescription}
            interpretedAction={intentDescription || targetDescription}
            microphoneLabel={capture.state.status === "listening"
              ? "停止录音"
              : captureActive
                ? "取消输入"
                : "开始说话"}
            soundLabel={playback.state.muted ? "打开声音" : "静音"}
            onToggleExpanded={() => setExpanded((value) => !value)}
            onMicrophone={() => {
              if (capture.state.status === "listening") capture.stop();
              else if (captureActive) capture.cancel();
              else if (session && session.state !== "ended") void startCapture(false);
              else requestGlobalVoiceSession();
            }}
            onSound={playback.toggleMute}
            onEnd={() => void endSession()}
            soundDisabled={!session || session.state === "ended"}
            endDisabled={!session || session.state === "ended"}
            notice={capture.state.error ?? notice}
          />
        ) : null}
      </div>
    </GlobalVoiceContextBridge.Provider>
  );
}

export { GlobalVoiceHost };
