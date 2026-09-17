import { describe, expect, it, vi } from "vitest";
import type { GlobalVoiceSession, VoiceRuntimeSnapshot } from "../../api/voice";
import { synchronizeVoiceContext } from "./voiceContextSync";

const session: GlobalVoiceSession = { voice_session_id: "voice", generation: 2, state: "listening", focused_surface: "conversation", attention_mode: "balanced", started_at: 1, updated_at: 1, schema_version: 1 };
const context = (id: string) => ({ focused_surface: "conversation" as const, conversational_anchor: { conversation_id: id, title: id, updated_at: 1 }, active_task: null });
const snapshot = (value: GlobalVoiceSession) => ({ session: value } as VoiceRuntimeSnapshot);

describe("voice context coordination", () => {
  it("replays B after A committed remotely but A's acknowledgement was discarded", async () => {
    let epoch = 1;
    let desired = context("a");
    let authoritative = { ...session };
    let finishA!: (value: GlobalVoiceSession) => void;
    const update = vi.fn(async (request) => {
      if (request.generation !== authoritative.generation) return null;
      authoritative = { ...authoritative, ...request, generation: authoritative.generation + 1 };
      if (request.conversational_anchor?.conversation_id === "a") {
        return new Promise<GlobalVoiceSession>((resolve) => { finishA = resolve; });
      }
      return authoritative;
    });
    const reload = vi.fn(async () => snapshot(authoritative));
    const first = synchronizeVoiceContext({ session, getContext: () => desired, isCurrent: () => epoch === 1, signal: new AbortController().signal, update, reload });
    epoch = 2; desired = context("b");
    finishA(authoritative);
    expect(await first).toBeNull();
    const second = await synchronizeVoiceContext({ session, getContext: () => desired, isCurrent: () => epoch === 2, signal: new AbortController().signal, update, reload });
    expect(reload).toHaveBeenCalledOnce();
    expect(second?.conversational_anchor?.conversation_id).toBe("b");
    expect(second?.generation).toBe(4);
    expect(update.mock.calls.map(([request]) => request.conversational_anchor?.conversation_id)).toEqual(["a", "b", "b"]);
  });
  it("recovers null/409 after an earlier committed context response was lost and replays latest context", async () => {
    let desired = context("a");
    const update = vi.fn().mockResolvedValueOnce(null).mockImplementationOnce(async (request) => ({ ...session, ...request, generation: 4 }));
    const reload = vi.fn().mockImplementation(async () => {
      desired = context("b");
      return snapshot({ ...session, generation: 3 });
    });
    const result = await synchronizeVoiceContext({ session, getContext: () => desired, isCurrent: () => true, signal: new AbortController().signal, update, reload });
    expect(reload).toHaveBeenCalledOnce();
    expect(update.mock.calls[1][0]).toMatchObject({ generation: 3, conversational_anchor: { conversation_id: "b" } });
    expect(result?.generation).toBe(4);
    expect(result?.conversational_anchor?.conversation_id).toBe("b");
  });

  it("discards the acknowledgement if identity or epoch changed while awaiting it", async () => {
    let current = true;
    const result = await synchronizeVoiceContext({ session, getContext: () => context("a"), isCurrent: () => current, signal: new AbortController().signal,
      update: vi.fn().mockImplementation(async () => { current = false; return session; }), reload: vi.fn(),
    });
    expect(result).toBeNull();
  });

  it("never adopts a replacement session returned by conflict recovery", async () => {
    const update = vi.fn().mockResolvedValue(null);
    const result = await synchronizeVoiceContext({ session, getContext: () => context("b"), isCurrent: () => true, signal: new AbortController().signal,
      update, reload: vi.fn().mockResolvedValue(snapshot({ ...session, voice_session_id: "other", generation: 8 })),
    });
    expect(result).toBeNull();
    expect(update).toHaveBeenCalledOnce();
  });
});
