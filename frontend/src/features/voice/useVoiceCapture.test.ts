import { describe, expect, it } from "vitest";
import type { GlobalVoiceSession, VoiceInputLease } from "../../api/voice";
import {
  appendCaptureChunk,
  CAPTURE_CHUNK_INTERVAL_MS,
  MAX_CAPTURE_BYTES,
  MAX_CAPTURE_DURATION_MS,
  isCaptureStartAllowed,
  isCurrentCaptureOperation,
  isExplicitMicrophoneStart,
  selectRecorderMimeType,
  type CaptureBuffer,
} from "./useVoiceCapture";

const session: GlobalVoiceSession = {
  voice_session_id: "voice-1",
  generation: 3,
  state: "listening",
  focused_surface: "conversation",
  attention_mode: "balanced",
  started_at: 1,
  updated_at: 1,
  schema_version: 1,
};

const lease: VoiceInputLease = {
  lease_id: "lease-1",
  voice_session_id: session.voice_session_id,
  generation: session.generation,
  owner: "builtin_asr",
  acquired_at: 1,
};

describe("bounded final-first voice capture", () => {
  it("allows automatic speech start only under explicit hands-free opt-in", () => {
    expect(isExplicitMicrophoneStart("hands-free", false)).toBe(false);
    expect(isExplicitMicrophoneStart("hands-free", true)).toBe(true);
    expect(isExplicitMicrophoneStart("mount", true)).toBe(false);
  });
  it("chooses only a supported official candidate and fails closed when none is supported", () => {
    expect(selectRecorderMimeType((mimeType) => mimeType === "audio/ogg;codecs=opus")).toBe(
      "audio/ogg;codecs=opus",
    );
    expect(selectRecorderMimeType(() => false)).toBeNull();
  });

  it("keeps the in-memory audio buffer below the byte ceiling", () => {
    const buffer: CaptureBuffer = { chunks: [], bytes: 0 };
    expect(appendCaptureChunk(buffer, new Blob([new Uint8Array(4)]), 8)).toBe(true);
    expect(appendCaptureChunk(buffer, new Blob([new Uint8Array(5)]), 8)).toBe(false);
    expect(buffer.bytes).toBe(4);
    expect(buffer.chunks).toHaveLength(1);
    expect(MAX_CAPTURE_BYTES).toBeGreaterThan(0);
    expect(MAX_CAPTURE_DURATION_MS).toBeGreaterThan(0);
    expect(CAPTURE_CHUNK_INTERVAL_MS).toBeGreaterThan(0);
  });

  it("blocks a second capture while acquiring, listening, transcribing, or finalizing", () => {
    expect(isCaptureStartAllowed("idle", false, false)).toBe(true);
    expect(isCaptureStartAllowed("acquiring", false, false)).toBe(false);
    expect(isCaptureStartAllowed("listening", false, false)).toBe(false);
    expect(isCaptureStartAllowed("transcribing", false, false)).toBe(false);
    expect(isCaptureStartAllowed("idle", false, true)).toBe(false);
  });

  it("does not acquire twice while the first start is awaiting its lease", () => {
    let acquireInFlight = false;
    let acquireCalls = 0;
    const startInSameRender = () => {
      if (!isCaptureStartAllowed("idle", false, false, acquireInFlight)) return;
      acquireInFlight = true;
      acquireCalls += 1;
    };

    startInSameRender();
    startInSameRender();

    expect(acquireCalls).toBe(1);
  });

  it("accepts only the current operation, lease, session, and generation", () => {
    expect(isCurrentCaptureOperation(7, 7, lease, lease, session, false)).toBe(true);
    expect(isCurrentCaptureOperation(6, 7, lease, lease, session, false)).toBe(false);
    expect(isCurrentCaptureOperation(7, 7, lease, null, session, false)).toBe(false);
    expect(
      isCurrentCaptureOperation(7, 7, lease, lease, { ...session, generation: 4 }, false),
    ).toBe(false);
    expect(isCurrentCaptureOperation(7, 7, lease, lease, session, true)).toBe(false);
  });
});
