import { createContext, useContext } from "react";
import type {
  GlobalVoiceContextBridgeValue,
  GlobalVoiceContextPatch,
  GlobalVoiceContextSnapshot,
} from "../model/types";

/**
 * The bridge a surface publishes through and its consumers read from.
 *
 * It is deliberately narrow: surfaces may publish a context patch and ask for
 * a session, but they cannot reach the runtime's state machine.
 */
export const GlobalVoiceContextBridge = createContext<GlobalVoiceContextBridgeValue | null>(null);

export function useGlobalVoiceContext(): GlobalVoiceContextBridgeValue {
  const value = useContext(GlobalVoiceContextBridge);
  if (!value) {
    throw new Error("useGlobalVoiceContext must be used inside GlobalVoiceHost");
  }
  return value;
}

/**
 * Merge a patch into the current context.
 *
 * Clearing the conversational anchor is the one destructive patch, so it is
 * gated behind an explicit `anchor_action: "replace"` — a publisher cannot
 * drop the anchor by merely omitting or nulling the field.
 */
export function mergeVoiceContext(
  current: GlobalVoiceContextSnapshot,
  patch: GlobalVoiceContextPatch,
): GlobalVoiceContextSnapshot {
  const next = { ...current };
  for (const key of [
    "focused_surface",
    "conversational_anchor",
    "active_task",
  ] as const) {
    if (key === "conversational_anchor" && patch[key] === null && patch.anchor_action !== "replace") continue;
    if (patch[key] !== undefined) next[key] = patch[key] as never;
  }
  return next;
}
