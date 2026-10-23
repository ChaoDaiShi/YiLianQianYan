import { useCallback, useEffect, useState } from "react";
import { CalendarClock, History, Pause, Pencil, Play, Plus, RefreshCw, Trash2 } from "lucide-react";
import PageShell from "../../components/layout/PageShell";
import { Badge, Button, ConfirmDialog, EmptyState, Panel, showToast } from "../../components/ui";
import { listAutomations, deleteAutomation, runAutomation, setAutomationEnabled, type Automation } from "../../api/automations";
import { listWorkflowGraphs, type WorkflowGraphRecord } from "../../api/workflows";
import { listWorkspaces, type Workspace } from "../../api/workspaces";
import AutomationEditor, { weekdays } from "./AutomationEditor";
import AutomationHistory from "./AutomationHistory";

function scheduleLabel(automation: Automation) {
  const schedule = automation.schedule;
  if (schedule.kind === "once") return `仅一次 · ${schedule.local_at.replace("T", " ")}`;
  if (schedule.kind === "daily") return `每天 ${schedule.time}`;
  return `每周${schedule.weekdays.map(day => weekdays[day - 1]).join("、")} ${schedule.time}`;
}

export default function AutomationsPage() {
  const [items, setItems] = useState<Automation[]>([]);
  const [graphs, setGraphs] = useState<WorkflowGraphRecord[]>([]);
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [editing, setEditing] = useState<Automation | null | undefined>(undefined);
  const [history, setHistory] = useState<Automation | null>(null);
  const [deleting, setDeleting] = useState<Automation | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [refreshVersion, setRefreshVersion] = useState(0);
  const reload = useCallback(() => setRefreshVersion(current => current + 1), []);
  useEffect(() => {
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    async function load() {
      const responses = await Promise.allSettled([listAutomations(), listWorkflowGraphs(), listWorkspaces()]);
      if (!active) return;
      const failures: string[] = [];
      const [automations, workflowGraphs, workspaceList] = responses;
      if (automations.status === "fulfilled") setItems(automations.value.automations); else failures.push(String(automations.reason));
      if (workflowGraphs.status === "fulfilled" && workflowGraphs.value.ok) setGraphs(workflowGraphs.value.data); else failures.push(workflowGraphs.status === "rejected" ? String(workflowGraphs.reason) : workflowGraphs.value.ok ? "" : workflowGraphs.value.error);
      if (workspaceList.status === "fulfilled" && workspaceList.value.ok) setWorkspaces(workspaceList.value.data); else failures.push(workspaceList.status === "rejected" ? String(workspaceList.reason) : workspaceList.value.ok ? "" : workspaceList.value.error);
      setError(failures.filter(Boolean).join("；")); setLoading(false);
      if (!failures.length) timer = setTimeout(() => void load(), 10000);
    }
    void load();
    return () => { active = false; clearTimeout(timer); };
  }, [refreshVersion]);

  async function action(item: Automation, run: () => Promise<void>) {
    setBusyId(item.id); setError("");
    try { await run(); reload(); }
    catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); }
    finally { setBusyId(null); }
  }
  return <PageShell title="时间任务" description="按计划运行已有工作流，查看执行结果与审批" className="page-canvas" maxWidth="dashboard" actions={<><Button size="sm" variant="ghost" onClick={reload} aria-label="刷新时间任务"><RefreshCw size={15} /></Button><Button size="sm" onClick={() => setEditing(null)}><Plus size={15} />新建时间任务</Button></>}>
    <div className="space-y-5">
      {error && <p role="alert" className="text-sm text-[var(--danger)]">{error}</p>}
      {loading ? <p role="status">正在读取时间任务…</p> : items.length === 0 ? <EmptyState icon={<CalendarClock size={30} />} title="安排第一次自动执行" description="选择工作流、工作空间上下文与执行时间。" action={<Button onClick={() => setEditing(null)}>新建时间任务</Button>} /> : <div className="grid grid-cols-1 xl:grid-cols-2 gap-4">{items.map(item => <Panel key={item.id} className="space-y-4">
        <div className="flex items-start gap-3"><CalendarClock size={21} className="text-[var(--accent-primary)] shrink-0 mt-0.5" /><div className="flex-1 min-w-0"><h2 className="font-semibold text-sm break-words">{item.name}</h2><p className="text-xs text-[var(--text-secondary)] mt-1">{scheduleLabel(item)} · {item.timezone}</p></div><Badge tone={item.enabled ? "success" : "default"}>{item.enabled ? "已启用" : "未启用"}</Badge></div>
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-xs"><dt className="text-[var(--text-muted)]">工作流</dt><dd>{graphs.find(graph => graph.id === item.workflow_graph_id)?.name ?? item.workflow_graph_id}</dd><dt className="text-[var(--text-muted)]">工作空间上下文</dt><dd>{workspaces.find(workspace => workspace.id === item.workspace_id)?.name ?? item.workspace_id}</dd><dt className="text-[var(--text-muted)]">下次执行</dt><dd>{item.next_run_at ? new Date(item.next_run_at).toLocaleString("zh-CN", { timeZone: item.timezone }) : item.enabled ? "无下一次执行" : "—"}</dd></dl>
        <div className="flex flex-wrap gap-1.5 border-t border-[var(--border-soft)] pt-3"><Button size="sm" variant="secondary" disabled={busyId !== null} onClick={() => void action(item, async () => { const result = await runAutomation(item.id); setHistory(item); if (result.occurrence.status === "failed") showToast(result.occurrence.error ?? "工作流启动失败", "error"); })}><Play size={13} />立即运行</Button><Button size="sm" variant="secondary" disabled={busyId !== null} onClick={() => void action(item, async () => { await setAutomationEnabled(item.id, !item.enabled, item.revision); })}>{item.enabled ? <Pause size={13} /> : <CalendarClock size={13} />}{item.enabled ? "暂停" : "启用"}</Button><Button size="sm" variant="ghost" onClick={() => setHistory(item)}><History size={13} />历史</Button><Button size="sm" variant="ghost" disabled={busyId !== null} onClick={() => setEditing(item)} aria-label={`编辑 ${item.name}`}><Pencil size={13} /></Button><Button size="sm" variant="ghost" className="ml-auto" disabled={busyId !== null} onClick={() => setDeleting(item)} aria-label={`删除 ${item.name}`}><Trash2 size={13} /></Button></div>
      </Panel>)}</div>}
    </div>
    {editing !== undefined && <AutomationEditor value={editing} graphs={graphs} workspaces={workspaces} onClose={() => setEditing(undefined)} onSaved={() => { setEditing(undefined); reload(); }} />}
    {history && <AutomationHistory automation={history} onClose={() => setHistory(null)} />}
    <ConfirmDialog open={Boolean(deleting)} title="删除时间任务" description={`删除“${deleting?.name ?? ""}”的执行计划？已有工作流运行记录仍保留。`} tone="danger" confirmLabel="删除" onCancel={() => setDeleting(null)} onConfirm={async () => { if (!deleting) return; const item = deleting; setDeleting(null); await action(item, async () => { await deleteAutomation(item.id); }); }} />
  </PageShell>;
}
