import { useCallback, useMemo, useRef } from "react";
import type { GlobalVoiceSession } from "../../../api/voice";
import type { CaptureEchoEvidence, PlaybackOverlap } from "../echoEvidence";
import type { GlobalVoiceContextSnapshot } from "../model/types";

/**
 * The global voice runtime — the mutable coordination state that must not be
 * spread across the host component.
 *
 * Everything here is a *guard against a stale result*. Three counters decide
 * whether an asynchronous answer is still allowed to land:
 *
 *   - `dispatchEpoch` — bumped by any barge-in, session start, session end,
 *     context-identity change, or unmount. A turn, continuation or narration
 *     that resolves after a bump is dropped.
 *   - `captureEpoch` — bumped by every capture operation. A capture start that
 *     resolves after a newer one began must not claim the microphone.
 *   - `identityEpoch` — bumped when the conversational anchor changes. Echo
 *     evidence is stamped with it, so evidence from one conversation can never
 *     suppress a transcript in another.
 *
 * The four rules this hook exists to keep in one place:
 *
 *   1. **Epochs only ever move forward.** Every bump is `+= 1` on a ref, so a
 *      reader that captured a value can compare for equality later.
 *   2. **Abort before await.** A continuation is aborted before a new one
 *      begins and before any operation that invalidates it.
 *   3. **Echo evidence is claimed once.** `takeEchoEvidence` clears as it
 *      reads, so one playback overlap cannot suppress two transcripts.
 *   4. **The capture operation and identity epochs are read together.** A
 *      capture start records both up front and re-checks both afterwards;
 *      splitting them would let a same-session capture survive a context
 *      change.
 *
 * None of this can be reached by a static-markup render. It is pinned instead
 * by the `bargeIn`, `turnFlow`, `voiceContextSync`, `echoEvidence` and
 * `GlobalVoiceHost` suites.
 */
export interface GlobalVoiceRuntime {
  /** The dispatch epoch as of this call. */
  readEpoch(): number;
  /** Invalidate every in-flight turn, continuation and narration. */
  bumpEpoch(): number;

  /** Begin a capture operation; returns the operation and identity epochs. */
  beginCaptureOperation(): { operationEpoch: number; identityEpoch: number };
  /**
   * Whether a capture start that recorded `operationEpoch`/`identityEpoch` is
   * still the current one. Both must still match.
   */
  isCurrentCaptureOperation(operationEpoch: number, identityEpoch: number): boolean;

  readIdentityEpoch(): number;
  /** Move to a new conversational anchor: bump identity, invalidate in flight. */
  bumpIdentityEpoch(): void;

  readLatestSession(): GlobalVoiceSession | null;
  commitSession(session: GlobalVoiceSession | null): void;

  readContext(): GlobalVoiceContextSnapshot;
  commitContext(context: GlobalVoiceContextSnapshot): void;

  /** Drop any echo evidence (a new capture, session or identity). */
  clearEchoEvidence(): void;
  /**
   * Record playback overlap as echo evidence, but only when the capture start
   * that observed it is still current and the overlap belongs to the same
   * session one generation behind the new lease.
   */
  recordEchoEvidence(
    overlap: PlaybackOverlap | null,
    lease: { sessionId: string; generation: number; leaseId: string } | null | undefined,
    operationEpoch: number,
    identityEpoch: number,
  ): void;
  /** Read and clear the echo evidence — one overlap suppresses one transcript. */
  takeEchoEvidence(): CaptureEchoEvidence | null;

  abortContinuation(): void;
  beginContinuation(): AbortController;
  endContinuation(controller: AbortController): void;

  /** Invalidate everything in flight: the unmount and context-change path. */
  invalidateInFlight(): void;
}

export function useGlobalVoiceRuntime(
  initialSession: GlobalVoiceSession | null = null,
): GlobalVoiceRuntime {
  const dispatchEpochRef = useRef(0);
  const captureEpochRef = useRef(0);
  const identityEpochRef = useRef(0);
  const echoEvidenceRef = useRef<CaptureEchoEvidence | null>(null);
  const continuationRef = useRef<AbortController | null>(null);
  const latestSessionRef = useRef<GlobalVoiceSession | null>(initialSession);
  const latestContextRef = useRef<GlobalVoiceContextSnapshot | null>(null);

  const readEpoch = useCallback(() => dispatchEpochRef.current, []);
  const bumpEpoch = useCallback(() => ++dispatchEpochRef.current, []);

  const abortContinuation = useCallback(() => {
    continuationRef.current?.abort();
    continuationRef.current = null;
  }, []);

  const bumpIdentityEpoch = useCallback(() => {
    identityEpochRef.current += 1;
    captureEpochRef.current += 1;
    echoEvidenceRef.current = null;
    dispatchEpochRef.current += 1;
    abortContinuation();
  }, [abortContinuation]);

  const invalidateInFlight = useCallback(() => {
    dispatchEpochRef.current += 1;
    captureEpochRef.current += 1;
    echoEvidenceRef.current = null;
    abortContinuation();
  }, [abortContinuation]);

  const beginCaptureOperation = useCallback(() => {
    captureEpochRef.current += 1;
    echoEvidenceRef.current = null;
    return {
      operationEpoch: captureEpochRef.current,
      identityEpoch: identityEpochRef.current,
    };
  }, []);

  const isCurrentCaptureOperation = useCallback(
    (operationEpoch: number, identityEpoch: number) =>
      operationEpoch === captureEpochRef.current
      && identityEpoch === identityEpochRef.current,
    [],
  );

  const recordEchoEvidence = useCallback(
    (
      overlap: PlaybackOverlap | null,
      lease: { sessionId: string; generation: number; leaseId: string } | null | undefined,
      operationEpoch: number,
      identityEpoch: number,
    ) => {
      if (
        !overlap
        || !lease
        || !isCurrentCaptureOperation(operationEpoch, identityEpoch)
        || overlap.sessionId !== lease.sessionId
        || overlap.playbackGeneration + 1 !== lease.generation
      ) {
        return;
      }
      echoEvidenceRef.current = { ...overlap, ...lease, identityEpoch };
    },
    [isCurrentCaptureOperation],
  );

  const takeEchoEvidence = useCallback(() => {
    const evidence = echoEvidenceRef.current;
    echoEvidenceRef.current = null;
    return evidence;
  }, []);

  const clearEchoEvidence = useCallback(() => {
    echoEvidenceRef.current = null;
  }, []);

  const beginContinuation = useCallback(() => {
    const controller = new AbortController();
    continuationRef.current = controller;
    return controller;
  }, []);

  const endContinuation = useCallback((controller: AbortController) => {
    if (continuationRef.current === controller) {
      continuationRef.current = null;
    }
  }, []);

  const readIdentityEpoch = useCallback(() => identityEpochRef.current, []);
  const readLatestSession = useCallback(() => latestSessionRef.current, []);
  const commitSession = useCallback((session: GlobalVoiceSession | null) => {
    latestSessionRef.current = session;
  }, []);
  const readContext = useCallback(
    () => latestContextRef.current as GlobalVoiceContextSnapshot,
    [],
  );
  const commitContext = useCallback((context: GlobalVoiceContextSnapshot) => {
    latestContextRef.current = context;
  }, []);

  // Stable identity: the host puts this object in memo and effect dependency
  // arrays, so a new object every render would re-run flows that must run once.
  return useMemo<GlobalVoiceRuntime>(
    () => ({
      readEpoch,
      bumpEpoch,
      beginCaptureOperation,
      isCurrentCaptureOperation,
      readIdentityEpoch,
      bumpIdentityEpoch,
      readLatestSession,
      commitSession,
      readContext,
      commitContext,
      clearEchoEvidence,
      recordEchoEvidence,
      takeEchoEvidence,
      abortContinuation,
      beginContinuation,
      endContinuation,
      invalidateInFlight,
    }),
    [
      abortContinuation,
      beginCaptureOperation,
      beginContinuation,
      bumpEpoch,
      bumpIdentityEpoch,
      clearEchoEvidence,
      commitContext,
      commitSession,
      endContinuation,
      invalidateInFlight,
      isCurrentCaptureOperation,
      readContext,
      readEpoch,
      readIdentityEpoch,
      readLatestSession,
      recordEchoEvidence,
      takeEchoEvidence,
    ],
  );
}
