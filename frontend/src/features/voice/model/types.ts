import type { ReactNode } from "react";
import type {
  ConversationalAnchor,
  FocusedSurface,
  GlobalVoiceSession,
  PresenceSnapshot,
  VoiceRuntimeSnapshot,
} from "../../../api/voice";
import type { ChatVoiceControls } from "../ChatVoiceInput";

export interface GlobalVoiceHostProps {
  children: ReactNode;
  /** Preview data keeps the host deterministic in focused UI tests. */
  initialSnapshot?: VoiceRuntimeSnapshot | null;
  initialPresence?: PresenceSnapshot | null;
  initialExpanded?: boolean;
  /** Route/surface publishers use this thin bridge to update the active voice context. */
  contextPatch?: GlobalVoiceContextPatch;
}

export interface GlobalVoiceContextSnapshot {
  focused_surface: FocusedSurface;
  conversational_anchor: ConversationalAnchor | null;
  active_task: string | null;
}

export type GlobalVoiceContextPatch = Partial<GlobalVoiceContextSnapshot> & {
  /** Only conversation lifecycle publishers may explicitly clear the anchor. */
  anchor_action?: "replace";
};

export interface ConversationRefreshSignal {
  conversation_id: string;
  revision: number;
}

export interface GlobalVoiceContextBridgeValue {
  context: GlobalVoiceContextSnapshot;
  conversationRefresh: ConversationRefreshSignal | null;
  session: GlobalVoiceSession | null;
  updateContext: (patch: GlobalVoiceContextPatch) => void;
  chatVoiceControls: ChatVoiceControls | null;
  requestGlobalVoiceSession: () => void;
  speakAssistantMessage: (text: string) => void;
}

export type VoiceSurfaceHostMode = "standalone" | "desktop-skeleton";
