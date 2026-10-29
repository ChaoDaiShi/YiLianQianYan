import { useEffect, useState } from "react";
import { Move, X } from "lucide-react";
import { closeFloatingSurface, pinFloatingSurface, resizeFloatingSurface, useFloatingDrag, useFloatingSubscriber } from "../voice/floatingSurface";
import { UsageReport, type MonitorProjection } from "./TokenUsagePanel";
export default function MonitorOverlay() {
  const { state, error, send, expanded } = useFloatingSubscriber<MonitorProjection>("monitor"); const [pinned, setPinned] = useState(true); const [actionError, setActionError] = useState("");
  useEffect(() => { const timer = window.setInterval(() => { if (!document.hidden) void send("refresh"); }, 30000); return () => window.clearInterval(timer); }, []);
  async function resize(next: boolean) { try { await resizeFloatingSurface("monitor", next); } catch (cause) { setActionError(String(cause)); } }
  async function pin() { try { await pinFloatingSurface(!pinned); setPinned(!pinned); } catch (cause) { setActionError(String(cause)); } }
  const reportWindowError = (cause: unknown) => setActionError(String(cause));
  const orbDrag = useFloatingDrag(() => void resize(true), reportWindowError);
  const headerDrag = useFloatingDrag(undefined, reportWindowError);
  const status = error || actionError || state?.error ? "error" : state?.busy ? "loading" : state?.report?.pending_attempt_count ? "pending" : state?.report ? "ready" : "waiting";
  const statusText = { error: "数据异常", loading: "更新中", pending: "请求中", ready: "已更新", waiting: "待同步" }[status];
  const total = state?.report?.total_tokens;
  const statusDetail = error || actionError || state?.error || (status === "pending" ? `${state?.report?.pending_attempt_count} 个模型请求进行中` : state?.refreshedAt ? `更新于 ${new Date(state.refreshedAt).toLocaleTimeString()}` : "等待主窗口的真实用量数据");
  if (!expanded) return <div className="floating-orb-wrap"><button type="button" aria-label={`展开 Token 监控 · ${statusText}`} data-status={status} title={`${total?.toLocaleString() ?? "—"} Token。${statusText}：${statusDetail}。点击展开，按住拖动`} className="floating-orb monitor-orb" {...orbDrag}><strong className="monitor-orb-total">{total === undefined ? "—" : Intl.NumberFormat("zh-CN", { notation: "compact", maximumFractionDigits: 1 }).format(total)}</strong><span className="monitor-orb-unit">Token</span><span className="floating-orb-status" role="status" aria-live="polite">{statusText}</span></button><button type="button" className="floating-orb-close" aria-label="关闭监控浮窗" title="关闭监控浮窗" onClick={() => void closeFloatingSurface("monitor").catch(reportWindowError)}><X size={15}/></button></div>;
  return <main className="h-screen overflow-auto rounded-2xl border border-[var(--border-soft)] bg-[var(--surface-solid)] p-4 text-[var(--text)]"><header className="flex gap-3 items-center pb-4"><button className="floating-drag-handle monitor-drag-handle" aria-label="拖动监控浮窗" {...headerDrag}><Move size={16}/><strong>用量监控</strong></button><button onClick={() => void pin()}>{pinned ? "取消置顶" : "置顶"}</button><button onClick={() => void resize(false)}>收起</button><button aria-label="关闭监控浮窗" onClick={() => void closeFloatingSurface("monitor").catch(cause => setActionError(String(cause)))}>关闭</button></header>{error || actionError || state?.error ? <p role="alert">{error || actionError || state?.error}</p> : state?.report ? <><p className="text-xs mb-3">{state.from} ～ {state.to}</p><UsageReport report={state.report} /></> : <p>等待主窗口的真实用量数据…</p>}<button className="mt-4 text-sm" onClick={() => void send("refresh")}>刷新</button></main>;
}
