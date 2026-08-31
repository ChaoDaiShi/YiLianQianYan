import { controlSessionHeaders } from "./controlSession";
import { API_BASE } from "./transport";

export type VoiceSessionState =
  | "listening"
  | "speaking"
  | "interrupted"
  | "stopped"
  | "cancelled";

export interface VoiceSession {
  id: string;
  state: VoiceSessionState;
  updated_at: number;
  schema_version: number;
  [key: string]: unknown;
}

export interface PresenceSnapshot {
  activity: "idle" | "working";
  interaction: "none" | "listening" | "speaking" | "interrupted";
  attention: "none" | "requested";
  source: string;
  updated_at: number;
  [key: string]: unknown;
}

async function voiceAction(path: string): Promise<VoiceSession | null> {
  const response = await fetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: controlSessionHeaders(),
  });
  return response.ok ? (response.json() as Promise<VoiceSession>) : null;
}

export async function getPresence(): Promise<PresenceSnapshot | null> {
  const response = await fetch(`${API_BASE}/api/presence`, {
    headers: controlSessionHeaders(),
  });
  return response.ok ? (response.json() as Promise<PresenceSnapshot>) : null;
}

export const startVoiceSession = () => voiceAction("/api/voice/sessions/start");
export const stopVoiceSession = () => voiceAction("/api/voice/sessions/stop");
export const interruptVoiceSession = () => voiceAction("/api/voice/sessions/interrupt");
export const cancelVoiceSession = () => voiceAction("/api/voice/sessions/cancel");

export async function transcribeDeterministicAudio(bytes: Uint8Array) {
  const response = await fetch(`${API_BASE}/api/voice/transcribe`, {
    method: "POST",
    headers: controlSessionHeaders(),
    body: bytes.slice().buffer as ArrayBuffer,
  });
  return response.ok ? response.json() : null;
}

export async function synthesizeDeterministicSpeech(text: string): Promise<ArrayBuffer | null> {
  const response = await fetch(`${API_BASE}/api/voice/speak`, {
    method: "POST",
    headers: { "Content-Type": "application/json", ...controlSessionHeaders() },
    body: JSON.stringify({ text }),
  });
  return response.ok ? response.arrayBuffer() : null;
}
