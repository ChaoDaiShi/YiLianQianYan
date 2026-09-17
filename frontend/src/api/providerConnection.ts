import { controlSessionHeaders } from "./controlSession";
import { API_BASE, getSettings } from "./legacy";
import type { ProviderReadinessProjection } from "../types";

export type ProviderConnectionKind = "model" | "stt" | "tts";

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
