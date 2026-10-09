import type {
  GlobalVoiceSession,
  VoiceContinuation,
  VoiceDispatchResult,
  VoiceRuntimeSnapshot,
  VoiceTurnDispatch,
} from "../../api/voice";
import type { RunVoiceContinuationOptions, VoiceContinuationResult } from "./voiceContinuation";
import type { FinalVoiceTranscript } from "./useVoiceCapture";
import type { CaptureEchoEvidence } from "./echoEvidence";
import { shouldSuppressCaptureEcho } from "./echoEvidence";
import { isFinalTranscriptForSession, nextConversationRefresh } from "./model/projection";
import type { ConversationRefreshSignal } from "./model/types";

/**
 * The final-transcript → dispatch → continuation → narration flow, extracted
 * from `GlobalVoiceHost` so its concurrency can be tested without a browser.
 *
 * The runtime injects every read, write and network call the flow needs, so
 * the flow itself is a pure function of `(result, runtime)`. The invariants it
 * preserves — the ones a static-markup render cannot reach — are:
 *
 *   - a transcript from a stale generation or conversation is dropped, not
 *     dispatched;
 *   - echo evidence suppresses a transcript that is just the assistant's own
 *     voice played back;
 *   - after every `await`, the dispatch epoch and session identity are re-read,
 *     and a result that resolves after a newer dispatch is discarded;
 *   - a continuation runs under an `AbortController` that a newer barge-in or
 *     session can abort, and its narration is spoken only while still current.
 */
export interface TurnFlowRuntime {
  readEpoch(): number;
  readLatestSession(): GlobalVoiceSession | null;
  readContextAnchorId(): string | undefined;
  readContextIdentityEpoch(): number;
  readChatDraft(): boolean;
  takeEchoEvidence(): CaptureEchoEvidence | null;
  /** Wall-clock timestamp for `updated_at` fields (Date.now). */
  now(): number;
  /** Monotonic clock for the echo-evidence TTL (performance.now). */
  monotonicNow(): number;
  forEachChatFinal(deliver: (subscriber: (text: string) => void) => void): void;

  setNotice(text: string): void;
  setSnapshot(update: (previous: VoiceRuntimeSnapshot | null) => VoiceRuntimeSnapshot | null): void;
  setTargetDescription(text: string): void;
  setIntentDescription(text: string): void;
  setConversationRefresh(
    update: (previous: ConversationRefreshSignal | null) => ConversationRefreshSignal | null,
  ): void;
  setAwaitingConfirmation(value: boolean): void;
  finishChatOwnedSession(): void;

  abortContinuation(): void;
  beginContinuation(): AbortController;
  endContinuation(controller: AbortController): void;

  dispatchVoiceTurn(turn: VoiceTurnDispatch): Promise<VoiceDispatchResult | null>;
  runContinuation(
    continuation: VoiceContinuation,
    options: RunVoiceContinuationOptions,
  ): Promise<VoiceContinuationResult>;
  speak(text: string): Promise<void>;
}

export async function runFinalTranscriptFlow(
  result: FinalVoiceTranscript,
  runtime: TurnFlowRuntime,
): Promise<void> {
  const current = runtime.readLatestSession();
  const dispatchEpoch = runtime.readEpoch();

  if (
    !current ||
    !isFinalTranscriptForSession(result, current) ||
    current.conversational_anchor?.conversation_id !== runtime.readContextAnchorId()
  ) {
    runtime.setNotice("这段语音来自旧会话，已安全丢弃。");
    runtime.finishChatOwnedSession();
    return;
  }

  const echoEvidence = runtime.takeEchoEvidence();
  if (
    shouldSuppressCaptureEcho(
      result,
      echoEvidence,
      runtime.readContextIdentityEpoch(),
      runtime.monotonicNow(),
    )
  ) {
    runtime.setNotice("已忽略疑似播报回声，请重新说话。");
    runtime.finishChatOwnedSession();
    return;
  }

  if (runtime.readChatDraft()) {
    runtime.setSnapshot((previous) =>
      previous
        ? {
            ...previous,
            lease: null,
            partial_transcript: null,
            final_transcript: result.text,
            session: { ...current, state: "listening", updated_at: runtime.now() },
          }
        : previous,
    );
    runtime.forEachChatFinal((listener) => listener(result.text));
    runtime.finishChatOwnedSession();
    return;
  }

  runtime.setSnapshot((previous) =>
    previous
      ? {
          ...previous,
          lease: null,
          partial_transcript: null,
          final_transcript: result.text,
          session: { ...current, state: "processing", updated_at: runtime.now() },
        }
      : previous,
  );

  const routed = await runtime.dispatchVoiceTurn({
    session_id: result.sessionId,
    generation: result.generation,
    lease_id: result.leaseId,
    final_transcript: result.text,
  });
  if (!routed) {
    runtime.setNotice("最终语音已接收，但交互路由暂不可用。");
    return;
  }

  const latest = runtime.readLatestSession();
  if (
    runtime.readEpoch() !== dispatchEpoch ||
    !latest ||
    latest.voice_session_id !== result.sessionId ||
    latest.generation !== result.generation
  ) {
    return;
  }

  runtime.setTargetDescription(
    routed.turn.resolved_target.status === "resolved" ? "服务端已解析" : "服务端未解析",
  );
  runtime.setIntentDescription(
    routed.turn.intent.kind === "conversation_turn" ? "会话对话（服务端）" : "服务端意图",
  );
  runtime.setSnapshot((previous) =>
    previous ? { ...previous, turns: [...previous.turns, routed.turn] } : previous,
  );

  let narration = routed.narration ?? null;
  if (routed.continuation) {
    const continuation = routed.continuation;
    runtime.setAwaitingConfirmation(continuation.kind === "approval");
    runtime.abortContinuation();
    const continuationController = runtime.beginContinuation();
    try {
      const continuationResult = await runtime.runContinuation(continuation, {
        signal: continuationController.signal,
        isCurrent: () =>
          runtime.readEpoch() === dispatchEpoch &&
          runtime.readLatestSession()?.voice_session_id === result.sessionId &&
          runtime.readLatestSession()?.generation === result.generation &&
          runtime.readContextAnchorId() === current.conversational_anchor?.conversation_id,
      });
      narration = continuationResult.narration;
      if (continuation.kind === "conversation") {
        runtime.setConversationRefresh((previous) =>
          nextConversationRefresh(previous, continuation.conversation_id),
        );
      }
    } catch (error) {
      if (!continuationController.signal.aborted) {
        runtime.setNotice(error instanceof Error ? error.message : "语音 continuation 执行失败");
      }
      return;
    } finally {
      runtime.setAwaitingConfirmation(false);
      runtime.endContinuation(continuationController);
    }
  }

  const afterContinuation = runtime.readLatestSession();
  if (
    runtime.readEpoch() !== dispatchEpoch ||
    !afterContinuation ||
    afterContinuation.voice_session_id !== result.sessionId ||
    afterContinuation.generation !== result.generation
  ) {
    return;
  }

  if (narration && current.attention_mode !== "silent") {
    await runtime.speak(narration);
  }
}
