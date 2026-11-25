import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  getPersonalManagement, previewPersonalFile, restorePersonalAssistant, setAutomationEnabled,
  startPersonalAssistant, stopPersonalAssistant, type PersonalManagement, type PersonalStartInput,
} from "../../api/automations";
import { Badge, Button, EmptyState, Modal } from "../../components/ui";
import { assistantErrorText } from "./assistantErrorText";
import {
  automationStatusLabels, occurrenceTime, personalStateGuidance,
  restorePendingPersonalStart, savePendingPersonalStart,
} from "./automationDisplay";
import PersonalHistoryContent from "./PersonalHistoryContent";
import { AttentionLog, AttentionSummary } from "./AttentionPanel";
import { taskWorkflowDestination } from "../workflows/taskGraphRoutes";

type DetailTab = "overview" | "execution" | "results" | "history" | "observations";
const tabs: { id: DetailTab; label: string }[] = [
  { id: "overview", label: "概览与操作" }, { id: "execution", label: "执行过程" },
  { id: "results", label: "报告与文件" }, { id: "history", label: "全部历史" },
];
const executionLabels: Record<string, string> = {
  succeeded: "已完成", queued: "待执行", running: "执行中", completed: "已完成", success: "已完成", failed: "失败", error: "失败",
  awaiting_approval: "等待审批", cancelled: "已取消", pending: "待确认", approved: "已批准", rejected: "已拒绝",
  passed: "核验通过", verified: "核验通过", not_requested: "未请求核验", skipped: "已跳过",
  not_required: "无需审批", not_applicable: "不适用", allowed: "已允许", denied: "已拒绝", uncertain: "结果不确定",
};
const text = (status: string) => executionLabels[status] || automationStatusLabels[status] || status;

export default function PersonalAssistantDetail({ automationId, initialTab = "overview", onClose, onChanged }: {
  automationId: string; initialTab?: DetailTab; onClose: () => void; onChanged: () => void;
}) {
  const navigate = useNavigate();
  const [data, setData] = useState<PersonalManagement | null>(null);
  const [tab, setTab] = useState(initialTab);
  const [historyVisited, setHistoryVisited] = useState(initialTab === "history");
  const [loading, setLoading] = useState(true);
  const [readError, setReadError] = useState("");
  const [actionError, setActionError] = useState("");
  const [busy, setBusy] = useState("");
  const [acknowledged, setAcknowledged] = useState(false);
  const [updatedAt, setUpdatedAt] = useState<number | null>(null);
  const [pendingStart, setPendingStart] = useState<PersonalStartInput | null>(() => restorePendingPersonalStart(automationId));
  const [preview, setPreview] = useState<{ toolCallId: string; path: string; content: string; size: number; truncated: boolean } | null>(null);
  const [previewError, setPreviewError] = useState("");
  const [previewLoading, setPreviewLoading] = useState<string | null>(null);
  const mounted = useRef(false);
  const targetEpoch = useRef(0);
  const requestSequence = useRef(0);
  const previewSequence = useRef(0);
  const busyRef = useRef(false);
  const reading = useRef(false);

  const load = useCallback(async (force = false) => {
    if ((busyRef.current && !force) || (reading.current && !force)) return;
    const sequence = ++requestSequence.current;
    const epoch = targetEpoch.current;
    reading.current = true;
    try {
      const value = await getPersonalManagement(automationId);
      if (!mounted.current || sequence !== requestSequence.current || epoch !== targetEpoch.current) return;
      setData(value); setReadError(""); setUpdatedAt(Date.now());
    } catch (cause) {
      if (mounted.current && sequence === requestSequence.current && epoch === targetEpoch.current) setReadError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      if (mounted.current && sequence === requestSequence.current && epoch === targetEpoch.current) { setLoading(false); reading.current = false; }
    }
  }, [automationId]);

  useEffect(() => {
    mounted.current = true; targetEpoch.current += 1; requestSequence.current += 1;
    reading.current = false; busyRef.current = false;
    setData(null); setLoading(true); setReadError(""); setActionError(""); setBusy(""); setAcknowledged(false);
    setTab(initialTab); setHistoryVisited(initialTab === "history"); setPreview(null); setPreviewError(""); setPreviewLoading(null);
    setPendingStart(restorePendingPersonalStart(automationId));
    void load();
    const timer = setInterval(() => void load(), 3000);
    return () => { mounted.current = false; targetEpoch.current += 1; requestSequence.current += 1; previewSequence.current += 1; clearInterval(timer); };
  }, [automationId, initialTab, load]);

  const reviewId = data?.actions.review_occurrence_id;
  useEffect(() => { setAcknowledged(false); }, [reviewId, data?.automation.revision]);

  async function action(label: string, task: () => Promise<unknown>) {
    if (busyRef.current) return;
    const epoch = targetEpoch.current;
    busyRef.current = true; requestSequence.current += 1; reading.current = false;
    setBusy(label); setActionError("");
    try {
      await task();
      if (!mounted.current || targetEpoch.current !== epoch) return;
      onChanged();
      await load(true);
    } catch (cause) {
      if (mounted.current && targetEpoch.current === epoch) setActionError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      if (mounted.current && targetEpoch.current === epoch) { busyRef.current = false; setBusy(""); }
    }
  }

  async function start(retry = false) {
    if (!data || busyRef.current) return;
    let input = retry ? pendingStart : null;
    if (!input) {
      if (!data.actions.can_start || !data.readiness.can_start || (data.actions.requires_review && (!acknowledged || !data.actions.review_occurrence_id))) return;
      input = {
        expected_revision: data.automation.revision, request_id: crypto.randomUUID(),
        ...(data.actions.requires_review && data.actions.review_occurrence_id ? {
          reviewed_occurrence_id: data.actions.review_occurrence_id, acknowledged: true,
        } : {}),
      };
      setPendingStart(input); savePendingPersonalStart(automationId, input);
    }
    const request = input;
    const epoch = targetEpoch.current;
    await action(retry ? "正在核对启动请求" : "正在启动", async () => {
      // Keep the exact payload until a response confirms the idempotent outcome.
      // Even a returned completed occurrence can be the earlier request's result.
      try { await startPersonalAssistant(automationId, request); }
      catch (cause) {
        // This flag is reserved for server responses that prove no occurrence
        // was accepted. Transport/5xx ambiguity keeps the exact request intact.
        if (cause && typeof cause === "object" && "definitiveRejected" in cause && cause.definitiveRejected === true) {
          savePendingPersonalStart(automationId, null);
          if (mounted.current && targetEpoch.current === epoch) setPendingStart(null);
        }
        throw cause;
      }
      savePendingPersonalStart(automationId, null);
      if (mounted.current && targetEpoch.current === epoch) { setPendingStart(null); setAcknowledged(false); }
    });
  }

  async function openFile(toolCallId: string) {
    const sequence = ++previewSequence.current;
    setPreview(null); setPreviewError(""); setPreviewLoading(toolCallId);
    try {
      const file = await previewPersonalFile(automationId, toolCallId);
      if (mounted.current && sequence === previewSequence.current) setPreview({ ...file, toolCallId });
    } catch (cause) {
      if (mounted.current && sequence === previewSequence.current) setPreviewError(cause instanceof Error ? cause.message : String(cause));
    } finally { if (mounted.current && sequence === previewSequence.current) setPreviewLoading(null); }
  }

  const go = (path: string) => { onClose(); navigate(path); };
  const personal = data?.automation.target?.kind === "personal_assistant" ? data.automation.target : null;
  const latest = data?.activity.latest;
  const stale = Boolean(readError);
  const mayMutate = Boolean(data) && !busy && !stale;
  const budgetExhausted = personal ? (data?.activity.used_runs || 0) >= personal.max_runs : false;
  const waiting = Boolean(data?.pending_inputs.length || data?.pending_approvals.length
    || latest?.status === "waiting_input" || latest?.status === "waiting_approval");
  const canStart = Boolean(mayMutate && data?.actions.can_start && data.readiness.can_start
    && (!data.actions.requires_review || (acknowledged && data.actions.review_occurrence_id)) && !pendingStart);
  const observes = data?.automation.schedule.kind === "watch" || data?.automation.schedule.kind === "file_changed";
  const visibleTabs = observes ? [...tabs, { id: "observations" as const, label: "观察日志" }] : tabs;

  return <Modal open size="xl" title={`个人助手详情${data ? ` · ${data.automation.name}` : ""}`} onClose={onClose}
    footer={<Button variant="secondary" onClick={onClose}>{tab === "history" ? "关闭" : "关闭详情"}</Button>}>
    <div className="min-w-0 space-y-4 text-sm [overflow-wrap:anywhere]">
      <div className="flex flex-wrap items-center justify-between gap-2"><p className="text-xs text-[var(--text-secondary)]">每三秒更新。关闭详情不会取消执行。{updatedAt && ` 上次读取：${occurrenceTime(updatedAt)}`}</p><Button variant="secondary" size="sm" disabled={Boolean(busy)} onClick={() => void load(true)}>刷新详情</Button></div>
      {readError && <div role="alert" className="space-y-2 text-[var(--danger)]"><p>读取详情失败：{assistantErrorText(readError)}{data ? "。当前显示上次读取的内容，刷新成功前不会发起新的执行或配置操作；仍可停止并暂停委托。" : "。尚未取得状态，不能启动或修改任务；仍可发送停止并暂停请求。"}</p><div className="flex flex-wrap gap-2"><Button size="sm" variant="secondary" disabled={Boolean(busy)} onClick={() => void load(true)}>重试读取详情</Button><Button size="sm" variant="danger" disabled={Boolean(busy)} loading={busy === "正在停止委托"} onClick={() => void action("正在停止委托", () => stopPersonalAssistant(automationId))}>停止并暂停委托</Button></div></div>}
      {actionError && <p role="alert" className="text-[var(--danger)]">操作未完成：{assistantErrorText(actionError)}</p>}
      {loading && !data ? <p role="status">正在读取个人委托…</p> : data && personal ? <>
        <div className="flex flex-wrap items-center gap-2"><Badge tone={data.automation.archived ? "default" : latest?.status === "completed" ? "success" : waiting ? "warning" : latest?.status === "failed" ? "danger" : "default"}>{data.automation.archived ? "已归档" : automationStatusLabels[latest?.status || ""] || "等待首次运行"}</Badge><span>{data.automation.enabled ? "后续触发已启用" : "后续触发已暂停"}</span><span>已用 {data.activity.used_runs} / {personal.max_runs} 轮</span></div>
        <div className="flex flex-wrap gap-2 border-b border-[var(--border-soft)] pb-3" role="tablist" aria-label="个人助手详情内容">{visibleTabs.map(item => <button key={item.id} type="button" role="tab" aria-selected={tab === item.id} onClick={() => { setTab(item.id); if (item.id === "history") setHistoryVisited(true); }} className={`rounded-lg border px-3 py-2 text-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent-primary)] ${tab === item.id ? "border-[var(--accent-primary)] bg-[var(--accent-soft)] text-[var(--accent-primary)]" : "border-[var(--border-soft)]"}`}>{item.label}</button>)}</div>
        {tab === "overview" && <section role="tabpanel" aria-label="概览与操作" className="space-y-4">
          <div className="space-y-2"><h4 className="font-medium">持续目标</h4><p className="whitespace-pre-wrap">{personal.goal}</p><p><span className="text-[var(--text-secondary)]">授权目录：</span>{personal.root_path || "未保存目录"}</p><p className="text-xs text-[var(--text-secondary)]">只读文件、待办与提问；目录查找：{personal.allow_file_discovery ? "已明确允许" : "未允许"}；创建新文件：{personal.allow_file_writes ? "已明确允许（不覆盖）" : "未允许"}。不提供命令、账号、网络或外发工具。</p><p className="text-xs text-[var(--text-secondary)]">每个连续执行段仍最多六次模型调用、24,000 token、三分钟。当前配置版本 {data.automation.revision}。</p></div>
          <p className="rounded-lg border border-[var(--border-soft)] p-3" role="status">{personalStateGuidance(latest?.status, Boolean(data.automation.archived), budgetExhausted)}</p>
          <AttentionSummary automation={data.automation} onOpenLog={() => setTab("observations")} onSelectGoal={proposal => {
            const query = new URLSearchParams({ mode: "goal", workspace: data.automation.workspace_id });
            onClose();
            navigate(taskWorkflowDestination(query.toString()), {
              state: { attentionGoal: {
                goal: proposal.goal,
                source: { automation_id: automationId, observation_id: proposal.observation_id },
              } },
            });
          }} />
          {data.activity.source_error && <p role="status" className="text-[var(--danger)]">文件监听暂停：{data.activity.source_error}</p>}
          {(latest?.error || data.conversation.run_error) && <p className="text-[var(--danger)]">本轮原因：{assistantErrorText(latest?.error || data.conversation.run_error || "")}</p>}
          <section className="space-y-2" aria-label="运行准备检查"><h4 className="font-medium">运行准备检查</h4>{data.readiness.issues.length ? <ul className="space-y-2">{data.readiness.issues.map((issue, index) => <li key={`${issue.code}:${index}`} className={issue.severity === "error" ? "text-[var(--danger)]" : "text-[var(--text-secondary)]"}><span className="font-medium">{issue.severity === "error" ? "需处理" : "提示"}：</span>{issue.message}</li>)}</ul> : <p className="text-[var(--text-secondary)]">当前检查未发现阻止启动的问题。实际工具操作仍受原有权限与审批约束。</p>}<p className="text-xs text-[var(--text-secondary)]">这里只读取配置，不测试收费模型，也不创建或扩大权限。</p><div className="flex flex-wrap gap-2"><Button size="sm" variant="secondary" onClick={() => go("/settings?section=model")}>模型设置</Button><Button size="sm" variant="secondary" onClick={() => go("/settings?section=permissions")}>权限与审批设置</Button></div></section>
          {waiting && <div className="space-y-2 rounded-lg border border-[var(--border-soft)] p-3"><h4 className="font-medium">等待你处理</h4>{data.pending_inputs.map(question => <p key={question.question_id}>待回答：{question.title}</p>)}{data.pending_approvals.map(operation => <p key={operation.tool_call_id}>待审批：{operation.name}{operation.path ? ` · ${operation.path}` : ""}</p>)}<Button size="sm" onClick={() => go(`/chat/${encodeURIComponent(data.conversation.id)}`)}>打开原对话处理等待项</Button></div>}
          {data.actions.requires_review && !data.automation.archived && <div className="space-y-2 rounded-lg border border-[var(--border-soft)] p-3"><p>待核对轮次：{data.actions.review_occurrence_id || "状态待更新"}。请先检查“执行过程”和“报告与文件”。</p><label className="flex items-start gap-2"><input className="mt-1 shrink-0" type="checkbox" checked={acknowledged} disabled={Boolean(busy) || stale} onChange={event => setAcknowledged(event.target.checked)} /><span>已核实本轮文件和执行记录，继续新一轮</span></label><p className="text-xs text-[var(--text-secondary)]">确认不会重放旧工具调用，也不会回滚已完成操作；原权限与剩余预算继续生效。</p></div>}
          {pendingStart && <div role="status" className="space-y-2 rounded-lg border border-[var(--border-soft)] p-3"><p>有一次启动请求尚未取得确认结果。可用同一请求核对，避免因响应丢失重复创建轮次。</p><Button size="sm" disabled={Boolean(busy)} loading={busy === "正在核对启动请求"} onClick={() => void start(true)}>核对上次启动请求</Button></div>}
          <div className="flex flex-wrap gap-2">
            {!data.automation.archived && <Button disabled={!canStart} loading={busy === "正在启动"} onClick={() => void start()}>{data.actions.requires_review ? "核实后继续新一轮" : "运行一轮"}</Button>}
            {!data.automation.archived && <Button variant="secondary" disabled={!mayMutate || (!data.automation.enabled && !data.actions.can_edit)} loading={busy === "正在修改后续触发"} onClick={() => void action("正在修改后续触发", () => setAutomationEnabled(automationId, !data.automation.enabled, data.automation.revision))}>{data.automation.enabled ? "暂停后续触发" : "恢复后续触发"}</Button>}
            {data.automation.archived && <Button variant="secondary" disabled={!mayMutate} loading={busy === "正在恢复归档"} onClick={() => void action("正在恢复归档", () => restorePersonalAssistant(automationId, data.automation.revision))}>恢复归档（保持暂停）</Button>}
            {!data.automation.archived && !stale && <Button variant="danger" disabled={Boolean(busy) || !data.actions.can_stop} loading={busy === "正在停止委托"} onClick={() => void action("正在停止委托", () => stopPersonalAssistant(automationId))}>停止委托</Button>}
            <Button variant="secondary" onClick={() => go(`/chat/${encodeURIComponent(data.conversation.id)}`)}>打开任务对话</Button>
          </div>
        </section>}
        {tab === "execution" && <section role="tabpanel" aria-label="执行过程" className="space-y-3"><p className="text-xs text-[var(--text-secondary)]">会话状态：{automationStatusLabels[data.conversation.run_status] || data.conversation.run_status}。以下为最近最多 50 条工具记录；更多上下文请打开任务对话。开始执行和完成核验分别展示，不把工具返回当作目标完成。</p>{!data.executions.length ? <EmptyState title="暂无工具执行记录" description="模型思考或仅文本回复不一定调用工具；不会虚构执行进度。" /> : data.executions.map(operation => <article key={operation.tool_call_id} className="glass-card min-w-0 space-y-2 rounded-xl border border-[var(--border-soft)] p-3"><div className="flex flex-wrap items-center gap-2"><h4 className="font-medium">{operation.name}</h4><Badge>{text(operation.execution_status)}</Badge><span className="text-xs">审批：{text(operation.approval_status)} · 核验：{text(operation.verification_status)}</span></div>{operation.path && <p>{operation.path}</p>}<p className="text-xs text-[var(--text-secondary)]">{operation.started_at ? occurrenceTime(operation.started_at) : "尚未开始"}{operation.finished_at ? ` → ${occurrenceTime(operation.finished_at)}` : ""}</p>{operation.arguments_preview && <pre className="whitespace-pre-wrap text-xs [overflow-wrap:anywhere] font-sans">{operation.arguments_preview}</pre>}{operation.verification_reason && <p className="text-xs text-[var(--text-secondary)]">核验说明：{operation.verification_reason}</p>}{operation.result && <details><summary className="cursor-pointer text-[var(--accent-primary)]">查看返回内容</summary><pre className="mt-2 whitespace-pre-wrap text-xs [overflow-wrap:anywhere] font-sans">{operation.result}</pre></details>}</article>)}</section>}
        {tab === "results" && <section role="tabpanel" aria-label="报告与文件" className="space-y-4"><section className="space-y-2"><h4 className="font-medium">当前轮次报告</h4>{latest?.report ? <p className="whitespace-pre-wrap">{latest.report}</p> : <p className="text-[var(--text-secondary)]">当前轮次没有保存报告正文；不会使用其他轮次的报告代替。可在“全部历史”查看各轮结果。</p>}</section><section className="space-y-2"><h4 className="font-medium">生成文件</h4><p className="text-xs text-[var(--text-secondary)]">文件来自真实执行记录，预览时重新检查当前文件、目录范围与读取权限。文件可能在执行后被修改；列表中的核验状态记录的是当时结果。</p>{!data.files.length ? <EmptyState title="暂无生成文件记录" description="只有实际文件创建操作才会出现在这里。已有文件或纯文本回复不会被当作新产物。" /> : <div className="space-y-2">{data.files.map(file => <article key={file.tool_call_id} className="glass-card flex min-w-0 flex-wrap items-center justify-between gap-2 rounded-lg border border-[var(--border-soft)] p-3"><div className="min-w-0 flex-1"><p>{file.relative_path || file.path}</p><p className="text-xs text-[var(--text-secondary)]">{text(file.execution_status)} · {text(file.verification_status)}{file.created_at ? ` · ${occurrenceTime(file.created_at)}` : ""}</p></div><Button size="sm" variant="secondary" disabled={Boolean(previewLoading)} loading={previewLoading === file.tool_call_id} onClick={() => void openFile(file.tool_call_id)}>预览文件</Button></article>)}</div>}</section>{previewError && <p role="alert" className="text-[var(--danger)]">文件预览失败：{assistantErrorText(previewError)}</p>}{previewLoading && <p role="status">正在核对并读取文件…</p>}{preview && <section className="glass-card space-y-2 rounded-lg border border-[var(--border-soft)] p-3" aria-label="生成文件只读预览"><div className="flex flex-wrap items-start justify-between gap-2"><div className="min-w-0 flex-1"><h4 className="font-medium">只读预览</h4><p>{preview.path}</p><p className="text-xs text-[var(--text-secondary)]">当前大小 {preview.size.toLocaleString()} 字节{preview.truncated ? "；内容已截断，未显示完整文件。" : ""}</p></div><Button size="sm" variant="ghost" onClick={() => { previewSequence.current += 1; setPreview(null); setPreviewError(""); }}>收起预览</Button></div>{preview.content ? <pre className="whitespace-pre-wrap text-xs [overflow-wrap:anywhere] font-sans">{preview.content}</pre> : <p role="status">文件内容为空。</p>}</section>}</section>}
        {tab === "observations" && observes && <AttentionLog automationId={automationId} timezone={data.automation.timezone} lastCheck={data.automation.attention?.last_checked_at} />}
      </> : !readError && !loading && tab !== "history" ? <EmptyState title="无法显示此个人委托" description="请关闭详情，刷新任务列表后再试。" /> : null}
      {historyVisited && <section role="tabpanel" aria-label="全部历史" hidden={tab !== "history"}><PersonalHistoryContent automationId={automationId} onNavigate={onClose} active={tab === "history"} /></section>}
    </div>
  </Modal>;
}
