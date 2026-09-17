import { controlSessionHeaders } from "./controlSession";
import { API_BASE } from "./transport";

export type FocusedSurface =
  | "conversation"
  | "task_canvas"
  | "workspace"
  | "memory"
  | "capability_center"
  | "system";

export type VoiceSessionState =
  | "idle"
  | "listening"
  | "processing"
  | "speaking"
  | "interrupted"
  | "error"
  | "ended"
  | "stopped"
  | "cancelled";

export type VoiceInputOwner =
  | "builtin_asr"
  | "external_asr"
  | "push_to_talk"
  | "future_wake_word";

export interface ConversationalAnchor {
  conversation_id: string;
  title: string;
  updated_at: number;
}

export interface GlobalVoiceSession {
  voice_session_id: string;
  generation: number;
  state: VoiceSessionState;
  input_owner?: VoiceInputOwner | null;
  focused_surface: FocusedSurface;
  conversational_anchor?: ConversationalAnchor | null;
  active_task?: string | null;
  attention_mode: "silent" | "balanced" | "companion";
  started_at: number;
  updated_at: number;
  schema_version: number;
  [key: string]: unknown;
}

/** Compatibility shape retained for existing callers. */
export type VoiceSession = GlobalVoiceSession;

export interface VoiceInputLease {
  lease_id: string;
  voice_session_id: string;
  generation: number;
  owner: VoiceInputOwner;
  acquired_at: number;
  [key: string]: unknown;
}

export interface VoiceRuntimeSnapshot {
  session: GlobalVoiceSession | null;
  lease?: VoiceInputLease | null;
  partial_transcript?: string | null;
  final_transcript?: string | null;
  turns: VoiceTurn[];
  presence: PresenceSnapshot;
}

export interface VoiceTurn {
  turn_id: string;
  voice_session_id: string;
  generation: number;
  lease_id: string;
  source: "text" | "voice" | "global_command";
  final_transcript: string;
  focused_surface: FocusedSurface;
  anchor_snapshot: {
    focused_surface: FocusedSurface;
    conversational_anchor?: ConversationalAnchor | null;
    active_task?: string | null;
  };
  resolved_target: TargetResolution;
  intent: InteractionIntent;
  created_at: number;
  schema_version: number;
}

export type TargetResolution =
  | { status: "resolved"; target: Record<string, unknown> }
  | { status: "ambiguous"; candidates: Record<string, unknown>[] }
  | { status: "missing"; reason: string };

export type InteractionIntent =
  | { kind: "conversation_turn" }
  | { kind: "query"; name: string }
  | { kind: "command"; name: string }
  | { kind: "graph_mutation_proposal" };

export interface PresenceSnapshot {
  activity: "idle" | "working";
  interaction: "none" | "listening" | "speaking" | "interrupted";
  attention: "none" | "requested";
  source: string;
  updated_at: number;
  [key: string]: unknown;
}

export interface VoiceContextUpdate {
  session_id: string;
  generation: number;
  focused_surface: FocusedSurface;
  conversational_anchor?: ConversationalAnchor | null;
  active_task?: string | null;
}

export interface VoiceApprovalAttestation {
  attestation_id: string;
  approval_id: string;
  conversation_id: string;
  voice_session_id: string;
  generation: number;
  displayed_at: number;
  expires_at: number;
}

export interface VoiceTurnDispatch {
  session_id: string;
  generation: number;
  lease_id: string;
  final_transcript: string;
}

export interface VoiceCommandResult {
  request_id: string;
  status: "succeeded" | "failed" | "not_found";
  result?: Record<string, unknown> | null;
  error?: { code: string; message: string } | null;
  schema_version: number;
}

export interface VoiceDispatchResult {
  turn: VoiceTurn;
  command_result?: VoiceCommandResult | null;
  narration?: string | null;
  continuation?: VoiceContinuation | null;
}

export type VoiceContinuation =
  | {
      kind: "conversation";
      conversation_id: string;
      message: string;
    }
  | {
      kind: "approval";
      approval_id: string;
      conversation_id: string;
      attestation_id: string;
      decision: "approve" | "reject";
    };

export interface SpeechRequest {
  text: string;
  session_id: string;
  generation: number;
  voice?: string;
  language?: string;
}

export interface SpeechPlayback {
  audio: ArrayBuffer;
  mediaType: string;
  provider?: string;
  generation?: number;
}

export type VoiceErrorCode =
  | "PROVIDER_UNAVAILABLE"
  | "AUTH_FAILED"
  | "RATE_LIMITED"
  | "AUDIO_TOO_LARGE"
  | "UNSUPPORTED_AUDIO"
  | "TRANSCRIPTION_FAILED"
  | "STT_TIMEOUT";

/** A value-free error for user-visible voice failures. Provider bodies never cross this boundary. */
export class VoiceApiError extends Error {
  readonly code: VoiceErrorCode;
  readonly status?: number;

  constructor(code: VoiceErrorCode, status?: number) {
    super(code);
    this.name = "VoiceApiError";
    this.code = code;
    this.status = status;
  }
}

function responseErrorCode(payload: unknown): string {
  if (typeof payload !== "object" || payload === null) return "";
  const code = (payload as Record<string, unknown>).code;
  return typeof code === "string" ? code.trim().toLowerCase() : "";
}

export function normalizeVoiceErrorCode(status: number, payload: unknown): VoiceErrorCode {
  if (status === 401 || status === 403) return "AUTH_FAILED";
  if (status === 408 || status === 504) return "STT_TIMEOUT";
  if (status === 413) return "AUDIO_TOO_LARGE";
  if (status === 415) return "UNSUPPORTED_AUDIO";
  if (status === 429) return "RATE_LIMITED";
  if (status >= 500) return "PROVIDER_UNAVAILABLE";

  switch (responseErrorCode(payload)) {
    case "provider_unavailable":
      return "PROVIDER_UNAVAILABLE";
    case "provider_timeout":
    case "stt_timeout":
      return "STT_TIMEOUT";
    case "rate_limited":
    case "rate_limit":
      return "RATE_LIMITED";
    case "auth_failed":
      return "AUTH_FAILED";
    case "audio_too_large":
      return "AUDIO_TOO_LARGE";
    case "unsupported_audio":
    case "invalid_audio":
    case "invalid_audio_response":
      return "UNSUPPORTED_AUDIO";
    default:
      return "TRANSCRIPTION_FAILED";
  }
}

export function voiceErrorMessage(error: unknown): string {
  const code = error instanceof VoiceApiError ? error.code : "PROVIDER_UNAVAILABLE";
  switch (code) {
    case "AUTH_FAILED":
      return "语音识别认证失败，请检查语音服务配置。";
    case "RATE_LIMITED":
      return "语音识别请求过于频繁，请稍后重试。";
    case "AUDIO_TOO_LARGE":
      return "语音片段过长，请缩短后重试。";
    case "UNSUPPORTED_AUDIO":
      return "当前录音格式暂不受语音服务支持。";
    case "STT_TIMEOUT":
      return "语音识别请求超时，请重试。";
    case "TRANSCRIPTION_FAILED":
      return "语音识别失败，请重试。";
    case "PROVIDER_UNAVAILABLE":
    default:
      return "语音识别暂时不可用，请检查网络后重试。";
  }
}

async function voiceAction(
  path: string,
  body?: unknown,
  signal?: AbortSignal,
): Promise<VoiceSession | null> {
  const response = await fetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: body === undefined
      ? controlSessionHeaders()
      : { "Content-Type": "application/json", ...controlSessionHeaders() },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal,
  });
  return response.ok ? (response.json() as Promise<VoiceSession>) : null;
}

async function jsonPost<T>(path: string, body: unknown, signal?: AbortSignal): Promise<T | null> {
  const response = await fetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json", ...controlSessionHeaders() },
    body: JSON.stringify(body),
    signal,
  });
  return response.ok ? (response.json() as Promise<T>) : null;
}

export async function getPresence(signal?: AbortSignal): Promise<PresenceSnapshot | null> {
  const response = await fetch(`${API_BASE}/api/presence`, {
    headers: controlSessionHeaders(),
    signal,
  });
  return response.ok ? (response.json() as Promise<PresenceSnapshot>) : null;
}

export async function getVoiceSession(signal?: AbortSignal): Promise<VoiceRuntimeSnapshot | null> {
  const response = await fetch(`${API_BASE}/api/voice/session`, {
    headers: controlSessionHeaders(),
    signal,
  });
  return response.ok ? (response.json() as Promise<VoiceRuntimeSnapshot>) : null;
}

export const startVoiceSession = (
  focusedSurface: FocusedSurface = "conversation",
  signal?: AbortSignal,
) => voiceAction("/api/voice/sessions/start", { focused_surface: focusedSurface }, signal);

export const reinitializeVoiceInput = (
  sessionId: string,
  generation: number,
  signal?: AbortSignal,
) => jsonPost<GlobalVoiceSession>("/api/voice/sessions/reinitialize", {
  session_id: sessionId,
  generation,
}, signal);

export const stopVoiceSession = (signal?: AbortSignal) => voiceAction("/api/voice/sessions/stop", undefined, signal);
export const cancelVoiceSession = (signal?: AbortSignal) => voiceAction("/api/voice/sessions/cancel", undefined, signal);

export async function endVoiceSession(
  sessionId: string,
  generation: number,
  signal?: AbortSignal,
): Promise<GlobalVoiceSession | null> {
  return voiceAction("/api/voice/sessions/stop", { session_id: sessionId, generation }, signal);
}

export const interruptVoiceSession = (
  sessionId?: string,
  generation?: number,
  signal?: AbortSignal,
) => voiceAction(
  "/api/voice/sessions/interrupt",
  sessionId === undefined || generation === undefined
    ? undefined
    : { session_id: sessionId, generation },
  signal,
);

export async function acquireVoiceInputLease(
  sessionId: string,
  generation: number,
  owner: VoiceInputOwner = "builtin_asr",
  signal?: AbortSignal,
): Promise<VoiceInputLease | null> {
  return jsonPost<VoiceInputLease>("/api/voice/leases", {
    session_id: sessionId,
    generation,
    owner,
  }, signal);
}

export async function updateVoiceContext(
  context: VoiceContextUpdate,
  signal?: AbortSignal,
): Promise<GlobalVoiceSession | null> {
  return jsonPost<GlobalVoiceSession>("/api/voice/context", context, signal);
}

export async function attestVoiceApprovalDisplayed(
  approvalId: string,
  sessionId: string,
  generation: number,
  displayId: string,
  signal?: AbortSignal,
): Promise<VoiceApprovalAttestation | null> {
  const response = await fetch(
    `${API_BASE}/api/voice/approvals/${encodeURIComponent(approvalId)}/displayed`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json", ...controlSessionHeaders() },
      body: JSON.stringify({
        voice_session_id: sessionId,
        generation,
        display_id: displayId,
      }),
      signal,
    },
  );
  return response.ok ? (response.json() as Promise<VoiceApprovalAttestation>) : null;
}

export async function revokeVoiceApprovalDisplay(
  approvalId: string,
  attestationId: string,
  displayId: string,
): Promise<void> {
  await fetch(`${API_BASE}/api/voice/approvals/${encodeURIComponent(approvalId)}/displayed`, {
    method: "DELETE",
    headers: { "Content-Type": "application/json", ...controlSessionHeaders() },
    body: JSON.stringify({ attestation_id: attestationId, display_id: displayId }),
  });
}

export async function sendPartialTranscript(
  sessionId: string,
  generation: number,
  leaseId: string,
  text: string,
  signal?: AbortSignal,
): Promise<VoiceRuntimeSnapshot | null> {
  return jsonPost<VoiceRuntimeSnapshot>("/api/voice/transcript/partial", {
    session_id: sessionId,
    generation,
    lease_id: leaseId,
    text,
  }, signal);
}

export async function dispatchVoiceTurn(
  turn: VoiceTurnDispatch,
  signal?: AbortSignal,
): Promise<VoiceDispatchResult | null> {
  return jsonPost<VoiceDispatchResult>("/api/voice/turns/dispatch", {
    session_id: turn.session_id,
    generation: turn.generation,
    lease_id: turn.lease_id,
    final_transcript: turn.final_transcript,
  }, signal);
}

export async function transcribeAudio(
  audio: Blob | ArrayBuffer | Uint8Array,
  sessionId: string,
  generation: number,
  leaseId: string,
  signal?: AbortSignal,
): Promise<unknown | null> {
  const mediaType = audio instanceof Blob && audio.type ? audio.type : "application/octet-stream";
  const body = audio instanceof Uint8Array ? audio.buffer : audio;
  const response = await fetch(`${API_BASE}/api/voice/transcribe`, {
    method: "POST",
    headers: {
      "Content-Type": mediaType,
      "x-yilian-voice-session": sessionId,
      "x-yilian-voice-generation": String(generation),
      "x-yilian-voice-lease": leaseId,
      ...controlSessionHeaders(),
    },
    body: body as BodyInit,
    signal,
  });
  if (!response.ok) {
    let payload: unknown = null;
    try {
      payload = await response.json();
    } catch {
      // The status alone is enough for a safe normalized error.
    }
    throw new VoiceApiError(normalizeVoiceErrorCode(response.status, payload), response.status);
  }
  try {
    return await response.json();
  } catch {
    throw new VoiceApiError("TRANSCRIPTION_FAILED", response.status);
  }
}

export async function transcribePartialAudio(
  audio: Blob | ArrayBuffer | Uint8Array,
  sessionId: string,
  generation: number,
  leaseId: string,
  signal?: AbortSignal,
): Promise<unknown | null> {
  const mediaType = audio instanceof Blob && audio.type ? audio.type : "application/octet-stream";
  const body = audio instanceof Uint8Array ? audio.buffer : audio;
  const response = await fetch(`${API_BASE}/api/voice/transcribe/partial`, {
    method: "POST",
    headers: {
      "Content-Type": mediaType,
      "x-yilian-voice-session": sessionId,
      "x-yilian-voice-generation": String(generation),
      "x-yilian-voice-lease": leaseId,
      ...controlSessionHeaders(),
    },
    body: body as BodyInit,
    signal,
  });
  return response.ok ? response.json() : null;
}

export async function synthesizeSpeech(
  request: SpeechRequest,
  signal?: AbortSignal,
): Promise<SpeechPlayback | null> {
  const response = await fetch(`${API_BASE}/api/voice/speak`, {
    method: "POST",
    headers: { "Content-Type": "application/json", ...controlSessionHeaders() },
    body: JSON.stringify(request),
    signal,
  });
  if (!response.ok) return null;
  return {
    audio: await response.arrayBuffer(),
    mediaType: response.headers.get("content-type") ?? "audio/mpeg",
    provider: response.headers.get("x-yilian-voice-provider") ?? undefined,
    generation: Number(response.headers.get("x-yilian-voice-generation") ?? request.generation ?? 0) || undefined,
  };
}

export const interruptVoiceSpeech = (
  sessionId: string,
  generation: number,
  signal?: AbortSignal,
) => interruptVoiceSession(sessionId, generation, signal);

/** @deprecated deterministic audio is no longer a production capability. */
export async function transcribeDeterministicAudio(bytes: Uint8Array) {
  return transcribeAudio(bytes, "", 0, "");
}

/** @deprecated deterministic audio is no longer a production capability. */
export async function synthesizeDeterministicSpeech(text: string): Promise<ArrayBuffer | null> {
  void text;
  return null;
}

export const markVoiceSpeechStarted = (
  sessionId: string,
  generation: number,
  signal?: AbortSignal,
) =>
  jsonPost<GlobalVoiceSession>(
    "/api/voice/speech/start",
    { session_id: sessionId, generation },
    signal,
  );

export const markVoiceSpeechFinished = (
  sessionId: string,
  generation: number,
  signal?: AbortSignal,
) =>
  jsonPost<GlobalVoiceSession>(
    "/api/voice/speech/finished",
    { session_id: sessionId, generation },
    signal,
  );
