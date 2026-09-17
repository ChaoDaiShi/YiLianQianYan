import { isPlaybackEcho } from "./speechActivity";
import type { FinalVoiceTranscript } from "./useVoiceCapture";

export interface PlaybackOverlap {
  text: string;
  sessionId: string;
  playbackGeneration: number;
  observedAt: number;
}

export interface CaptureEchoEvidence extends PlaybackOverlap {
  generation: number;
  leaseId: string;
  identityEpoch: number;
}

export const ECHO_EVIDENCE_TTL_MS = 30_000;

export function readPlaybackOverlap(
  audio: Pick<HTMLAudioElement, "paused" | "ended"> | null,
  spoken: { text: string; sessionId: string; generation: number } | null,
  current: { sessionId: string | null; generation: number | null },
  now: number,
): PlaybackOverlap | null {
  if (!audio || audio.paused || audio.ended || !spoken
    || spoken.sessionId !== current.sessionId || spoken.generation !== current.generation) return null;
  return { text: spoken.text, sessionId: spoken.sessionId, playbackGeneration: spoken.generation, observedAt: now };
}

export function shouldSuppressCaptureEcho(result: FinalVoiceTranscript, evidence: CaptureEchoEvidence | null, identityEpoch: number, now: number): boolean {
  return evidence !== null && evidence.sessionId === result.sessionId
    && evidence.generation === result.generation && evidence.leaseId === result.leaseId
    && evidence.identityEpoch === identityEpoch && now >= evidence.observedAt
    && now - evidence.observedAt <= ECHO_EVIDENCE_TTL_MS
    && isPlaybackEcho(result.text, evidence.text);
}
