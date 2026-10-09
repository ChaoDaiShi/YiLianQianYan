import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("./controlSession", () => ({
  controlSessionHeaders: () => ({ "X-Yilian-Control-Session": "a".repeat(64) }),
}));
import {
  acquireVoiceInputLease,
  dispatchVoiceTurn,
  getPresence,
  normalizeVoiceErrorCode,
  sendPartialTranscript,
  startVoiceSession,
  synthesizeSpeech,
  transcribeAudio,
  updateVoiceContext,
  VoiceApiError,
  voiceErrorMessage,
  type VoiceTurnDispatch,
} from "./voice";
import { listTaskProjections } from "./projections";

describe("voice and projection shared API", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("uses protected provider-neutral voice routes", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce({ ok: true, json: async () => ({ activity: "idle" }) })
      .mockResolvedValueOnce({ ok: true, json: async () => ({ id: "voice-1" }) });
    vi.stubGlobal("fetch", fetchMock);

    await getPresence();
    await startVoiceSession();

    expect(fetchMock.mock.calls[0][0]).toContain("/api/presence");
    expect(fetchMock.mock.calls[1][0]).toContain("/api/voice/sessions/start");
    expect(fetchMock.mock.calls[1][1]?.headers).toMatchObject({
      "X-Yilian-Control-Session": "a".repeat(64),
    });
  });

  it("keeps task data behind projections", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce({ ok: true, json: async () => ({ tasks: [] }) });
    vi.stubGlobal("fetch", fetchMock);

    await listTaskProjections("workspace:test");

    expect(fetchMock.mock.calls[0][0]).toContain("/api/projections/tasks");
  });

  it("sends generation and lease metadata for final-only audio routing", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({ lease_id: "lease-1", generation: 2 }),
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({ partial_transcript: "暂停" }),
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({ transcript: { text: "暂停这个任务" } }),
      });
    vi.stubGlobal("fetch", fetchMock);

    await acquireVoiceInputLease("session-1", 2, "builtin_asr");
    await sendPartialTranscript("session-1", 2, "lease-1", "暂停");
    await dispatchVoiceTurn({
      session_id: "session-1",
      generation: 2,
      lease_id: "lease-1",
      final_transcript: "暂停这个任务",
    } as VoiceTurnDispatch);

    expect(fetchMock.mock.calls[0][1]?.body).toContain("builtin_asr");
    expect(fetchMock.mock.calls[1][1]?.body).toContain("暂停");
    expect(fetchMock.mock.calls[2][1]?.body).toContain("暂停这个任务");
  });

  it("does not send client-side intent or target claims to dispatch", async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ turn: {} }),
    });
    vi.stubGlobal("fetch", fetchMock);

    await dispatchVoiceTurn({
      session_id: "session-1",
      generation: 2,
      lease_id: "lease-1",
      final_transcript: "暂停这个任务",
      focused_surface: "task_canvas",
      resolved_target: { status: "resolved", target: { kind: "task", graph_id: "spoofed" } },
      intent: { kind: "command", name: "task.pause" },
    } as unknown as VoiceTurnDispatch);

    const payload = JSON.parse(fetchMock.mock.calls[0][1]?.body as string) as Record<string, unknown>;
    expect(payload).toEqual({
      session_id: "session-1",
      generation: 2,
      lease_id: "lease-1",
      final_transcript: "暂停这个任务",
    });
    expect(payload.intent).toBeUndefined();
    expect(payload.resolved_target).toBeUndefined();
  });

  it("keeps AbortSignal and generation headers on real audio calls", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({ text: "打开记事本" }),
      })
      .mockResolvedValueOnce({
        ok: true,
        arrayBuffer: async () => new ArrayBuffer(2),
        headers: new Headers({ "content-type": "audio/mpeg" }),
      });
    vi.stubGlobal("fetch", fetchMock);
    const controller = new AbortController();

    await transcribeAudio(
      new Blob(["audio"], { type: "audio/webm" }),
      "session-1",
      2,
      "lease-1",
      controller.signal,
    );
    await synthesizeSpeech(
      { text: "你好", session_id: "session-1", generation: 2 },
      controller.signal,
    );

    expect(fetchMock.mock.calls[0][1]?.signal).toBe(controller.signal);
    expect(fetchMock.mock.calls[0][1]?.headers).toMatchObject({
      "x-yilian-voice-session": "session-1",
      "x-yilian-voice-generation": "2",
      "x-yilian-voice-lease": "lease-1",
      "Content-Type": "audio/webm",
    });
    expect(fetchMock.mock.calls[1][1]?.signal).toBe(controller.signal);
  });

  it("normalizes provider failures to value-free voice error codes", () => {
    expect(normalizeVoiceErrorCode(401, { code: "provider_request_failed" })).toBe("AUTH_FAILED");
    expect(normalizeVoiceErrorCode(429, null)).toBe("RATE_LIMITED");
    expect(normalizeVoiceErrorCode(413, null)).toBe("AUDIO_TOO_LARGE");
    expect(normalizeVoiceErrorCode(415, null)).toBe("UNSUPPORTED_AUDIO");
    expect(normalizeVoiceErrorCode(504, null)).toBe("STT_TIMEOUT");
    expect(normalizeVoiceErrorCode(502, null)).toBe("PROVIDER_UNAVAILABLE");
    expect(normalizeVoiceErrorCode(400, { code: "provider_request_failed" })).toBe(
      "TRANSCRIPTION_FAILED",
    );
  });

  it("throws a sanitized error for a non-successful final transcription", async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: false,
      status: 503,
      json: async () => ({ code: "provider_request_failed", message: "SECRET_VALUE" }),
    });
    vi.stubGlobal("fetch", fetchMock);

    await expect(
      transcribeAudio(new Blob(["audio"], { type: "audio/ogg" }), "session-1", 2, "lease-1"),
    ).rejects.toMatchObject({ code: "PROVIDER_UNAVAILABLE" });

    try {
      await transcribeAudio(new Blob(["audio"], { type: "audio/ogg" }), "session-1", 2, "lease-1");
    } catch (error) {
      expect(error).toBeInstanceOf(VoiceApiError);
      expect((error as Error).message).not.toContain("SECRET_VALUE");
      expect(voiceErrorMessage(error)).toBe("语音识别暂时不可用，请检查网络后重试。");
    }
  });

  it("does not invent an unsupported WebM type for binary audio without metadata", async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ text: "binary audio" }),
    });
    vi.stubGlobal("fetch", fetchMock);

    await transcribeAudio(new Uint8Array([1, 2, 3]), "session-1", 2, "lease-1");

    expect(fetchMock.mock.calls[0][1]?.headers).toMatchObject({
      "Content-Type": "application/octet-stream",
    });
  });

  it("normalizes a malformed successful transcription payload", async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => {
        throw new SyntaxError("provider body is not exposed");
      },
    });
    vi.stubGlobal("fetch", fetchMock);

    await expect(
      transcribeAudio(new Blob(["audio"], { type: "audio/ogg" }), "session-1", 2, "lease-1"),
    ).rejects.toMatchObject({ code: "TRANSCRIPTION_FAILED" });
  });

  it("posts independent context anchors without coupling the route to a session id", async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => ({}) });
    vi.stubGlobal("fetch", fetchMock);

    await updateVoiceContext({
      session_id: "session-1",
      generation: 3,
      focused_surface: "task_canvas",
      conversational_anchor: { conversation_id: "conversation-a", title: "Rust", updated_at: 1 },
      active_task: "task-1",
    });

    expect(fetchMock.mock.calls[0][1]?.body).toContain("conversation-a");
  });
});
