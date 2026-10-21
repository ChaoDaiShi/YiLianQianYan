import { useCallConnection } from "./runtime/useCallConnection";
import { useVoiceProductState } from "./model/useVoiceProductState";
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
import { GlobalVoiceContextBridge, projectVoiceContextPatch, selectBrowsingCallContext } from "./context/VoiceContext";
import {
  EMPTY_PRESENCE,
  contextFromSession,
  contextMatchesSession,
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
import { runFinalTranscriptFlow, type TurnFlowRuntime } from "./turnFlow";
import { useHandsFree } from "./useHandsFree";
import { synchronizeVoiceContext } from "./voiceContextSync";
import { type ChatVoiceControls } from "./ChatVoiceInput";
import { GlobalVoiceLeaf } from "./GlobalVoiceLeaf";
import { useGlobalVoiceRuntime } from "./runtime/useGlobalVoiceRuntime";
import "./globalVoice.css";
import { desktopFloatingAvailable, useFloatingPublisher } from "./floatingSurface";
import type { CallPresentation } from "./callPresentation";

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
  const [routeNotice,setRouteNotice]=useState<string|null>(null);
  const [targetDescription, setTargetDescription] = useState("尚未解析");
  const [intentDescription, setIntentDescription] = useState("会话对话");
  const [handsFreeEnabled, setHandsFreeEnabled] = useState(false);
  const [connecting,setConnecting]=useState(false);
  const [microphoneMuted,setMicrophoneMuted]=useState(false);
  const [connectedAt,setConnectedAt]=useState<string|null>(null);
  const [callChecks,setCallChecks]=useState({session:false,microphone:false,stt:false,tts:false,output:false});
  const callStartEpoch=useRef(0);

  const [globalConversationEnabled, setGlobalConversationEnabled] = useState(
    Boolean(initialSnapshot?.session && initialSnapshot.session.state !== "ended"),
  );
  const [awaitingConfirmation, setAwaitingConfirmation] = useState(false);
  const [conversationRefresh, setConversationRefresh] = useState<ConversationRefreshSignal | null>(null);
  const runtime = useGlobalVoiceRuntime(initialSnapshot?.session ?? null);
  const chatDraftCaptureRef = useRef(false);
  const chatOwnedSessionRef = useRef<string | null>(null);
  const pendingChatCaptureRef = useRef(false);
  const chatFinalSubscribersRef = useRef(new Set<(text: string) => void>());
  const pendingManualSpeechRef = useRef<string | null>(null);
  const manualSpeechOwnedSessionRef = useRef<string | null>(null);
  const manualSpeechStartedRef = useRef(false);
  const endSessionRef = useRef<() => Promise<void>>(async () => undefined);
  const invalidateContextRef = useRef<() => void>(() => undefined);
  const localContextTouchedRef = useRef(false);
  const [voiceContext, setVoiceContext] = useState<GlobalVoiceContextSnapshot>(() =>
    contextFromSession(initialSnapshot?.session ?? null),
  );
  runtime.commitContext(voiceContext);
  const [browsingContext,setBrowsingContext]=useState(voiceContext);
  const browsingContextRef=useRef(voiceContext);
  const callPinnedRef=useRef(Boolean(initialSnapshot?.session&&initialSnapshot.session.state!=="ended"));
  const pendingAnchorSwitchRef=useRef<string|null>(null);
  const microphoneMutedRef=useRef(microphoneMuted);microphoneMutedRef.current=microphoneMuted;


  const updateContext = useCallback((patch: GlobalVoiceContextPatch) => {
    localContextTouchedRef.current = true;
    const previous=runtime.readContext();
    const projection=projectVoiceContextPatch(previous,browsingContextRef.current,patch,callPinnedRef.current);
    browsingContextRef.current=projection.browsing;setBrowsingContext(projection.browsing);
    if(projection.active===previous)return;
    const next=projection.active;
    if (next.conversational_anchor?.conversation_id !== previous.conversational_anchor?.conversation_id) {
      runtime.bumpIdentityEpoch();
      invalidateContextRef.current();
    }
    runtime.commitContext(next);
    setVoiceContext(next);
  }, [runtime]);

  const session = snapshot?.session ?? null;
  runtime.commitSession(session);
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
    const epoch = runtime.readEpoch();
    void synchronizeVoiceContext({
      session,
      signal: controller.signal,
      getContext: () => runtime.readContext(),
      isCurrent: () => runtime.readEpoch() === epoch
        && runtime.readLatestSession()?.voice_session_id === session.voice_session_id
        && runtime.readLatestSession()?.generation === session.generation,
    }).then((updated) => {
      if (!updated || controller.signal.aborted) return;
      setSnapshot((previous) => {
        if (
          controller.signal.aborted || runtime.readEpoch() !== epoch
          || !contextMatchesSession(runtime.readContext(), updated) ||
          !previous?.session ||
          previous.session.voice_session_id !== session.voice_session_id ||
          previous.session.generation !== session.generation
        ) {
          return previous;
        }
        return { ...previous, session: updated };
      });
      if(pendingAnchorSwitchRef.current===updated.conversational_anchor?.conversation_id && contextMatchesSession(runtime.readContext(),updated)){pendingAnchorSwitchRef.current=null;setHandsFreeEnabled(!microphoneMutedRef.current);if(microphoneMutedRef.current)setConnecting(false);}
    }).catch((error) => {
      if (controller.signal.aborted || runtime.readEpoch() !== epoch) return;
      setNotice(error instanceof Error ? error.message : "语音上下文同步失败。");
    });
    return () => controller.abort();
  }, [
    session,
    voiceContext,
    runtime,
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
    runtime.invalidateInFlight();
    chatDraftCaptureRef.current = false;
    pendingChatCaptureRef.current = false;
    chatFinalSubscribersRef.current.clear();
    pendingManualSpeechRef.current = null;
  }, [runtime]);

  const finishChatOwnedSession = useCallback(() => {
    chatDraftCaptureRef.current = false;
    pendingChatCaptureRef.current = false;
    if (!chatOwnedSessionRef.current) return;
    chatOwnedSessionRef.current = null;
    void endSessionRef.current();
  }, []);

  const turnFlowRuntime = useMemo<TurnFlowRuntime>(() => ({
    readEpoch: runtime.readEpoch,
    readLatestSession: runtime.readLatestSession,
    readContextAnchorId: () => runtime.readContext().conversational_anchor?.conversation_id,
    readContextIdentityEpoch: runtime.readIdentityEpoch,
    readChatDraft: () => chatDraftCaptureRef.current,
    takeEchoEvidence: runtime.takeEchoEvidence,
    now: () => Date.now(),
    monotonicNow: () => performance.now(),
    forEachChatFinal: (deliver) => {
      for (const listener of chatFinalSubscribersRef.current) deliver(listener);
    },
    setNotice,
    setSnapshot,
    setTargetDescription,
    setIntentDescription,
    setConversationRefresh,
    setAwaitingConfirmation,
    finishChatOwnedSession,
    abortContinuation: runtime.abortContinuation,
    beginContinuation: runtime.beginContinuation,
    endContinuation: runtime.endContinuation,
    dispatchVoiceTurn,
    runContinuation:(continuation,options)=>runVoiceContinuation(continuation,{...options,onEvent:event=>{options.onEvent?.(event);if(event.type==="model_route"){const route=event as unknown as {phase:string;from_tier:number|null;to_tier:number|null};if(route.phase==="switched"&&route.to_tier!==route.from_tier)setRouteNotice(`已切换到第 ${route.to_tier} 级模型`);else if(route.phase==="recovered")setRouteNotice("已恢复使用一级模型");}}}),
    speak: (text) => playbackRef.current.speak(text),
  }), [finishChatOwnedSession, runtime]);

  const handleFinalTranscript = useCallback(
    (result: FinalVoiceTranscript) => runFinalTranscriptFlow(result, turnFlowRuntime),
    [turnFlowRuntime],
  );

  const interruptForBargeIn = useCallback(async (): Promise<GlobalVoiceSession | void> => {
    return runVoiceBargeIn(session, {
      bumpEpoch: runtime.bumpEpoch,
      readEpoch: runtime.readEpoch,
      readLatestSession: runtime.readLatestSession,
      readContextAnchorId: () => runtime.readContext().conversational_anchor?.conversation_id,
      abortContinuation: runtime.abortContinuation,
      interruptPlayback: playback.interrupt,
      interruptVoiceSession,
      commitSession: (interrupted) => {
        runtime.commitSession(interrupted);
        setSnapshot((previous) =>
          previous
            ? { ...previous, session: interrupted, lease: null, partial_transcript: null }
            : previous,
        );
        setPresence((previous) => ({ ...previous, interaction: "interrupted" }));
      },
    });
  }, [playback.interrupt, runtime, session]);

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
    identityEpoch: runtime.readIdentityEpoch(),
    playing: playback.state.status === "playing" || playback.state.status === "synthesizing",
    capture: {
      ...capture,
      start: async (reason, stream) => {
        const operation = runtime.beginCaptureOperation();
        chatDraftCaptureRef.current = false;
        // Snapshot only actual audio playback, before barge-in releases it.
        const overlap = playbackRef.current.captureOverlap();
        const handle = await capture.start(reason, stream);
        runtime.recordEchoEvidence(
          overlap,
          handle?.lease ?? null,
          operation.operationEpoch,
          operation.identityEpoch,
        );
        return handle;
      },
    },
    onReady:()=>{setCallChecks(value=>({...value,microphone:true}));setConnecting(false);setConnectedAt(value=>value??new Date().toISOString());},
    onError: (message) => { setConnecting(false);setHandsFreeEnabled(false); capture.cancel(); setNotice(message); },
  });

  const startSession = useCallback(async (showGlobalConversation = true): Promise<GlobalVoiceSession | null> => {
    runtime.beginCaptureOperation();
    const requestEpoch = runtime.bumpEpoch();
    runtime.abortContinuation();
    playback.interrupt();
    const started = await startVoiceSession(voiceContext.focused_surface);
    if (runtime.readEpoch() !== requestEpoch) return null;
    if (!started) {
      setNotice("语音会话未能启动，请检查后端与语音配置。");
      return null;
    }
    setNotice(null);
    if (showGlobalConversation) setGlobalConversationEnabled(true);
    runtime.commitSession(started);
    setSnapshot((previous) => snapshotWithSession(previous, started));
    setPresence((previous) => ({ ...previous, activity: "working", interaction: "listening" }));
    return started;
  }, [playback, runtime, voiceContext.focused_surface]);

  const endSession = useCallback(async () => {
    callPinnedRef.current=false;pendingAnchorSwitchRef.current=null;
    callStartEpoch.current+=1;setConnecting(false);setHandsFreeEnabled(false);setConnectedAt(null);
    runtime.bumpEpoch();runtime.abortContinuation();capture.cancel();playback.interrupt();
    runtime.commitContext(browsingContextRef.current);setVoiceContext(browsingContextRef.current);
    if (!session){setGlobalConversationEnabled(false);return;}
    runtime.beginCaptureOperation();
    setHandsFreeEnabled(false);
    const requestEpoch = runtime.bumpEpoch();
    runtime.abortContinuation();
    await cleanupVoiceSession({
      sessionId: session.voice_session_id,
      generation: session.generation,
      cancelCapture: capture.cancel,
      interruptPlayback: playback.interrupt,
      endSession: async (sessionId, generation) => {
        const ended = await endVoiceSession(sessionId, generation);
        if (ended && runtime.readEpoch() === requestEpoch) {
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
    if(runtime.readEpoch()!==requestEpoch)return;
    chatDraftCaptureRef.current = false;
    pendingChatCaptureRef.current = false;
    chatOwnedSessionRef.current = null;
    pendingManualSpeechRef.current = null;
    manualSpeechOwnedSessionRef.current = null;
    manualSpeechStartedRef.current = false;
    setGlobalConversationEnabled(false);
    setNotice(null);
  }, [capture.cancel, playback.interrupt, runtime, session]);
  endSessionRef.current = endSession;

  const startCapture = useCallback(async (chatDraft = false) => {
    const operation = runtime.beginCaptureOperation();
    chatDraftCaptureRef.current = chatDraft;
    const overlap = playbackRef.current.captureOverlap();
    const handle = await capture.start("user-click");
    runtime.recordEchoEvidence(
      overlap,
      handle?.lease ?? null,
      operation.operationEpoch,
      operation.identityEpoch,
    );
  }, [capture, runtime]);

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

  const requestGlobalVoiceSession=useCallConnection({session,epoch:callStartEpoch,startSession:()=>startSession(true),
    onStart:()=>{callPinnedRef.current=true;setGlobalConversationEnabled(true);setExpanded(true);setConnecting(true);setNotice(null);setMicrophoneMuted(false);setHandsFreeEnabled(false);setConnectedAt(null);setCallChecks({session:false,microphone:false,stt:false,tts:false,output:false});},
    onOutputReady:()=>setCallChecks(value=>({...value,output:true})),onServicesReady:(stt,tts)=>setCallChecks(value=>({...value,stt,tts})),
    onSessionReady:()=>{setCallChecks(value=>({...value,session:true}));setHandsFreeEnabled(true);},onFailure:message=>{setConnecting(false);setHandsFreeEnabled(false);setNotice(message);},
  });
  const toggleMicrophone=useCallback(()=>{const next=!microphoneMuted;setMicrophoneMuted(next);setHandsFreeEnabled(!next);if(next)capture.cancel();},[capture.cancel,microphoneMuted]);

  const switchToBrowsingConversation=useCallback(()=>{
    const previous=runtime.readContext();const next=selectBrowsingCallContext(previous,browsingContextRef.current);
    if(next===previous)return;
    setHandsFreeEnabled(false);setConnecting(true);pendingAnchorSwitchRef.current=next.conversational_anchor!.conversation_id;
    runtime.beginCaptureOperation();runtime.bumpEpoch();runtime.bumpIdentityEpoch();runtime.abortContinuation();capture.cancel();playback.interrupt();
    runtime.commitContext(next);setVoiceContext(next);
  },[runtime,capture.cancel,playback.interrupt]);

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

  const productState=useVoiceProductState({capture:capture.state.status,playback:playback.state.status,notice,connecting,muted:microphoneMuted,awaitingConfirmation,session:session?.state});

  const currentContextDescription = [
    voiceContext.active_task ? `当前任务：${voiceContext.active_task}` : "",
    voiceContext.conversational_anchor?.title
      ? `当前对话：${voiceContext.conversational_anchor.title}`
      : "",
  ].filter(Boolean).join(" · ") || "当前页面";


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

  const callPresentation=useMemo<CallPresentation>(()=>({session_id:session?.voice_session_id??null,phase:!globalConversationEnabled?"ended":productState==="understanding"||productState==="confirmation"?"thinking":productState,conversation_id:session?.conversational_anchor?.conversation_id??null,muted:microphoneMuted,speaker_enabled:!playback.state.muted,connected_at:connectedAt,caption:finalTranscript||snapshot?.partial_transcript||"",notice:notice??routeNotice,checks:callChecks,conversation_title:session?.conversational_anchor?.title??null,browsing_conversation_title:browsingContext.conversational_anchor?.title??null,can_switch_conversation:globalConversationEnabled&&!connecting&&browsingContext.focused_surface==="conversation"&&!!browsingContext.conversational_anchor&&browsingContext.conversational_anchor.conversation_id!==session?.conversational_anchor?.conversation_id}),[session,globalConversationEnabled,productState,microphoneMuted,playback.state.muted,connectedAt,finalTranscript,snapshot?.partial_transcript,notice,routeNotice,callChecks,browsingContext,connecting]);
  useFloatingPublisher("call",callPresentation,action=>{if(action==="end")void endSession();else if(action==="microphone")toggleMicrophone();else if(action==="speaker")playback.toggleMute();else if(action==="start")requestGlobalVoiceSession();else if(action==="switch_conversation")switchToBrowsingConversation();});
  return (
    <GlobalVoiceContextBridge.Provider value={contextBridge}>
      <div className="global-voice-host">
        {children}
        {globalConversationEnabled && !desktopFloatingAvailable() ? (
          <GlobalVoiceLeaf
            state={productState}
            expanded={expanded}
            heardText={(finalTranscript || snapshot?.partial_transcript) || undefined}
            currentContext={currentContextDescription}
            interpretedAction={intentDescription || targetDescription}
            microphoneLabel={microphoneMuted?"取消麦克风静音":"麦克风静音"}
            soundLabel={playback.state.muted ? "打开声音" : "静音"}
            onToggleExpanded={() => setExpanded((value) => !value)}
            onMicrophone={toggleMicrophone}
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
