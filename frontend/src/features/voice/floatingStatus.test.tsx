import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import CallOverlay from "./CallOverlay";
import MonitorOverlay from "../monitor/MonitorOverlay";
import type { CallPresentation } from "./callPresentation";
import type { MonitorProjection } from "../monitor/TokenUsagePanel";

const projection = vi.hoisted(() => ({ state: null as unknown, error: null as string | null }));
vi.mock("./floatingSurface", async importOriginal => ({
  ...await importOriginal<typeof import("./floatingSurface")>(),
  useFloatingSubscriber: () => ({ ...projection, expanded: false, send: vi.fn() }),
}));
const call: CallPresentation = {
  phase: "listening", conversation_title: "当前通话", browsing_conversation_title: null,
  can_switch_conversation: false, session_id: "call-1", conversation_id: "conversation-1",
  muted: false, speaker_enabled: true, connected_at: null, caption: "", notice: null,
  checks: { session: true, microphone: true, stt: true, tts: true, output: true },
};
const monitor: MonitorProjection = {
  report: { prompt_tokens: 80, completion_tokens: 20, total_tokens: 100, known_usage_count: 1,
    unknown_usage_count: 0, failed_attempt_count: 0, pending_attempt_count: 0, timezone_offset_minutes: -480, days: [], models: [] },
  from: "2026-10-21", to: "2026-10-21", error: "", refreshedAt: 1, busy: false,
};
describe("collapsed floating status", () => {
  it.each(["listening", "thinking", "speaking", "muted", "ended", "error"] as const)("shows the published call phase %s", phase => {
    projection.state = { ...call, phase }; projection.error = null;
    const html = renderToStaticMarkup(<CallOverlay />);
    expect(html).toContain(`data-status="${phase}"`);
    expect(html).toContain('role="status"');
    expect(html).toContain("点击展开，按住拖动");
    expect(html).toContain("展开小涟通话 · ");
  });
  it("shows synchronization while waiting for a main-window call projection", () => {
    projection.state = null; projection.error = null;
    const html = renderToStaticMarkup(<CallOverlay />);
    expect(html).toContain("同步中"); expect(html).not.toContain("正在连接");
  });
  it.each([
    [monitor, "ready", "已更新"],
    [{ ...monitor, busy: true }, "loading", "更新中"],
    [{ ...monitor, report: { ...monitor.report!, pending_attempt_count: 2 } }, "pending", "请求中"],
    [{ ...monitor, report: null, error: "真实统计请求失败" }, "error", "数据异常"],
    [null, "waiting", "待同步"],
  ])("shows published monitor facts without inventing runtime state", (state, status, label) => {
    projection.state = state; projection.error = null;
    const html = renderToStaticMarkup(<MonitorOverlay />);
    expect(html).toContain(`data-status="${status}"`);
    expect(html).toContain(label); expect(html).toContain('role="status"');
    expect(html).toContain("展开 Token 监控 · ");
  });
});
