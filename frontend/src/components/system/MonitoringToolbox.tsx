import { useCallback, useEffect, useState, type CSSProperties } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { listPendingApprovals } from "../../api/approvals";
import { getIsolationStatus } from "../../api/system";
import type { PendingApproval } from "../../types/approval";
import { Button, Modal } from "../ui";
import { useProductPreferences } from "./useProductPreferences";
import { MONITOR_CARDS } from "./productPreferences";
import { useVisibleRefresh } from "./useVisibleRefresh";
import "./monitoringToolbox.css";

const LABELS: Record<string, string> = { health: "服务健康", processes: "进程", resources: "系统资源", approvals: "待审批", diagnostics: "诊断" };
export default function MonitoringToolbox({ data, health }: { data: Record<string, unknown> | null; health: Record<string, unknown> | null }) {
  const { preferences, loaded, error, save } = useProductPreferences();
  const [layout, setLayout] = useState(preferences.monitor);
  const [saving, setSaving] = useState(false);
  const [approvals, setApprovals] = useState<PendingApproval[] | null>(null);
  const [diagnostics, setDiagnostics] = useState<Record<string, unknown> | null>(null);
  const [approvalOpen, setApprovalOpen] = useState(false);
  const location = useLocation(); const navigate = useNavigate();
  useEffect(() => { setLayout(preferences.monitor); }, [preferences]);
  useEffect(() => { if (new URLSearchParams(location.search).get("card") === "approvals") setApprovalOpen(true); }, [location.search]);
  const refresh = useCallback(async () => {
    const [pending, diagnostic] = await Promise.allSettled([listPendingApprovals(), getIsolationStatus()]);
    setApprovals(pending.status === "fulfilled" ? pending.value : null);
    setDiagnostics(diagnostic.status === "fulfilled" ? diagnostic.value as unknown as Record<string, unknown> : null);
  }, []);
  useVisibleRefresh(refresh);
  const values: Record<string, string> = {
    health: typeof health?.status === "string" ? health.status : "unavailable",
    processes: typeof data?.process_count === "number" ? `${data.process_count} 个系统进程` : "unavailable",
    resources: data?.memory && typeof data.memory === "object" ? `内存 ${(data.memory as Record<string, unknown>).used_gb ?? "unavailable"} / ${(data.memory as Record<string, unknown>).total_gb ?? "unavailable"} GB` : "unavailable",
    approvals: approvals === null ? "unavailable" : approvals.length === 0 ? "暂无待审批操作" : `${approvals.length} 项待审批`,
    diagnostics: diagnostics ? `进程隔离：${diagnostics.process_containment === true ? "可用" : "unavailable"}；受限令牌：${diagnostics.restricted_token === true ? "可用" : "unavailable"}` : "unavailable",
  };
  const enabled = preferences.enabled_modules.includes("monitoring");
  return <section className="mb-5 rounded-xl border border-[var(--border-soft)] p-4" aria-label="监控工具箱">
    <div className="mb-3 flex flex-wrap items-center gap-2"><h2 className="mr-auto font-medium">监控工具箱</h2><Button variant="secondary" size="sm" onClick={() => { setApprovalOpen(true); void refresh(); }}>审批入口</Button>
      <select aria-label="监控布局模式" value={layout.mode} disabled={!enabled} onChange={(event) => setLayout({ ...layout, mode: event.target.value as "grid" | "free" })}><option value="grid">网格</option><option value="free">自由布局</option></select>
      <Button size="sm" disabled={!loaded || saving || !enabled} onClick={() => { setSaving(true); void save({ ...preferences, monitor: layout }).catch(() => undefined).finally(() => setSaving(false)); }}>保存布局</Button>
    </div>
    {error && <p role="alert" className="text-sm text-[var(--danger)]">{error}</p>}
    {!enabled ? <p>监控模块已禁用，可在侧栏“定制导航”中重新开启。审批入口仍可使用。</p> : <>
      <div className="mb-3 flex flex-wrap gap-2">{MONITOR_CARDS.filter((id) => !layout.cards.some((card) => card.id === id)).map((id) => <button type="button" key={id} onClick={() => setLayout({ ...layout, cards: [...layout.cards, { id, x: 0, y: 0, width: 2, height: 1 }] })}>添加{LABELS[id]}</button>)}</div>
      {layout.cards.length === 0 && <p className="text-sm">工具箱为空，选择需要显示的卡片。</p>}
      <div className={`monitor-toolbox-grid ${layout.mode === "free" ? "monitor-toolbox-free" : ""}`}>
        {layout.cards.map((card, index) => <article key={card.id} style={{ "--card-x": card.x + 1, "--card-y": card.y + 1, "--card-w": card.width, "--card-h": card.height } as CSSProperties} className="monitor-toolbox-card rounded-xl bg-[var(--surface-muted)] p-3">
          <div className="flex items-center justify-between gap-2"><h3 className="text-sm font-medium">{LABELS[card.id]}</h3><button type="button" aria-label={`移除${LABELS[card.id]}`} onClick={() => setLayout({ ...layout, cards: layout.cards.filter((item) => item.id !== card.id) })}>移除</button></div>
          <p className="my-3 break-words text-sm" aria-live="polite">{values[card.id]}</p>
          <div className="flex flex-wrap items-center gap-2 text-xs">
            <button type="button" disabled={index === 0} aria-label={`前移${LABELS[card.id]}`} onClick={() => { const cards = [...layout.cards]; [cards[index - 1], cards[index]] = [cards[index], cards[index - 1]]; setLayout({ ...layout, cards }); }}>前移</button>
            {(["x", "y", "width", "height"] as const).filter((key) => layout.mode === "free" || ["width", "height"].includes(key)).map((key) => <label key={key}>{({ x: "列", y: "行", width: "宽", height: "高" })[key]}<input aria-label={`${LABELS[card.id]} ${key}`} type="number" className="ml-1 w-12" min={key === "width" || key === "height" ? 1 : 0} max={key === "y" ? 30 : key === "height" ? 3 : key === "x" ? 3 : 4} value={card[key]} onChange={(event) => { const next = { ...card, [key]: Number(event.target.value) }; if (next.x + next.width > 4) next.x = 4 - next.width; setLayout({ ...layout, cards: layout.cards.map((item) => item.id === card.id ? next : item) }); }} /></label>)}
          </div>
        </article>)}
      </div><p className="mt-2 text-xs text-[var(--text-faint)]">仅显示实际接口数据；页面隐藏时停止自动刷新，前台最多每 30 秒刷新一次。</p>
    </>}
    <Modal open={approvalOpen} onClose={() => setApprovalOpen(false)} title="待审批操作">
      {approvals === null ? <p role="status">审批数据 unavailable，请稍后重试。</p> : approvals.length === 0 ? <p>暂无待审批操作。</p> : approvals.map((approval) => <div key={approval.approval_id} className="mb-3"><p>{approval.tool_name}</p><Button variant="secondary" onClick={() => navigate(`/chat/${encodeURIComponent(approval.conversation_id)}`)}>前往对应对话核对并审批</Button></div>)}
      <Button variant="secondary" onClick={() => void refresh()}>刷新审批</Button>
    </Modal>
  </section>;
}
