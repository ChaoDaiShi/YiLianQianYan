import { controlSessionHeaders } from "./controlSession";
import { API_BASE } from "./transport";

export interface ProductEvent {
  id: string;
  namespace: string;
  type: string;
  source: string;
  scope?: string;
  timestamp: number;
  schema_version: number;
  payload: Record<string, unknown>;
  [key: string]: unknown;
}

export function parseProductEventData(data: string): ProductEvent | null {
  try {
    const value = JSON.parse(data) as Partial<ProductEvent>;
    if (
      typeof value.id !== "string" ||
      typeof value.namespace !== "string" ||
      typeof value.type !== "string" ||
      typeof value.source !== "string" ||
      typeof value.timestamp !== "number" ||
      typeof value.schema_version !== "number" ||
      !value.payload ||
      typeof value.payload !== "object" ||
      Array.isArray(value.payload)
    ) {
      return null;
    }
    return value as ProductEvent;
  } catch {
    return null;
  }
}

export function subscribeToEvents(
  onEvent: (event: ProductEvent) => void,
  onError: (error: Error) => void = () => undefined,
): AbortController {
  const controller = new AbortController();
  void fetch(`${API_BASE}/api/events`, {
    method: "GET",
    headers: {
      Accept: "text/event-stream",
      ...controlSessionHeaders(),
    },
    signal: controller.signal,
  })
    .then(async (response) => {
      if (!response.ok || !response.body) {
        throw new Error(`event stream unavailable: HTTP ${response.status}`);
      }
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() ?? "";
        for (const line of lines) {
          if (!line.startsWith("data:")) continue;
          const data = line.slice(5).trim();
          if (data === "keepalive") continue;
          const event = parseProductEventData(data);
          if (event) onEvent(event);
        }
      }
    })
    .catch((error: unknown) => {
      if (!controller.signal.aborted) {
        onError(error instanceof Error ? error : new Error(String(error)));
      }
    });
  return controller;
}
