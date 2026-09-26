/**
 * Pure voice projections: everything here is a function of its arguments.
 *
 * Nothing in this file touches React, the network or the session lifecycle, so
 * the labelling and comparison rules can be reasoned about — and tested —
 * without rendering the host.
 */

import type {
  GlobalVoiceSession,
  PresenceSnapshot,
  VoiceRuntimeSnapshot,
  VoiceSessionState,
} from "../../../api/voice";
import type { FinalVoiceTranscript } from "../useVoiceCapture";
import type {
  ConversationRefreshSignal,
  GlobalVoiceContextSnapshot,
} from "./types";

export const EMPTY_PRESENCE: PresenceSnapshot = {
  activity: "idle",
  interaction: "none",
  attention: "none",
  source: "global-voice-runtime",
  updated_at: 0,
};

export function getVoiceSessionKey(snapshot: VoiceRuntimeSnapshot | null): string {
  const session = snapshot?.session;
  return session ? `${session.voice_session_id}:${session.generation}` : "voice:none";
}

export function isFinalTranscriptForSession(
  result: FinalVoiceTranscript,
  session: GlobalVoiceSession | null,
): boolean {
  return Boolean(
    session &&
      session.voice_session_id === result.sessionId &&
      session.generation === result.generation,
  );
}

export function voiceStatusLabel(
  status: VoiceSessionState | "acquiring" | "transcribing" | "playing" | "paused" | "error" | "idle",
): string {
  switch (status) {
    case "listening":
      return "正在听取";
    case "processing":
    case "transcribing":
      return "正在处理";
    case "speaking":
    case "playing":
      return "正在播报";
    case "acquiring":
      return "正在准备麦克风";
    case "paused":
      return "播报已暂停";
    case "interrupted":
      return "已打断";
    case "error":
      return "语音错误";
    case "ended":
      return "会话已结束";
    case "stopped":
      return "已停止";
    case "cancelled":
      return "已取消";
    case "idle":
    default:
      return "未启动";
  }
}

/** Some responses arrive wrapped; unwrap once so callers see one shape. */
export function normalizeSnapshot(value: VoiceRuntimeSnapshot | null): VoiceRuntimeSnapshot | null {
  if (!value) return null;
  const candidate = value as unknown as { snapshot?: VoiceRuntimeSnapshot };
  return candidate.snapshot ?? value;
}

export function contextFromSession(session: GlobalVoiceSession | null): GlobalVoiceContextSnapshot {
  return {
    focused_surface: session?.focused_surface ?? "conversation",
    conversational_anchor: session?.conversational_anchor ?? null,
    active_task: session?.active_task ?? null,
  };
}

export function contextMatchesSession(
  context: GlobalVoiceContextSnapshot,
  session: GlobalVoiceSession,
): boolean {
  return (
    context.focused_surface === session.focused_surface &&
    context.active_task === (session.active_task ?? null) &&
    context.conversational_anchor?.conversation_id === session.conversational_anchor?.conversation_id &&
    context.conversational_anchor?.title === session.conversational_anchor?.title &&
    context.conversational_anchor?.updated_at === session.conversational_anchor?.updated_at
  );
}

export function snapshotWithSession(
  previous: VoiceRuntimeSnapshot | null,
  session: GlobalVoiceSession,
): VoiceRuntimeSnapshot {
  return {
    session,
    lease: null,
    partial_transcript: null,
    final_transcript: previous?.final_transcript ?? null,
    turns: previous?.turns ?? [],
    presence: previous?.presence ?? EMPTY_PRESENCE,
  };
}

export function transcriptFromTurn(snapshot: VoiceRuntimeSnapshot | null): string {
  const lastTurn = snapshot?.turns[snapshot.turns.length - 1];
  return snapshot?.final_transcript ?? lastTurn?.final_transcript ?? "";
}

/**
 * Bump the refresh revision for the same conversation, and start over for a
 * different one, so a subscriber can tell "new turn" from "new conversation".
 */
export function nextConversationRefresh(
  current: ConversationRefreshSignal | null,
  conversationId: string,
): ConversationRefreshSignal {
  const conversation_id = conversationId.trim();
  if (!conversation_id) throw new Error("conversation identity must not be empty");
  return {
    conversation_id,
    revision: current?.conversation_id === conversation_id ? current.revision + 1 : 1,
  };
}
