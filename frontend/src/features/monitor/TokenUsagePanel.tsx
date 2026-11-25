import { useCallback, useEffect, useMemo, useState } from "react";
import { listLlmModels } from "../../api/system";
import type { LlmModel, LlmUsageReport } from "../../types";
import { Button, ErrorState, Skeleton } from "../../components/ui";
import { useLocation } from "react-router-dom";
import { useMonitorStore } from "./monitorStore";
import { desktopFloatingAvailable, openFloatingSurface, useFloatingPublisher } from "../voice/floatingSurface";

export interface MonitorProjection {
  report: LlmUsageReport | null;
  error: string;
  from: string;
  to: string;
  refreshedAt: number | null;
  busy: boolean;
}

export default function TokenUsagePanel() {
  const { from, to, modelId, report, error, busy, refreshedAt, refresh, setFilter } = useMonitorStore();
  const [models, setModels] = useState<LlmModel[]>([]);
  const [actionError, setActionError] = useState("");
  useEffect(() => { void listLlmModels().then(result => { if (result) setModels(result); }); }, []);

  return <section className="glass-card mb-4 rounded-xl border border-[var(--border-soft)] bg-[var(--surface-elevated)] p-4" aria-label="Token 用量监控">
    <div className="mb-3 flex flex-wrap items-center gap-2">
      <h2 className="mr-auto font-medium">Token 用量</h2>
      {desktopFloatingAvailable() && <Button size="sm" variant="secondary" onClick={() => {
        setActionError("");
        void openFloatingSurface("monitor").catch(cause => setActionError(String(cause)));
      }}>桌面监控浮窗</Button>}
      <Button size="sm" disabled={busy} onClick={() => void refresh()}>刷新用量</Button>
    </div>
    <p className="mb-2 text-xs text-[var(--text-secondary)]">
      {from} ～ {to}{refreshedAt && " · 更新于 " + new Date(refreshedAt).toLocaleTimeString()}
    </p>
    {error && <ErrorState title="用量统计无法加载" description={error} />}
    {actionError && <ErrorState title="监控浮窗无法打开" description={actionError} />}
    {busy && !report ? <Skeleton className="h-14" /> : report && <UsageSummary report={report} />}
    <details className="mt-3">
      <summary className="cursor-pointer text-sm text-[var(--text-secondary)]">筛选与用量明细</summary>
      <div className="my-3 flex flex-wrap items-end gap-3">
        <label className="text-xs">开始日期<input aria-label="用量开始日期" type="date" className="mt-1 block rounded border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2" value={from} onChange={event => setFilter("from", event.target.value)} /></label>
        <label className="text-xs">结束日期<input aria-label="用量结束日期" type="date" className="mt-1 block rounded border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2" value={to} onChange={event => setFilter("to", event.target.value)} /></label>
        <label className="text-xs">模型<select aria-label="筛选用量模型" className="mt-1 block max-w-full rounded border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2" value={modelId} onChange={event => setFilter("modelId", event.target.value)}><option value="">全部模型</option>{models.map(model => <option key={model.id} value={model.id}>{model.label}</option>)}</select></label>
      </div>
      {report && <UsageReport report={report} />}
      <p className="mt-3 text-xs text-[var(--text-muted)]">按本地日期统计。总量只汇总厂商报告的 Token，未知用量单独计数。</p>
    </details>
  </section>;
}

export function UsageSummary({ report }: { report: LlmUsageReport }) {
  const hasUnknownTotal = report.known_usage_count === 0 && (report.unknown_usage_count > 0 || report.pending_attempt_count > 0);
  return <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1">
    <div><strong className="text-2xl tabular-nums">{hasUnknownTotal ? "—" : report.total_tokens.toLocaleString()}</strong> <span className="text-xs text-[var(--text-secondary)]">已报告 Token</span></div>
    {report.unknown_usage_count > 0 && <span className="text-xs">未知用量 {report.unknown_usage_count} 次</span>}
    {report.failed_attempt_count > 0 && <span className="text-xs text-[var(--danger)]">失败 {report.failed_attempt_count} 次</span>}
    {report.pending_attempt_count > 0 && <span className="text-xs" role="status">进行中 {report.pending_attempt_count} 次</span>}
  </div>;
}

function UsageReport({ report }: { report: LlmUsageReport }) {
  const max = Math.max(1, ...report.days.map(day => day.total_tokens));
  return <>
    <div className="mb-3 flex flex-wrap gap-x-4 gap-y-2 text-sm">
      <span>输入 {report.prompt_tokens.toLocaleString()} Token</span>
      <span>输出 {report.completion_tokens.toLocaleString()} Token</span>
      <span>已报告 {report.known_usage_count} 次</span>
    </div>
    <div aria-label="每日 Token 趋势" className="max-h-52 space-y-2 overflow-auto">
      {report.days.length === 0 ? <p className="text-sm text-[var(--text-muted)]">该时段没有已报告的用量。</p> : report.days.map(day => (
        <div key={day.date} className="grid grid-cols-[6rem_minmax(0,1fr)_5rem] items-center gap-3 text-xs">
          <span>{day.date}</span>
          <div className="h-3 overflow-hidden rounded-full bg-[var(--surface-muted)]" title={"输入 " + day.prompt_tokens + " · 输出 " + day.completion_tokens}>
            <div className="h-full rounded-full" style={{ width: day.total_tokens / max * 100 + "%", background: "var(--cyrene-gradient-primary)" }} />
          </div>
          <span className="text-right tabular-nums">{day.total_tokens.toLocaleString()}</span>
        </div>
      ))}
    </div>
    {report.models?.length > 0 && <div className="mt-4 overflow-auto">
      <table className="w-full text-left text-xs">
        <caption className="py-2 text-left text-[var(--text-muted)]">按模型统计</caption>
        <thead><tr>{["模型", "输入", "输出", "总量", "未知", "失败"].map(label => <th key={label} className="p-2">{label}</th>)}</tr></thead>
        <tbody>{report.models.map(model => <tr key={model.model_id + ":" + model.model} className="border-t border-[var(--border-soft)]">
          <td className="p-2 break-words">{model.model}</td><td className="p-2">{model.prompt_tokens}</td><td className="p-2">{model.completion_tokens}</td><td className="p-2">{model.total_tokens}</td><td className="p-2">{model.unknown_usage_count}</td><td className="p-2">{model.failed_attempt_count}</td>
        </tr>)}</tbody>
      </table>
    </div>}
  </>;
}

/** Mount once in the main application, including while the user is in a call. */
export function MonitorDataHost() {
  const { report, error, from, to, modelId, refreshedAt, busy, refresh } = useMonitorStore();
  const refreshCurrent = useCallback(() => refresh(), [refresh, from, to, modelId]);
  const { pathname } = useLocation();
  const [overlayOpen,setOverlayOpen]=useState(false);
  const visible=pathname==="/system"||overlayOpen;
  useEffect(()=>{if(!visible)return;void refreshCurrent();const timer=window.setInterval(()=>void refreshCurrent(),5000);return()=>window.clearInterval(timer);},[visible,refreshCurrent]);
  const projection = useMemo(() => ({ report, error, from, to, refreshedAt, busy }), [report,error,from,to,refreshedAt,busy]);
  useFloatingPublisher<MonitorProjection>("monitor", projection, action => { if (action === "opened") setOverlayOpen(true); else if (action === "close") setOverlayOpen(false); else if (action === "refresh") void refresh(); });
  return null;
}
