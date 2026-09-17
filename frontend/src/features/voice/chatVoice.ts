export type ChatVoiceTranscriptPhase = "partial" | "final";

/** A chat draft belongs to the user until they explicitly press send. */
export function appendFinalTranscriptToComposer(draft: string, transcript: string): string {
  const next = transcript.trim();
  if (!next) return draft;
  return draft.trim() ? `${draft}\n${next}` : next;
}

export function shouldInsertFinalTranscript(
  phase: ChatVoiceTranscriptPhase,
  cancelled: boolean,
): boolean {
  return phase === "final" && !cancelled;
}

/** Convert an analyser RMS sample to a deliberately bounded visual-only meter. */
export function voiceMeterPercent(rms: number): number {
  if (!Number.isFinite(rms) || rms <= 0) return 0;
  return Math.min(100, Math.round(Math.sqrt(rms) * 100));
}
