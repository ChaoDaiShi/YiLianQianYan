import { describe, expect, it } from "vitest";
import type { GlobalVoiceSession } from "../../api/voice";
import { runVoiceBargeIn, type BargeInRuntime } from "./bargeIn";

const session: GlobalVoiceSession = {
  voice_session_id: "voice-session-6",
  generation: 4,
  state: "speaking",
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

interface Harness {
  calls: string[];
  runtime: BargeInRuntime;
  resolveInterrupt: (session: GlobalVoiceSession | null) => void;
  /** Simulate a newer dispatch landing while the server interrupt is in flight. */
  advanceEpoch: () => void;
  setLatest: (session: GlobalVoiceSession) => void;
}

function makeHarness(session: GlobalVoiceSession | null): Harness {
  const calls: string[] = [];
  let epoch = 0;
  let latest: GlobalVoiceSession | null = session;
  let resolveInterrupt!: (session: GlobalVoiceSession | null) => void;
  const interrupt = new Promise<GlobalVoiceSession | null>((resolve) => {
    resolveInterrupt = resolve;
  });

  const runtime: BargeInRuntime = {
    bumpEpoch: () => {
      epoch += 1;
      calls.push("bump");
      return epoch;
    },
    readEpoch: () => epoch,
    readLatestSession: () => latest,
    readContextAnchorId: () => "conversation-7",
    abortContinuation: () => calls.push("abort-continuation"),
    interruptPlayback: () => calls.push("interrupt-playback"),
    interruptVoiceSession: (sessionId, generation) => {
      calls.push(`interrupt:${sessionId}:${generation}`);
      return interrupt;
    },
    commitSession: (committed) => {
      latest = committed;
      calls.push(`commit:${committed.voice_session_id}:${committed.generation}`);
    },
  };

  return {
    calls,
    runtime,
    resolveInterrupt,
    advanceEpoch: () => {
      epoch += 1;
    },
    setLatest: (next) => {
      latest = next;
    },
  };
}

describe("runVoiceBargeIn", () => {
  it("commits the new generation after a confirmed server interrupt", async () => {
    const harness = makeHarness(session);
    const interrupted = { ...session, generation: 5 };

    const pending = runVoiceBargeIn(session, harness.runtime);
    expect(harness.calls).toEqual(["bump", "abort-continuation", "interrupt-playback", "interrupt:voice-session-6:4"]);

    harness.resolveInterrupt(interrupted);
    expect(await pending).toEqual(interrupted);
    expect(harness.calls).toEqual([
      "bump",
      "abort-continuation",
      "interrupt-playback",
      "interrupt:voice-session-6:4",
      "commit:voice-session-6:5",
    ]);
  });

  it("bumps the epoch, aborts the continuation and interrupts playback before touching the server", async () => {
    const harness = makeHarness(session);
    void runVoiceBargeIn(session, harness.runtime);
    expect(harness.calls).toEqual(["bump", "abort-continuation", "interrupt-playback", "interrupt:voice-session-6:4"]);
    harness.resolveInterrupt({ ...session, generation: 5 });
  });

  it("fails closed when the server does not confirm the interrupt", async () => {
    const harness = makeHarness(session);
    const pending = runVoiceBargeIn(session, harness.runtime);
    harness.resolveInterrupt(null);
    await expect(pending).rejects.toThrow("语音服务未确认打断，未取得新的输入 generation。");
    expect(harness.calls).not.toContain("commit:voice-session-6:5");
  });

  it("fails closed while the conversation anchor is still syncing", async () => {
    const harness = makeHarness(session);
    // The context anchor the host knows differs from the session's.
    harness.runtime.readContextAnchorId = () => "conversation-other";
    await expect(runVoiceBargeIn(session, harness.runtime)).rejects.toThrow(
      "正在同步对话，请稍后重新说话。",
    );
    expect(harness.calls).toEqual(["bump", "abort-continuation", "interrupt-playback"]);
  });

  it("drops a stale interrupt response when the epoch advanced mid-await", async () => {
    const harness = makeHarness(session);
    const pending = runVoiceBargeIn(session, harness.runtime);
    // A newer dispatch (another barge-in or a new session) lands while the
    // server interrupt is still in flight.
    harness.advanceEpoch();
    harness.setLatest({ ...session, generation: 6 });
    harness.resolveInterrupt({ ...session, generation: 5 });
    await expect(pending).rejects.toThrow("旧语音会话的打断响应已丢弃。");
    expect(harness.calls).not.toContain("commit:voice-session-6:5");
  });

  it("interrupts local playback and returns without a session", async () => {
    const harness = makeHarness(null);
    await expect(runVoiceBargeIn(null, harness.runtime)).resolves.toBeUndefined();
    expect(harness.calls).toEqual(["bump", "abort-continuation", "interrupt-playback"]);
  });
});
