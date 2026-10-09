import type { GlobalVoiceSession } from "../../api/voice";

/**
 * The barge-in flow, extracted from `GlobalVoiceHost` so its concurrency can be
 * tested without a browser. It is a pure function of its arguments plus the
 * injected runtime: nothing here touches React or the network.
 *
 * The order is load-bearing and mirrors the host's original `interruptForBargeIn`:
 *
 *   1. bump the dispatch epoch — every in-flight turn and continuation is now stale;
 *   2. abort any running continuation;
 *   3. interrupt local audio playback;
 *   4. only then ask the server to interrupt;
 *   5. re-check the epoch and session identity before committing the new generation.
 *
 * Step 5 is the concurrency guard: a newer dispatch (another barge-in, a new
 * session) invalidates the server's response, which must be dropped rather than
 * committed as if it were the current generation.
 */
export interface BargeInRuntime {
  bumpEpoch(): number;
  readEpoch(): number;
  readLatestSession(): GlobalVoiceSession | null;
  readContextAnchorId(): string | undefined;
  abortContinuation(): void;
  interruptPlayback(): void;
  interruptVoiceSession(sessionId: string, generation: number): Promise<GlobalVoiceSession | null>;
  commitSession(session: GlobalVoiceSession): void;
}

export async function runVoiceBargeIn(
  session: GlobalVoiceSession | null,
  runtime: BargeInRuntime,
): Promise<GlobalVoiceSession | void> {
  const requestEpoch = runtime.bumpEpoch();
  runtime.abortContinuation();
  runtime.interruptPlayback();
  if (!session) return;

  if (session.conversational_anchor?.conversation_id !== runtime.readContextAnchorId()) {
    throw new Error("正在同步对话，请稍后重新说话。");
  }

  const interrupted = await runtime.interruptVoiceSession(
    session.voice_session_id,
    session.generation,
  );
  if (!interrupted) {
    throw new Error("语音服务未确认打断，未取得新的输入 generation。");
  }

  const latest = runtime.readLatestSession();
  if (
    runtime.readEpoch() !== requestEpoch ||
    latest?.voice_session_id !== session.voice_session_id ||
    latest?.generation !== session.generation
  ) {
    throw new Error("旧语音会话的打断响应已丢弃。");
  }

  runtime.commitSession(interrupted);
  return interrupted;
}
