// Task World transport — the task-world envelope differs from the generic
// one, so this parses `code` and `message` out of a refusal instead of
// collapsing it to a status line.
import { API_BASE } from "../transport";
import { controlSessionHeaders } from "../controlSession";
import type {
  ApiResult,
  ErrorPayload,
  TaskGraphDefinition,
  TaskGraphSummary,
} from "./types";

export async function requestTaskWorld<T>(
  method: string,
  path: string,
  body?: unknown,
): Promise<ApiResult<T>> {
  try {
    const options: RequestInit = {
      method,
      headers: {
        "Content-Type": "application/json",
        ...controlSessionHeaders(),
      },
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
          message: "Task World API 返回了无效 JSON。",
        },
      };
    }
    return { ok: true, data: payload as T };
  } catch (error: unknown) {
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
  const code = (payload as ErrorPayload).error;
  return typeof code === "string" && /^[a-z0-9_\-]+$/.test(code) ? code : null;
}

export function graphSummary(graph: TaskGraphDefinition): TaskGraphSummary {
  return {
    id: graph.id,
    schema_version: graph.schema_version,
    revision: graph.revision,
    node_count: graph.nodes.length,
    edge_count: graph.edges.length,
  };
}
