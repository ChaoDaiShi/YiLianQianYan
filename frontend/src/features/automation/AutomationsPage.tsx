import { assistantErrorText } from "./assistantErrorText";
import WorkspaceIcon from "../../components/ui/WorkspaceIcon";
import { useCallback, useEffect, useRef, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { Archive, Clock3, History, MoreHorizontal, Pencil, Play, Plus, RefreshCw, RotateCcw, Trash2 } from "lucide-react";
import PageShell from "../../components/layout/PageShell";
import { Badge, Button, ConfirmDialog, EmptyState, Input, showToast } from "../../components/ui";
import { archivePersonalAssistant, restorePersonalAssistant, listAutomations, deleteAutomation, runAutomation, setAutomationEnabled, stopAutomation, type Automation, type AutomationRuntime } from "../../api/automations";
import { subscribeToEvents } from "../../api/events";
import { listWorkflowGraphs, type WorkflowGraphRecord } from "../../api/workflows";
import { listWorkspaces, type Workspace } from "../../api/workspaces";
import AutomationEditor, { weekdays } from "./AutomationEditor";
import AutomationHistory, { automationStatusLabels } from "./AutomationHistory";
import { automationTriggerLabels } from "./automationDisplay";
import PersonalAssistantDetail from "./PersonalAssistantDetail";
import { automationManagementPolicy, matchesAutomation, type AutomationFilter } from "./automationManagementPolicy";

function scheduleLabel(automation: Automation) {
  const schedule = automation.schedule;
  if (schedule.kind === "file_changed") return "本地文件内容变化";
  if (schedule.kind === "watch") return schedule.source.kind === "file" ? "主动关注本地文件" : "主动关注网址";
  if (schedule.kind === "once") return `仅一次 · ${schedule.local_at.replace("T", " ")}`;
  if (schedule.kind === "daily") return `每天 ${schedule.time}`;
  return `每周${schedule.weekdays.map(day => weekdays[day - 1]).join("、")} ${schedule.time}`;
}

const filters: { id: AutomationFilter; label: string }[] = [
  { id: "all", label: "全部" }, { id: "enabled", label: "已启用" }, { id: "paused", label: "已暂停" },
  { id: "attention", label: "需要处理" }, { id: "archived", label: "已归档" },
];

const phases = {
  suspended: { label: "挂起", tone: "default" },
  waking: { label: "唤醒", tone: "info" },
  running: { label: "运行", tone: "running" },
  waiting_approval: { label: "等待审批", tone: "warning" },
  waiting_input: { label: "等待回答", tone: "warning" },
} as const satisfies Record<AutomationRuntime["phase"], { label: string; tone: string }>;

const triggerLabels: Record<string, string> = automationTriggerLabels;

export default function AutomationsPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const assistantId = searchParams.get("assistant");
  const [filter, setFilter] = useState<AutomationFilter>("all");
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<Automation[]>([]);
  const [graphs, setGraphs] = useState<WorkflowGraphRecord[]>([]);
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [loading, setLoading] = useState(true);
  const [hasLoaded, setHasLoaded] = useState(false);
  const [loadError, setLoadError] = useState("");
  const [auxiliaryError, setAuxiliaryError] = useState("");
  const [actionError, setActionError] = useState("");
  const [editing, setEditing] = useState<Automation | null | undefined>(undefined);
  const [history, setHistory] = useState<Automation | null>(null);
  const [detail, setDetail] = useState<{ id: string; tab?: "history" } | null>(assistantId ? { id: assistantId } : null);
  useEffect(() => { if (assistantId) { setEditing(undefined); setHistory(null); setDetail({ id: assistantId }); } }, [assistantId]);
  function closeDetail() { setDetail(null); if (assistantId) { const next = new URLSearchParams(searchParams); next.delete("assistant"); setSearchParams(next, { replace: true }); } }
  const [removing, setRemoving] = useState<Automation | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const busyRef = useRef<string | null>(null);
  const [refreshVersion, setRefreshVersion] = useState(0);
  const reload = useCallback(() => setRefreshVersion(current => current + 1), []);
  useEffect(() => {
    const connection = subscribeToEvents(event => {
      if (event.type === "automation.runtime.updated") {
        const { automation_id, phase, status, last_woke_at, trigger } = event.payload;
        if (typeof automation_id === "string" && typeof phase === "string" && Object.prototype.hasOwnProperty.call(phases, phase)
            && typeof status === "string" && typeof last_woke_at === "number" && typeof trigger === "string") {
          setItems(items => items.map(item => item.id === automation_id && (item.runtime?.last_woke_at ?? 0) <= last_woke_at
            ? { ...item, runtime: { phase: phase as AutomationRuntime["phase"], latest_status: status, last_woke_at, trigger } }
            : item));
        }
        reload();
      } else if (event.type === "automation.personal.updated" || event.type === "automation.attention.updated") reload();
    }, undefined, state => { if (state === "connected") reload(); });
    return () => connection.abort();
  }, [reload]);
  useEffect(() => {
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    async function load() {
      const responses = await Promise.allSettled([listAutomations(), listWorkflowGraphs(), listWorkspaces()]);
      if (!active) return;
      const failures: string[] = [];
      const [automations, workflowGraphs, workspaceList] = responses;
      if (automations.status === "fulfilled") {
        setItems(automations.value.automations); setHasLoaded(true); setLoadError("");
      } else setLoadError(String(automations.reason));
      if (workflowGraphs.status === "fulfilled" && workflowGraphs.value.ok) setGraphs(workflowGraphs.value.data);
      else failures.push(`工作流读取失败：${workflowGraphs.status === "rejected" ? String(workflowGraphs.reason) : workflowGraphs.value.ok ? "" : workflowGraphs.value.error}`);
      if (workspaceList.status === "fulfilled" && workspaceList.value.ok) setWorkspaces(workspaceList.value.data);
      else failures.push(`工作空间读取失败：${workspaceList.status === "rejected" ? String(workspaceList.reason) : workspaceList.value.ok ? "" : workspaceList.value.error}`);
      setAuxiliaryError(failures.join("；")); setLoading(false);
      // Auxiliary failures must never freeze task state. Keep retrying every source.
      const executing = automations.status === "fulfilled" && automations.value.automations.some(item => item.runtime && item.runtime.phase !== "suspended");
      timer = setTimeout(() => void load(), executing ? 3000 : 10000);
    }
    void load();
    return () => { active = false; clearTimeout(timer); };
  }, [refreshVersion]);

  async function action(item: Automation, run: () => Promise<void>) {
    if (busyRef.current !== null) return;
    busyRef.current = item.id; setBusyId(item.id); setActionError("");
    try { await run(); }
    catch (cause) { setActionError(cause instanceof Error ? cause.message : String(cause)); }
    finally { busyRef.current = null; setBusyId(null); reload(); }
  }
  function openDetail(id: string, tab?: "history") { setEditing(undefined); setHistory(null); setDetail({ id, tab }); }
  function openEditor(item: Automation | null) { setDetail(null); setHistory(null); setEditing(item); }
  const visibleItems = items.filter(item => matchesAutomation(item, filter, query));
  const error = [actionError, loadError, auxiliaryError].filter(Boolean).join("；");
  const archivedRemoval = removing?.target?.kind === "personal_assistant";
  return <PageShell title="定时任务" variant="workspace" className="page-canvas management-automations-page" contentClassName="management-automations-content" actions={<><Button size="sm" variant="secondary" onClick={reload} aria-label="刷新定时任务"><RefreshCw size={15} />刷新</Button><Button size="sm" onClick={() => openEditor(null)}><Plus size={15} />新建任务</Button></>}>
    <div className="management-automations-workspace">
      <div className="flex shrink-0 flex-wrap items-end gap-3 min-w-0">
        <div className="min-w-0 w-full sm:max-w-sm"><Input label="搜索任务" value={query} onChange={event => setQuery(event.target.value)} placeholder="搜索名称或个人目标" /></div>
        <div className="management-status-tabs min-w-0" aria-label="定时任务状态筛选">
          {filters.map(tab => <button type="button" key={tab.id} aria-pressed={filter === tab.id} onClick={() => setFilter(tab.id)}>{tab.label}</button>)}
        </div>
      </div>
      {error && <div role="alert" className="shrink-0 text-sm text-[var(--danger)] break-words"><p>{error}</p>{loadError && hasLoaded && <p className="text-xs text-[var(--text-secondary)]">当前显示上次成功读取的数据，请勿将其视为最新状态。操作仍由服务端重新校验。</p>}<Button size="sm" variant="secondary" onClick={reload}>重试读取</Button></div>}
      <section className="management-automation-list scrollbar-thin" aria-label="定时任务列表">
        {loading ? <p role="status" className="p-5">正在读取定时任务…</p> : !hasLoaded ? null : items.length === 0 ? <EmptyState icon={<WorkspaceIcon name="task-calendar" size={30} />} title="安排第一次自动执行" description="选择个人目标或工作流，以及工作空间和执行时间。" /> : visibleItems.length === 0 ? <EmptyState title={query.trim() ? "没有匹配的任务" : filter === "archived" ? "暂无归档委托" : filter === "attention" ? "暂无需要处理的任务" : "当前筛选下没有任务"} description={filter === "archived" ? "归档保留历史与文件，恢复后仍为暂停状态。" : "调整关键词或筛选条件后查看。"} action={<Button variant="secondary" onClick={() => { setFilter("all"); setQuery(""); }}>查看全部任务</Button>} /> : visibleItems.map(item => {
          const policy = automationManagementPolicy(item);
          const phase = item.runtime?.phase ?? "suspended";
          const phaseView = phases[phase];
          const phaseLabel = item.archived ? "已归档" : phase === "suspended" && !item.enabled ? "已暂停" : phaseView.label;
          return <article key={item.id} className="management-automation-row">
            <span className="management-automation-icon" aria-hidden="true"><WorkspaceIcon name="task-calendar" size={24} /></span>
            <div className="management-automation-name"><h2>{item.name}</h2><p>{item.target?.kind === "personal_assistant" ? `个人主动助手 · ${item.target.goal}` : item.target?.kind === "agent_turn" ? "QQ 机器人目标任务" : graphs.find(graph => graph.id === item.workflow_graph_id)?.name ?? item.workflow_graph_id}</p><small>工作空间：{workspaces.find(workspace => workspace.id === item.workspace_id)?.name ?? item.workspace_id}</small>{item.target?.kind === "personal_assistant" && <><small>已用 {item.assistant_activity?.used_runs ?? 0} / {item.target.max_runs} 轮 · {automationStatusLabels[item.assistant_activity?.latest?.status ?? ""] ?? "等待首次运行"}</small><div className="mt-2 flex flex-wrap items-center gap-2"><Button size="sm" variant="secondary" onClick={() => openDetail(item.id)}>任务详情</Button><Link className="text-xs" to={`/chat/${encodeURIComponent(item.target.conversation_id ?? "")}`}>{policy.waiting ? "处理审批或回答" : "任务对话"}</Link>{policy.canStop && <Button size="sm" variant="ghost" disabled={busyId !== null} title="暂停后续计划并停止当前轮次；已完成的文件操作不会回滚" onClick={() => void action(item, async () => { await stopAutomation(item.id); })}>停止委托</Button>}</div>{item.archived && <small>归档中：记录和文件保留，恢复后仍暂停。</small>}{policy.exhausted && <small role="status">预算已用完；检查结果后可编辑总轮次，最多 20 轮。</small>}{item.assistant_activity?.source_error && <small role="status">文件监听暂停：{item.assistant_activity.source_error}</small>}{item.assistant_activity?.latest?.error && <small role="status">{assistantErrorText(item.assistant_activity.latest.error)}</small>}</>}</div>
            <div className="management-automation-time"><Clock3 size={18} aria-hidden="true" /><div><p>{scheduleLabel(item)} · {item.timezone === "Asia/Shanghai" ? "上海时间" : item.timezone}</p><small>{item.schedule.kind === "watch" ? "下次复查" : "下次执行"}：{!item.archived && item.next_run_at ? new Date(item.next_run_at).toLocaleString("zh-CN", { timeZone: item.timezone }) : !item.archived && item.enabled ? (item.schedule.kind === "file_changed" ? "等待文件内容变化" : item.attention?.status === "checking" ? "复查中" : "—") : "—"}</small></div></div>
            <div className="automation-runtime-status" role="status" aria-label="任务运行状态" data-automation-phase={phase}>
              <Badge tone={item.archived ? "default" : phaseView.tone}>{phaseLabel}</Badge>
              {policy.attentionBadge && !item.archived && <div className="mt-1"><Badge tone="warning">{policy.attentionBadge}</Badge></div>}
              {item.runtime?.last_woke_at != null && <p className="mt-1 text-xs text-[var(--text-secondary)]" title={new Date(item.runtime.last_woke_at).toLocaleString("zh-CN", { timeZone: item.timezone })}>
                最近唤醒{item.runtime.trigger ? ` · ${triggerLabels[item.runtime.trigger] ?? item.runtime.trigger}` : ""}
              </p>}
            </div>
            <button type="button" className="management-automation-switch" role="switch" aria-checked={item.enabled && !item.archived} aria-label={`${item.enabled ? "暂停" : "启用"} ${item.name}`} title={policy.scheduleReason} disabled={busyId !== null || !policy.canToggleSchedule} onClick={() => void action(item, async () => { await setAutomationEnabled(item.id, !item.enabled, item.revision); })}><span /></button>
            <details className="management-automation-menu"><summary aria-label={`更多操作 ${item.name}`}><MoreHorizontal size={19} /></summary><div>
              {item.archived ? <Button size="sm" variant="ghost" disabled={busyId !== null} onClick={() => void action(item, async () => { await restorePersonalAssistant(item.id, item.revision); showToast("已恢复为暂停状态，请检查配置后再启用计划或运行。", "success"); })}><RotateCcw size={14} />恢复归档</Button> : <>
                {!policy.personal && <Button size="sm" variant="ghost" disabled={busyId !== null} onClick={() => void action(item, async () => { const result = await runAutomation(item.id); setHistory(item); if (["failed", "blocked"].includes(result.occurrence.status)) showToast(result.occurrence.error ?? "任务启动失败", "error"); })}><Play size={14} />立即运行</Button>}
                {!policy.personal && policy.canStop && <Button size="sm" variant="ghost" disabled={busyId !== null} onClick={() => void action(item, async () => { await stopAutomation(item.id); })}>停止任务</Button>}
                <Button size="sm" variant="ghost" disabled={busyId !== null || !policy.canEdit} title={policy.editReason} onClick={() => openEditor(item)} aria-label={`编辑 ${item.name}`}><Pencil size={14} />编辑任务</Button>
                <Button size="sm" variant="ghost" disabled={busyId !== null} onClick={() => setRemoving(item)} aria-label={`${policy.personal ? "归档" : "删除"} ${item.name}`}>{policy.personal ? <Archive size={14} /> : <Trash2 size={14} />}{policy.personal ? "归档委托" : "删除任务"}</Button>
              </>}
              <Button size="sm" variant="ghost" onClick={() => { if (policy.personal) openDetail(item.id, "history"); else { setDetail(null); setHistory(item); } }}><History size={14} />运行历史</Button>
            </div></details>
          </article>;
        })}
      </section>
      <details className="management-automation-runtime-note"><summary>任务状态说明</summary><p>挂起时等待触发，唤醒时准备执行。暂停只停止后续计划。停止任务还会取消当前执行。应用重启后会继续等待未到期的计划。</p></details>
    </div>
    {editing !== undefined && <AutomationEditor value={editing} graphs={graphs} workspaces={workspaces} onClose={() => setEditing(undefined)} onSaved={() => { setEditing(undefined); reload(); }} />}
    {history && <AutomationHistory automation={history} onClose={() => setHistory(null)} />}
    {detail && <PersonalAssistantDetail automationId={detail.id} initialTab={detail.tab} onClose={closeDetail} onChanged={reload} />}
    <ConfirmDialog open={Boolean(removing)} title={archivedRemoval ? "归档个人委托" : "删除定时任务"} description={archivedRemoval ? `归档“${removing?.name ?? ""}”将停止当前轮次和未来计划。历史、对话和已生成文件保留，可从“已归档”筛选查看；恢复后仍为暂停状态。` : `删除“${removing?.name ?? ""}”的执行计划？已有执行记录仍保留。`} tone="danger" confirmLabel={archivedRemoval ? "归档" : "删除"} onCancel={() => { if (busyRef.current === null) setRemoving(null); }} onConfirm={async () => { if (!removing || busyRef.current !== null) return; const item = removing; setRemoving(null); await action(item, async () => { if (item.target?.kind === "personal_assistant") await archivePersonalAssistant(item.id); else await deleteAutomation(item.id); }); }} />
  </PageShell>;
}
