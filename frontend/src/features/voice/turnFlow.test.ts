import { describe, expect, it } from "vitest";
import type {
  GlobalVoiceSession,
  VoiceContinuation,
  VoiceDispatchResult,
  VoiceRuntimeSnapshot,
  VoiceTurn,
} from "../../api/voice";
import type { VoiceContinuationResult } from "./voiceContinuation";
import type { FinalVoiceTranscript } from "./useVoiceCapture";
import type { CaptureEchoEvidence } from "./echoEvidence";
import { runFinalTranscriptFlow, type TurnFlowRuntime } from "./turnFlow";

const session: GlobalVoiceSession = {
  voice_session_id: "voice-session-6",
  generation: 4,
  state: "listening",
  focused_surface: "conversation",
  conversational_anchor: {
    conversation_id: "conversation-7",
    title: "Rust 练习",
    updated_at: 1726000000000,
  },
  active_task: null,
  attention_mode: "balanced",
  started_at: 1726000000000,
  updated_at: 1726000001000,
  schema_version: 1,
};

const result: FinalVoiceTranscript = {
  text: "请继续 Rust 练习",
  sessionId: "voice-session-6",
  generation: 4,
  leaseId: "lease-9",
};

const turn: VoiceTurn = {
  turn_id: "turn-1",
  voice_session_id: "voice-session-6",
  generation: 4,
  lease_id: "lease-9",
  source: "voice",
  final_transcript: "请继续 Rust 练习",
  focused_surface: "conversation",
  anchor_snapshot: {
    focused_surface: "conversation",
    conversational_anchor: session.conversational_anchor,
    active_task: null,
  },
  resolved_target: { status: "resolved", target: {} },
  intent: { kind: "conversation_turn" },
  created_at: 1726000000000,
  schema_version: 1,
};

const routed = (continuation: VoiceContinuation | null = null, narration: string | null = "好的，继续。"): VoiceDispatchResult => ({
  turn,
  narration,
  continuation,
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

interface Harness {
  calls: string[];
  notices: string[];
  chatFinals: string[];
  spoken: string[];
  runtime: TurnFlowRuntime;
  setEpoch: (value: number) => void;
  setLatest: (value: GlobalVoiceSession | null) => void;
  setAnchorId: (value: string | undefined) => void;
  setChatDraft: (value: boolean) => void;
  setEcho: (value: CaptureEchoEvidence | null) => void;
  dispatch: ReturnType<typeof deferred<VoiceDispatchResult | null>>;
  continuation: ReturnType<typeof deferred<VoiceContinuationResult>>;
  speak: ReturnType<typeof deferred<void>>;
  continuationController: AbortController | null;
}

function makeHarness(): Harness {
  const calls: string[] = [];
  const notices: string[] = [];
  const chatFinals: string[] = [];
  const spoken: string[] = [];
  const chatFinalSubscribers: ((text: string) => void)[] = [(text) => chatFinals.push(text)];

  let epoch = 0;
  let latest: GlobalVoiceSession | null = session;
  let anchorId: string | undefined = session.conversational_anchor!.conversation_id;
  let chatDraft = false;
  let echo: CaptureEchoEvidence | null = null;
  let snap: VoiceRuntimeSnapshot | null = {
    session,
    lease: null,
    partial_transcript: null,
    final_transcript: null,
    turns: [],
    presence: {
      activity: "working",
      interaction: "listening",
      attention: "none",
      source: "voice",
      updated_at: 0,
    },
  };
  let continuationController: AbortController | null = null;

  const dispatch = deferred<VoiceDispatchResult | null>();
  const continuation = deferred<VoiceContinuationResult>();
  const speak = deferred<void>();

  const runtime: TurnFlowRuntime = {
    readEpoch: () => epoch,
    readLatestSession: () => latest,
    readContextAnchorId: () => anchorId,
    readContextIdentityEpoch: () => 0,
    readChatDraft: () => chatDraft,
    takeEchoEvidence: () => {
      const evidence = echo;
      echo = null;
      return evidence;
    },
    now: () => 1726000000000,
    monotonicNow: () => 1000,
    forEachChatFinal: (deliver) => {
      for (const subscriber of chatFinalSubscribers) deliver(subscriber);
    },
    setNotice: (text) => notices.push(text),
    setSnapshot: (update) => {
      snap = update(snap);
      calls.push(`snapshot:${snap?.session?.state}:${snap?.turns.length}:${snap?.final_transcript ?? ""}`);
    },
    setTargetDescription: (text) => {
      calls.push(`target:${text}`);
    },
    setIntentDescription: (text) => {
      calls.push(`intent:${text}`);
    },
    setConversationRefresh: (update) => {
      const next = update(null)!;
      calls.push(`refresh:${next.conversation_id}:${next.revision}`);
    },
    setAwaitingConfirmation: (value) => {
      calls.push(`awaiting:${value}`);
    },
    finishChatOwnedSession: () => calls.push("finish-owned"),
    abortContinuation: () => calls.push("abort-continuation"),
    beginContinuation: () => {
      continuationController = new AbortController();
      calls.push("begin-continuation");
      return continuationController;
    },
    endContinuation: () => {
      continuationController = null;
      calls.push("end-continuation");
    },
    dispatchVoiceTurn: () => {
      calls.push("dispatch");
      return dispatch.promise;
    },
    runContinuation: (_continuation, _options) => {
      calls.push("run-continuation");
      return continuation.promise;
    },
    speak: (text) => {
      spoken.push(text);
      calls.push(`speak:${text}`);
      return speak.promise;
    },
  };

  return {
    calls,
    notices,
    chatFinals,
    spoken,
    runtime,
    setEpoch: (value) => {
      epoch = value;
    },
    setLatest: (value) => {
      latest = value;
    },
    setAnchorId: (value) => {
      anchorId = value;
    },
    setChatDraft: (value) => {
      chatDraft = value;
    },
    setEcho: (value) => {
      echo = value;
    },
    dispatch,
    continuation,
    speak,
    get continuationController() {
      return continuationController;
    },
  };
}

const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

describe("runFinalTranscriptFlow", () => {
  it("drops a transcript from a stale generation without dispatching", async () => {
    const h = makeHarness();
    h.setLatest({ ...session, generation: 5 });

    await runFinalTranscriptFlow(result, h.runtime);

    expect(h.notices).toEqual(["这段语音来自旧会话，已安全丢弃。"]);
    expect(h.calls).toEqual(["finish-owned"]);
  });

  it("drops a transcript whose conversation anchor no longer matches", async () => {
    const h = makeHarness();
    h.setAnchorId("conversation-other");

    await runFinalTranscriptFlow(result, h.runtime);

    expect(h.notices).toEqual(["这段语音来自旧会话，已安全丢弃。"]);
    expect(h.calls).toEqual(["finish-owned"]);
  });

  it("suppresses a transcript that is the assistant's own voice played back", async () => {
    const h = makeHarness();
    h.setEcho({
      text: "这是小涟的播报内容",
      sessionId: "voice-session-6",
      playbackGeneration: 4,
      observedAt: 1000,
      generation: 4,
      leaseId: "lease-9",
      identityEpoch: 0,
    });
    const echoResult = { ...result, text: "小涟的播报" };

    await runFinalTranscriptFlow(echoResult, h.runtime);

    expect(h.notices).toEqual(["已忽略疑似播报回声，请重新说话。"]);
    expect(h.calls).toEqual(["finish-owned"]);
  });

  it("delivers a chat-draft transcript to subscribers without dispatching", async () => {
    const h = makeHarness();
    h.setChatDraft(true);

    await runFinalTranscriptFlow(result, h.runtime);

    expect(h.chatFinals).toEqual(["请继续 Rust 练习"]);
    expect(h.calls).toEqual(["snapshot:listening:0:请继续 Rust 练习", "finish-owned"]);
  });

  it("dispatches, appends the turn and speaks the narration", async () => {
    const h = makeHarness();
    const pending = runFinalTranscriptFlow(result, h.runtime);

    h.dispatch.resolve(routed());
    h.speak.resolve();
    await pending;

    expect(h.calls).toEqual([
      "snapshot:processing:0:请继续 Rust 练习",
      "dispatch",
      "target:服务端已解析",
      "intent:会话对话（服务端）",
      "snapshot:processing:1:请继续 Rust 练习",
      "speak:好的，继续。",
    ]);
  });

  it("reports when dispatch returns no route", async () => {
    const h = makeHarness();
    const pending = runFinalTranscriptFlow(result, h.runtime);

    h.dispatch.resolve(null);
    await pending;

    expect(h.notices).toEqual(["最终语音已接收，但交互路由暂不可用。"]);
    expect(h.spoken).toEqual([]);
  });

  it("discards a dispatch result that resolves after a newer epoch", async () => {
    const h = makeHarness();
    const pending = runFinalTranscriptFlow(result, h.runtime);

    // A newer dispatch lands while the route is in flight.
    h.setEpoch(1);
    h.dispatch.resolve(routed());
    await pending;

    expect(h.spoken).toEqual([]);
    expect(h.calls).not.toContain("target:服务端已解析");
  });

  it("runs a conversation continuation, refreshes the conversation and speaks its narration", async () => {
    const h = makeHarness();
    const pending = runFinalTranscriptFlow(result, h.runtime);

    h.dispatch.resolve(routed({ kind: "conversation", conversation_id: "conversation-7", message: "继续" }));
    h.continuation.resolve({ narration: "已经继续了。", verified: true });
    h.speak.resolve();
    await pending;

    expect(h.calls).toEqual([
      "snapshot:processing:0:请继续 Rust 练习",
      "dispatch",
      "target:服务端已解析",
      "intent:会话对话（服务端）",
      "snapshot:processing:1:请继续 Rust 练习",
      "awaiting:false",
      "abort-continuation",
      "begin-continuation",
      "run-continuation",
      "refresh:conversation-7:1",
      "awaiting:false",
      "end-continuation",
      "speak:已经继续了。",
    ]);
  });

  it("holds confirmation state across an approval continuation", async () => {
    const h = makeHarness();
    const pending = runFinalTranscriptFlow(result, h.runtime);

    h.dispatch.resolve(routed({
      kind: "approval",
      approval_id: "approval-1",
      conversation_id: "conversation-7",
      attestation_id: "attest-1",
      decision: "approve",
    }));
    h.continuation.resolve({ narration: "审批已通过。", verified: true });
    h.speak.resolve();
    await pending;

    const awaiting = h.calls.filter((call) => call.startsWith("awaiting:"));
    expect(awaiting).toEqual(["awaiting:true", "awaiting:false"]);
    expect(h.spoken).toEqual(["审批已通过。"]);
  });

  it("silently drops a continuation that a newer barge-in aborted", async () => {
    const h = makeHarness();
    const pending = runFinalTranscriptFlow(result, h.runtime);

    h.dispatch.resolve(routed({ kind: "conversation", conversation_id: "conversation-7", message: "继续" }));
    await flush();

    // A barge-in aborts the in-flight continuation, then it rejects.
    h.continuationController!.abort();
    h.continuation.reject(new Error("语音 continuation 已取消"));
    await pending;

    expect(h.notices).toEqual([]);
    expect(h.spoken).toEqual([]);
    expect(h.calls).toContain("end-continuation");
  });

  it("surfaces a continuation failure that was not caused by an abort", async () => {
    const h = makeHarness();
    const pending = runFinalTranscriptFlow(result, h.runtime);

    h.dispatch.resolve(routed({ kind: "conversation", conversation_id: "conversation-7", message: "继续" }));
    await flush();
    h.continuation.reject(new Error("会话响应未完成，不能交给 TTS"));
    await pending;

    expect(h.notices).toEqual(["会话响应未完成，不能交给 TTS"]);
    expect(h.spoken).toEqual([]);
  });

  it("does not speak narration while in silent attention mode", async () => {
    const h = makeHarness();
    h.setLatest({ ...session, attention_mode: "silent" });
    const pending = runFinalTranscriptFlow(result, h.runtime);

    h.dispatch.resolve(routed());
    await pending;

    expect(h.spoken).toEqual([]);
    expect(h.calls).toContain("snapshot:processing:1:请继续 Rust 练习");
  });
});
