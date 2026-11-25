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

export type EventConnectionState = "connecting" | "connected" | "reconnecting" | "unavailable";

export function subscribeToEvents(
  onEvent: (event: ProductEvent) => void,
  onError: (error: Error) => void = () => undefined,
  onConnection: (state: EventConnectionState) => void = () => undefined,
): AbortController {
  const controller = new AbortController();
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  let reader: ReadableStreamDefaultReader<Uint8Array> | undefined;
  let retryDelay = 1000;
  let lastEventId: string | undefined;
  controller.signal.addEventListener("abort", () => {
    clearTimeout(retryTimer);
    void reader?.cancel().catch(() => undefined);
  }, { once: true });

  const connect = async () => {
    if (controller.signal.aborted) return;
    let denied = false;
    try {
      const cursor = lastEventId ? `?after=${encodeURIComponent(lastEventId)}` : "";
      const response = await fetch(`${API_BASE}/api/events${cursor}`, {
        method: "GET", headers: { Accept: "text/event-stream", ...controlSessionHeaders() }, signal: controller.signal,
      });
      denied = response.status === 401 || response.status === 403;
      if (!response.ok || !response.body) throw new Error(`实时事件连接不可用（HTTP ${response.status}）`);
      if (controller.signal.aborted) return;
      onConnection("connected");
      reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      while (!controller.signal.aborted) {
        const { done, value } = await reader.read();
        if (done) break;
        retryDelay = 1000;
        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n"); buffer = lines.pop() ?? "";
        for (const line of lines) {
          if (!line.startsWith("data:")) continue;
          const data = line.slice(5).trim();
          if (data === "keepalive") continue;
          const event = parseProductEventData(data);
          if (event) {
            onEvent(event);
            lastEventId = event.type === "core.events.lagged" ? undefined : event.id;
          }
        }
      }
      if (!controller.signal.aborted) throw new Error("实时事件连接已断开，正在重新连接。");
    } catch (cause) {
      if (controller.signal.aborted) return;
      onError(cause instanceof Error ? cause : new Error(String(cause)));
      onConnection(denied ? "unavailable" : "reconnecting");
      if (!denied) {
        retryTimer = setTimeout(() => { void connect(); }, retryDelay);
        retryDelay = Math.min(retryDelay * 2, 10000);
      }
    } finally { reader = undefined; }
  };
  onConnection("connecting");
  void connect();
  return controller;
}
