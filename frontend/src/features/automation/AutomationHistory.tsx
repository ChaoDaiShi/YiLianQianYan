import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Badge, Button, EmptyState, Modal } from "../../components/ui";
import WorkflowRunInspector from "../../components/workflow/WorkflowRunInspector";
import { getAutomationHistory, type Automation, type AutomationOccurrence } from "../../api/automations";

const labels: Record<string, string> = { dispatching: "正在启动", created: "已创建", running: "运行中", waiting_approval: "等待审批", awaiting_approval: "等待审阅", queued: "等待运行", stopped: "已停止", completed: "已完成", failed: "失败", cancelled: "已取消", interrupted: "已中断", skipped: "已跳过" };
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
        timer = setTimeout(() => void load(), 2500);
      } catch (cause) { if (active) setError(cause instanceof Error ? cause.message : String(cause)); }
      finally { if (active) setLoading(false); }
    }
    void load();
    return () => { active = false; clearTimeout(timer); };
  }, [automation.id, version]);
  return <Modal open onClose={onClose} title={`运行历史 · ${automation.name}`} size="xl" footer={<Button variant="secondary" onClick={onClose}>关闭</Button>}>
    <div className="space-y-4">
      <div className="flex justify-end"><Button size="sm" variant="secondary" onClick={() => setVersion(current => current + 1)}>刷新历史</Button></div>
      {error && <p role="alert" className="text-sm text-[var(--danger)]">{error}</p>}
      {loading ? <p role="status">正在读取运行历史…</p> : !items.length ? <EmptyState title="尚无运行记录" description="计划触发或手动运行后，记录会显示在这里。" /> : <div className="space-y-2">{items.map(item => <article key={item.id} className="rounded-xl border border-[var(--border-soft)] p-3 space-y-2"><div className="flex flex-wrap items-center gap-2"><Badge tone={item.status === "completed" ? "success" : item.status === "failed" ? "danger" : item.status === "waiting_approval" ? "warning" : "default"}>{labels[item.status] ?? item.status}</Badge><span className="text-sm">{item.trigger === "manual" ? "手动运行" : "计划触发"}</span><time className="text-xs text-[var(--text-secondary)]">{new Date(item.scheduled_at).toLocaleString("zh-CN", { timeZone: item.timezone })} · {item.timezone} · 版本 {item.automation_revision}</time>{item.robot_run_id && item.robot_channel_id && <Button size="sm" variant="secondary" className="ml-auto" onClick={() => navigate(`/channels?channel=${encodeURIComponent(item.robot_channel_id!)}&run=${encodeURIComponent(item.robot_run_id!)}`)}>查看机器人运行</Button>}{item.run_id && item.status !== "dispatching" && <Button size="sm" variant="secondary" className="ml-auto" onClick={() => setRunId(item.run_id)}>查看运行与结果</Button>}</div>{item.error && <p className="text-xs text-[var(--text-secondary)]">{item.error}</p>}</article>)}</div>}
      {runId && <WorkflowRunInspector key={runId} runId={runId} onClose={() => setRunId(null)} onOpenGraph={id => navigate(`/capabilities?category=workflow&workflow=${encodeURIComponent(id)}`)} />}
    </div>
  </Modal>;
}
