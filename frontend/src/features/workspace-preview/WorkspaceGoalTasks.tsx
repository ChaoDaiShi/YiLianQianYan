import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { listProofDrafts, type ProofDraftSummary } from "../../api/proofloop";
import { Button, EmptyState, ErrorState, Select } from "../../components/ui";
import GoalTrackingPanel from "../task-world/goal-view/GoalTrackingPanel";
import { taskWorkflowDestination } from "../workflows/taskGraphRoutes";
import { confirmedGoalChoices } from "../task-world/goal-view/goalDraftChoices";

export default function WorkspaceGoalTasks({ workspaceId, initialGraphId = "", onSelectGraph }: { workspaceId: string; initialGraphId?: string; onSelectGraph?: (graphId: string) => void }) {
  const navigate = useNavigate();
  const [goals, setGoals] = useState<ProofDraftSummary[]>([]);
  const [selected, setSelected] = useState(initialGraphId);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const request = useRef(0);
  const reload = useCallback(async () => {
    const ticket = ++request.current;
    setLoading(true);
    const result = await listProofDrafts(workspaceId);
    if (ticket !== request.current) return;
    if (result.ok) {
      const next = confirmedGoalChoices(result.data.drafts);
      setGoals(next); setSelected(current => next.some(goal => goal.graph_id === current) ? current : next[0]?.graph_id ?? ""); setError("");
    } else setError(result.error.message);
    setLoading(false);
  }, [workspaceId]);
  useEffect(() => {
    void reload();
    const refresh = () => void reload();
    window.addEventListener("focus", refresh);
    return () => { request.current++; window.removeEventListener("focus", refresh); };
  }, [reload]);

  return <div className="workspace-goal-tasks flex min-h-0 min-w-0 flex-1 flex-col gap-3" data-testid="workspace-goal-tasks">
    <div className="flex shrink-0 flex-wrap items-end gap-2">
      <div className="min-w-0 flex-1">{goals.length > 0 && <Select label="目标任务" value={selected} onChange={event => { setSelected(event.target.value); onSelectGraph?.(event.target.value); }} options={goals.map(goal => ({ value: goal.graph_id, label: goal.user_goal }))} />}</div>
      <Button size="sm" variant="secondary" disabled={loading} onClick={() => void reload()}>刷新目标</Button>
    </div>
    {error && <ErrorState title="目标任务读取失败" description={error} onRetry={() => void reload()} />}
    {loading && !goals.length ? <p role="status">正在读取当前空间的目标任务…</p>
        : !error && !goals.length ? <EmptyState title="当前空间还没有已确认目标" description="目标确认后会在此显示，沿用同一任务画布、执行记录和交付物。" action={<Button onClick={() => navigate(taskWorkflowDestination(new URLSearchParams({ mode: "goal", workspace: workspaceId }).toString()))}>规划任务画布</Button>} />
        : selected ? <div className="workspace-goal-status min-h-0 min-w-0 flex-1 overflow-y-auto"><GoalTrackingPanel key={selected} graphId={selected} compact /></div> : null}
  </div>;
}
