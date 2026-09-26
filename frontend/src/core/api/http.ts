// ============================================================
// HTTP core — the one place that talks to the backend over fetch.
//
// Everything above this file (the `src/api/*` domain clients, the feature
// modules that call them) uses one of the two shapes below:
//
//   request<T>       — null on any failure. For callers that render "no data".
//   requestResult<T> — the status and the backend's message on failure. For
//                      callers that must show *why* a write was refused.
//
// Both attach the control-session header, so no caller has to remember to.
//
// Note on the import below: `controlSessionHeaders` lives in
// `src/api/controlSession.ts`, not here, because roughly fifteen modules and
// their `vi.mock("./controlSession", ...)` calls depend on that path.
// Re-exporting it from here instead would quietly stop those mocks from
// applying to this file. Recorded rather than fixed; moving it is a change to
// fifteen test files for no boundary gain.
// ============================================================

import { controlSessionHeaders } from "../../api/controlSession";

export const API_BASE =
  (import.meta.env.VITE_API_BASE as string | undefined)?.replace(/\/$/, "") ||
  "http://127.0.0.1:9420";

/** A request that could not be completed. `status` is 0 for a transport error. */
export type ApiFailure = { ok: false; status: number; error: string };

export type ApiResult<T> = { ok: true; data: T } | ApiFailure;

function jsonInit(method: string, body?: unknown): RequestInit {
  const opts: RequestInit = {
    method,
    headers: {
      "Content-Type": "application/json",
      ...controlSessionHeaders(),
    },
  };
  if (body !== undefined) opts.body = JSON.stringify(body);
  return opts;
}

/**
 * Issue a request, returning `null` on any failure — a non-2xx status, an
 * unparseable body, or a transport error. The failure is logged, not thrown.
 */
export async function request<T>(
  method: string,
  path: string,
  body?: unknown
): Promise<T | null> {
  try {
    const res = await fetch(`${API_BASE}${path}`, jsonInit(method, body));
    if (!res.ok) {
      console.error(`API ${method} ${path}: ${res.status}`);
      return null;
    }
    if (res.status === 204) return {} as T;
    const text = await res.text();
    if (!text) return {} as T;
    return JSON.parse(text) as T;
  } catch (e) {
    console.error(`API ${method} ${path} failed:`, e);
    return null;
  }
}

/**
 * Like `request`, but surfaces the backend's validation message to the caller
 * instead of discarding it. A body shaped `{error}` or `{message}` is preferred;
 * otherwise the raw body text is used.
 */
export async function requestResult<T>(
  method: string,
  path: string,
  body?: unknown
): Promise<ApiResult<T>> {
  try {
    const res = await fetch(`${API_BASE}${path}`, jsonInit(method, body));
    const text = await res.text();
    if (!res.ok) {
      let error = `HTTP ${res.status}`;
      try {
        const parsed = JSON.parse(text) as { error?: string; message?: string };
        if (parsed && typeof parsed === "object") {
          const msg = parsed.error || parsed.message;
          if (msg) error = String(msg);
        }
      } catch {
        if (text.trim()) error = text.trim();
      }
      return { ok: false, status: res.status, error };
    }
    if (!text) return { ok: true, data: {} as T };
    return { ok: true, data: JSON.parse(text) as T };
  } catch (e) {
    return { ok: false, status: 0, error: String(e) };
  }
}
