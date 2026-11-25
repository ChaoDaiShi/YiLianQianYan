import { API_BASE } from "./transport";
import { controlSessionHeaders } from "./controlSession";
import type { ApiResult } from "./taskWorld";

interface ErrorPayload {
  error?: string;
  code?: string;
  message?: string;
}

function parseJson(text: string): unknown {
  if (!text.trim()) return null;
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return null;
  }
}

function errorMessage(payload: unknown): string | null {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) return null;
  const value = payload as ErrorPayload;
  return typeof value.message === "string"
    ? value.message
    : typeof value.error === "string"
      ? value.error
      : null;
}

function errorCode(payload: unknown): string | null {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) return null;
  const value = payload as ErrorPayload;
  const code = value.code || value.error;
  return typeof code === "string" && /^[a-z0-9_\-]+$/.test(code) ? code : null;
}

export async function requestProofLoop<T>(
  method: string,
  path: string,
  body?: unknown,
  signal?: AbortSignal,
): Promise<ApiResult<T>> {
  try {
    const options: RequestInit = {
      method,
      headers: {
        "Content-Type": "application/json",
        ...controlSessionHeaders(),
      },
      signal,
    };
    if (body !== undefined) options.body = JSON.stringify(body);

    const response = await fetch(`${API_BASE}${path}`, options);
    const text = response.status === 204 ? "" : await response.text();
    const payload = parseJson(text);

    if (!response.ok) {
      const message = errorMessage(payload) || `HTTP ${response.status}`;
      return {
        ok: false,
        error: {
          status: response.status,
          code: errorCode(payload) || `http_${response.status}`,
          message,
        },
      };
    }

    if (!text) return { ok: true, data: {} as T };
    if (payload === null) {
      return {
        ok: false,
        error: {
          status: 0,
          code: "invalid_json",
          message: "ProofLoop API 返回了无效 JSON。",
        },
      };
    }
    return { ok: true, data: payload as T };
  } catch (error: unknown) {
    if (error instanceof DOMException && error.name === "AbortError") {
      return {
        ok: false,
        error: {
          status: 0,
          code: "aborted",
          message: "请求已被用户取消",
        },
      };
    }
    return {
      ok: false,
      error: {
        status: 0,
        code: "network_error",
        message: error instanceof Error ? error.message : String(error),
      },
    };
  }
}

