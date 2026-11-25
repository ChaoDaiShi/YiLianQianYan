import { controlSessionHeaders } from "./controlSession";
import { API_BASE, getSettings } from "./legacy";
import type { ProviderReadinessProjection } from "../types";
import type { ThinkingMode } from "../components/chat/chatRequestOptions";
import { request } from "../core/api/http";

export type ProviderConnectionKind = "model" | "stt" | "tts" | "embedding";

const NORMALIZED_ERRORS = new Set([
  "INVALID_CREDENTIAL",
  "PROVIDER_UNREACHABLE",
  "MODEL_NOT_FOUND",
  "RATE_LIMITED",
  "TIMEOUT",
  "INVALID_CONFIGURATION",
]);

export type ProviderConnectionResult =
  | { ok: true }
  | { ok: false; error: string };

export async function getProviderReadiness(): Promise<ProviderReadinessProjection | null> {
  const settings = await getSettings();
  return settings?.provider_readiness ?? null;
}
type ChatModelReadiness = ProviderReadinessProjection["model"] & { thinking_modes?: ThinkingMode[] };

export async function getChatModelReadiness(conversationId:string|null):Promise<ChatModelReadiness|null>{
 return request<ChatModelReadiness>("GET",`/api/chat/readiness${conversationId?`?conversation_id=${encodeURIComponent(conversationId)}`:""}`);
}

export interface CliProvider {
  id: string;
  label: string;
  ready: boolean;
}

export function listCliProviders() {
  return request<CliProvider[]>("GET", "/api/chat/cli-providers");
}

/** Calls the protected leaf route and deliberately discards provider bodies. */
export async function verifyProviderConnection(kind: ProviderConnectionKind): Promise<ProviderConnectionResult> {
  try {
    const response = await fetch(`${API_BASE}/api/providers/${kind}/verify`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        ...controlSessionHeaders(),
      },
    });
    if (response.ok) return { ok: true };
    const payload: unknown = await response.json().catch(() => null);
    const error = payload && typeof payload === "object" && "error" in payload
      ? (payload as { error?: unknown }).error
      : undefined;
    return {
      ok: false,
      error: typeof error === "string" && NORMALIZED_ERRORS.has(error)
        ? error
        : "INVALID_CONFIGURATION",
    };
  } catch {
    return { ok: false, error: "PROVIDER_UNREACHABLE" };
  }
}
