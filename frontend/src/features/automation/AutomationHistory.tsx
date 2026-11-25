import { assistantErrorText } from "./assistantErrorText";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Badge, Button, EmptyState, Modal } from "../../components/ui";
import WorkflowRunInspector from "../../components/workflow/WorkflowRunInspector";
import { getAutomationHistory, type Automation, type AutomationOccurrence } from "../../api/automations";
import { automationStatusLabels, automationTriggerLabels } from "./automationDisplay";

export { automationStatusLabels } from "./automationDisplay";
export default function AutomationHistory({ automation, onClose }: { automation: Automation; onClose: () => void }) {
  const navigate = useNavigate();
  const [items, setItems] = useState<AutomationOccurrence[]>([]);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [version, setVersion] = useState(0);
  const [runId, setRunId] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    async function load() {
      try {
        const result = await getAutomationHistory(automation.id);
        if (!active) return;
        setItems(result.occurrences); setError("");
      } catch (cause) { if (active) setError(cause instanceof Error ? cause.message : String(cause)); }
      finally { if (active) { setLoading(false); timer = setTimeout(() => void load(), 2500); } }
    }
    void load();
    return () => { active = false; clearTimeout(timer); };
  }, [automation.id, version]);
  return <Modal open onClose={onClose} title={`运行历史 · ${automation.name}`} size="xl" footer={<Button variant="secondary" onClick={onClose}>关闭</Button>}>
    <div className="space-y-4">
      <div className="flex justify-end"><Button size="sm" variant="secondary" onClick={() => setVersion(current => current + 1)}>刷新历史</Button></div>
      {error && <p role="alert" className="text-sm text-[var(--danger)]">{error}</p>}
      {loading ? <p role="status">正在读取运行历史…</p> : !items.length && !error ? <EmptyState title="尚无运行记录" description="计划触发或手动运行后，记录会显示在这里。" /> : <div className="space-y-2">{items.map(item => <article key={item.id} className="glass-card rounded-xl border border-[var(--border-soft)] p-3 space-y-2"><div className="flex flex-wrap items-center gap-2"><Badge tone={item.status === "completed" ? "success" : item.status === "failed" ? "danger" : item.status === "waiting_approval" ? "warning" : "default"}>{automationStatusLabels[item.status] ?? item.status}</Badge><span className="text-sm">{automationTriggerLabels[item.trigger]}</span><time className="text-xs text-[var(--text-secondary)]">{new Date(item.scheduled_at).toLocaleString("zh-CN", { timeZone: item.timezone })} · {item.timezone} · 版本 {item.automation_revision}</time>{item.robot_run_id && item.robot_channel_id && <Button size="sm" variant="secondary" className="ml-auto" onClick={() => navigate(`/software/channels?channel=${encodeURIComponent(item.robot_channel_id!)}&run=${encodeURIComponent(item.robot_run_id!)}`)}>查看机器人运行</Button>}{item.conversation_id && !item.robot_run_id && <Button size="sm" variant="secondary" onClick={() => navigate(`/chat/${encodeURIComponent(item.conversation_id!)}`)}>查看任务对话与确认事项</Button>}{item.run_id && item.status !== "dispatching" && <Button size="sm" variant="secondary" className="ml-auto" onClick={() => setRunId(item.run_id)}>查看运行与结果</Button>}</div>{item.error && <p className="text-xs text-[var(--text-secondary)] break-words">{assistantErrorText(item.error)}</p>}{item.report && <p className="whitespace-pre-wrap break-words text-sm">{item.report}</p>}</article>)}</div>}
      {runId && <WorkflowRunInspector key={runId} runId={runId} onClose={() => setRunId(null)} onOpenGraph={id => navigate(`/capabilities?category=workflow&workflow=${encodeURIComponent(id)}`)} />}
    </div>
  </Modal>;
}
