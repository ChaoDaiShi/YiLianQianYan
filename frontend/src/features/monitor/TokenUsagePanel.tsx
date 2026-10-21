import { useCallback, useEffect, useMemo, useState } from "react";
import { listLlmModels } from "../../api/system";
import type { LlmModel, LlmUsageReport } from "../../types";
import { Button, ErrorState, Skeleton } from "../../components/ui";
import { useLocation } from "react-router-dom";
import { useMonitorStore } from "./monitorStore";
import { desktopFloatingAvailable, openFloatingSurface, useFloatingPublisher } from "../voice/floatingSurface";
export interface MonitorProjection { report: LlmUsageReport | null; error: string; from: string; to: string; refreshedAt: number | null; busy: boolean }
export default function TokenUsagePanel() {
  const { from, to, modelId, report, error, busy, refreshedAt, refresh, setFilter } = useMonitorStore();
  const [models, setModels] = useState<LlmModel[]>([]);
  const [actionError, setActionError] = useState("");
  useEffect(() => { void listLlmModels().then(result => { if (result) setModels(result); }); }, []);
  return <section className="mb-5 rounded-xl border border-[var(--border-soft)] bg-[var(--surface-elevated)] p-5" aria-label="Token 用量监控"><div className="flex flex-wrap items-center gap-3 mb-4"><h2 className="font-medium mr-auto">Token 用量</h2>{desktopFloatingAvailable() && <Button size="sm" variant="secondary" onClick={() => void openFloatingSurface("monitor").catch(cause => setActionError(String(cause)))}>桌面监控浮窗</Button>}<Button size="sm" disabled={busy} onClick={() => void refresh()}>刷新用量</Button></div><div className="flex flex-wrap items-end gap-3 mb-5"><label className="text-xs">开始日期<input aria-label="用量开始日期" type="date" className="block mt-1 rounded border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2" value={from} onChange={event => setFilter("from", event.target.value)} /></label><label className="text-xs">结束日期<input aria-label="用量结束日期" type="date" className="block mt-1 rounded border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2" value={to} onChange={event => setFilter("to", event.target.value)} /></label><label className="text-xs">模型<select aria-label="筛选用量模型" className="block mt-1 rounded border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2" value={modelId} onChange={event => setFilter("modelId", event.target.value)}><option value="">全部模型</option>{models.map(model => <option key={model.id} value={model.id}>{model.label}</option>)}</select></label></div>{(error || actionError) && <ErrorState title="用量统计无法加载" description={error || actionError} />}{busy && !report ? <Skeleton className="h-40" /> : report && <UsageReport report={report} />}<p className="mt-3 text-xs text-[var(--text-muted)]">按本地日期统计。只汇总厂商报告的用量；模型池尝试的未知用量单独计数，失败尝试可能仍有已报告的 Token。{refreshedAt ? ` 更新于 ${new Date(refreshedAt).toLocaleTimeString()}` : ""}</p></section>;
}
export function UsageReport({ report }: { report: LlmUsageReport }) {
  const max = Math.max(1, ...report.days.map(day => day.total_tokens));
  return <><div className="grid grid-cols-3 gap-3">{[["输入 Token",report.prompt_tokens],["输出 Token",report.completion_tokens],["总 Token",report.total_tokens]].map(([label,value]) => <div key={label} className="rounded-xl bg-[var(--surface-muted)] p-4"><p className="text-xs text-[var(--text-muted)]">{label}</p><strong className="text-2xl tabular-nums">{Number(value).toLocaleString()}</strong></div>)}</div><div className="flex flex-wrap gap-5 my-4 text-xs"><span>已报告用量 {report.known_usage_count} 次</span><span>未知用量 {report.unknown_usage_count} 次</span><span>失败尝试 {report.failed_attempt_count} 次</span><span>进行中 {report.pending_attempt_count} 次</span></div><div aria-label="每日 Token 趋势" className="space-y-2 max-h-52 overflow-auto">{report.days.length === 0 ? <p className="text-sm text-[var(--text-muted)]">该时段没有已报告的用量。</p> : report.days.map(day => <div key={day.date} className="grid grid-cols-[90px_1fr_80px] gap-3 items-center text-xs"><span>{day.date}</span><div className="rounded-full bg-[var(--surface-muted)] h-3 overflow-hidden" title={`输入 ${day.prompt_tokens} · 输出 ${day.completion_tokens}`}><div className="h-full rounded-full" style={{ width: `${day.total_tokens / max * 100}%`, background: "var(--cyrene-gradient-primary)" }} /></div><span className="text-right tabular-nums">{day.total_tokens.toLocaleString()}</span></div>)}</div>{report.models?.length > 0 && <div className="overflow-auto mt-5"><table className="w-full text-xs text-left"><caption className="text-left py-2 text-[var(--text-muted)]">按模型统计</caption><thead><tr>{["模型","输入","输出","总量","未知","失败"].map(label => <th key={label} className="p-2">{label}</th>)}</tr></thead><tbody>{report.models.map(model => <tr key={`${model.model_id}-${model.model}`} className="border-t border-[var(--border-soft)]"><td className="p-2">{model.model}</td><td className="p-2">{model.prompt_tokens}</td><td className="p-2">{model.completion_tokens}</td><td className="p-2">{model.total_tokens}</td><td className="p-2">{model.unknown_usage_count}</td><td className="p-2">{model.failed_attempt_count}</td></tr>)}</tbody></table></div>}</>;
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
