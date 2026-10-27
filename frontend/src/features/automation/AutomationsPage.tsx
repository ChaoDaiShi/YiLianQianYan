import WorkspaceIcon from "../../components/ui/WorkspaceIcon";
import { useCallback, useEffect, useState } from "react";
import { Clock3, History, Info, MoreHorizontal, Pencil, Play, Plus, RefreshCw, Trash2 } from "lucide-react";
import PageShell from "../../components/layout/PageShell";
import { Badge, Button, ConfirmDialog, EmptyState, showToast } from "../../components/ui";
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
  const [filter, setFilter] = useState<"all" | "enabled" | "paused">("all");
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
  const visibleItems = items.filter(item => filter === "all" || (filter === "enabled" ? item.enabled : !item.enabled));
  return <PageShell title="定时任务" description="安排工作，按计划完成后回来查看结果。" variant="workspace" className="page-canvas management-automations-page" contentClassName="management-automations-content" actions={<><Button size="sm" variant="secondary" onClick={reload} aria-label="刷新定时任务"><RefreshCw size={15} />刷新</Button><Button size="sm" onClick={() => setEditing(null)}><Plus size={15} />新建任务</Button></>}>
    <div className="management-automations-workspace">
      <div className="management-status-tabs" aria-label="定时任务状态筛选">
        {([{ id: "all", label: "全部" }, { id: "enabled", label: "已启用" }, { id: "paused", label: "已暂停" }] as const).map(tab => <button type="button" key={tab.id} aria-pressed={filter === tab.id} onClick={() => setFilter(tab.id)}>{tab.label}</button>)}
      </div>
      {error && <p role="alert" className="text-sm text-[var(--danger)]">{error}</p>}
      <section className="management-automation-list scrollbar-thin" aria-label="定时任务列表">
        {loading ? <p role="status" className="p-5">正在读取定时任务…</p> : error && items.length === 0 ? null : items.length === 0 ? <EmptyState icon={<WorkspaceIcon name="task-calendar" size={30} />} title="安排第一次自动执行" description="选择工作流、工作空间上下文与执行时间。" action={<Button onClick={() => setEditing(null)}>新建任务</Button>} /> : visibleItems.length === 0 ? <EmptyState title={filter === "enabled" ? "暂无已启用的任务" : "暂无已暂停的任务"} action={<Button variant="secondary" onClick={() => setFilter("all")}>查看全部任务</Button>} /> : visibleItems.map(item => <article key={item.id} className="management-automation-row">
          <span className="management-automation-icon" aria-hidden="true"><WorkspaceIcon name="task-calendar" size={24} /></span>
          <div className="management-automation-name"><h2>{item.name}</h2><p>{item.target?.kind === "agent_turn" ? "QQ 机器人目标任务" : graphs.find(graph => graph.id === item.workflow_graph_id)?.name ?? item.workflow_graph_id}</p><small>工作空间：{workspaces.find(workspace => workspace.id === item.workspace_id)?.name ?? item.workspace_id}</small></div>
          <div className="management-automation-time"><Clock3 size={18} aria-hidden="true" /><div><p>{scheduleLabel(item)} · {item.timezone === "Asia/Shanghai" ? "上海时间" : item.timezone}</p><small>下次执行：{item.next_run_at ? new Date(item.next_run_at).toLocaleString("zh-CN", { timeZone: item.timezone }) : item.enabled ? "无下一次执行" : "—"}</small></div></div>
          <Badge tone={item.enabled ? "success" : "default"}>{item.enabled ? "已启用" : "已暂停"}</Badge>
          <button type="button" className="management-automation-switch" role="switch" aria-checked={item.enabled} aria-label={`${item.enabled ? "暂停" : "启用"} ${item.name}`} disabled={busyId !== null} onClick={() => void action(item, async () => { await setAutomationEnabled(item.id, !item.enabled, item.revision); })}><span /></button>
          <details className="management-automation-menu"><summary aria-label={`更多操作 ${item.name}`}><MoreHorizontal size={19} /></summary><div>
            <Button size="sm" variant="ghost" disabled={busyId !== null} onClick={() => void action(item, async () => { const result = await runAutomation(item.id); setHistory(item); if (result.occurrence.status === "failed") showToast(result.occurrence.error ?? "工作流启动失败", "error"); })}><Play size={14} />立即运行</Button>
            <Button size="sm" variant="ghost" onClick={() => setHistory(item)}><History size={14} />运行历史</Button>
            <Button size="sm" variant="ghost" disabled={busyId !== null} onClick={() => setEditing(item)} aria-label={`编辑 ${item.name}`}><Pencil size={14} />编辑任务</Button>
            <Button size="sm" variant="ghost" disabled={busyId !== null} onClick={() => setDeleting(item)} aria-label={`删除 ${item.name}`}><Trash2 size={14} />删除任务</Button>
          </div></details>
        </article>)}
      </section>
      <p className="management-automation-runtime-note"><Info size={16} aria-hidden="true" />定时任务需要应用保持运行。运行记录可在任务的“运行历史”中查看。</p>
    </div>
    {editing !== undefined && <AutomationEditor value={editing} graphs={graphs} workspaces={workspaces} onClose={() => setEditing(undefined)} onSaved={() => { setEditing(undefined); reload(); }} />}
    {history && <AutomationHistory automation={history} onClose={() => setHistory(null)} />}
    <ConfirmDialog open={Boolean(deleting)} title="删除定时任务" description={`删除“${deleting?.name ?? ""}”的执行计划？已有执行记录仍保留。`} tone="danger" confirmLabel="删除" onCancel={() => setDeleting(null)} onConfirm={async () => { if (!deleting) return; const item = deleting; setDeleting(null); await action(item, async () => { await deleteAutomation(item.id); }); }} />
  </PageShell>;
}
