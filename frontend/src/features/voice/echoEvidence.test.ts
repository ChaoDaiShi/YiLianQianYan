import { describe, expect, it } from "vitest";
import { ECHO_EVIDENCE_TTL_MS, readPlaybackOverlap, shouldSuppressCaptureEcho, type CaptureEchoEvidence } from "./echoEvidence";

const result = { text: "继续", sessionId: "voice", generation: 4, leaseId: "lease-a" };
const evidence: CaptureEchoEvidence = { text: "你可以继续查看任务详情", sessionId: "voice", playbackGeneration: 3, generation: 4, leaseId: "lease-a", identityEpoch: 1, observedAt: 100 };

describe("capture-bound echo evidence", () => {
  it("samples only actual current playback, not synthesis, pause, ended or stale audio", () => {
    const audio = { paused: false, ended: false };
    const spoken = { text: "继续", sessionId: "voice", generation: 3 };
    const current = { sessionId: "voice", generation: 3 };
    expect(readPlaybackOverlap(audio, spoken, current, 100)).toMatchObject({ text: "继续", observedAt: 100 });
    expect(readPlaybackOverlap(audio, null, current, 100)).toBeNull();
    expect(readPlaybackOverlap({ ...audio, paused: true }, spoken, current, 100)).toBeNull();
    expect(readPlaybackOverlap({ ...audio, ended: true }, spoken, current, 100)).toBeNull();
    expect(readPlaybackOverlap(audio, spoken, { ...current, generation: 4 }, 100)).toBeNull();
    expect(readPlaybackOverlap(audio, spoken, { ...current, sessionId: "other" }, 100)).toBeNull();
  });
  it("suppresses matching narration only for its overlapping input lease", () => {
    expect(shouldSuppressCaptureEcho(result, evidence, 1, 200)).toBe(true);
    expect(shouldSuppressCaptureEcho({ ...result, leaseId: "later-lease" }, evidence, 1, 200)).toBe(false);
  });
  it("accepts legitimate repeated commands without playback overlap or after expiry", () => {
    expect(shouldSuppressCaptureEcho(result, null, 1, 200)).toBe(false);
    expect(shouldSuppressCaptureEcho(result, evidence, 1, 101 + ECHO_EVIDENCE_TTL_MS)).toBe(false);
  });
  it("cannot carry echo evidence across an anchor, session or generation change", () => {
    expect(shouldSuppressCaptureEcho(result, evidence, 2, 200)).toBe(false);
    expect(shouldSuppressCaptureEcho({ ...result, sessionId: "other" }, evidence, 1, 200)).toBe(false);
    expect(shouldSuppressCaptureEcho({ ...result, generation: 5 }, evidence, 1, 200)).toBe(false);
  });
});
