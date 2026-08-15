import { useCallback, useEffect, useState } from "react";
import { History } from "lucide-react";
import { Button, EmptyState, Spinner, Badge } from "../ui";
import {
  listWorkflowRuns,
  type WorkflowRunRecord,
  type WorkflowRunStatus,
} from "../../api/client";

const STATUS_TONE: Record<WorkflowRunStatus, "default" | "success" | "warning" | "danger" | "info"> = {
  created: "default",
  running: "info",
  waiting_approval: "warning",
  completed: "success",
  failed: "danger",
  cancelled: "default",
};

export default function WorkflowRecentRuns({
  onSelectRun,
  refreshKey,
}: {
  onSelectRun: (runId: string) => void;
  refreshKey: number;
}) {
  const [runs, setRuns] = useState<WorkflowRunRecord[]>([]);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    const res = await listWorkflowRuns({ limit: 20 });
    if (res.ok) setRuns(res.data);
    setLoading(false);
  }, []);

  useEffect(() => {
    load();
  }, [load, refreshKey]);

  if (loading) {
    return (
      <div className="flex justify-center py-8">
        <Spinner className="w-6 h-6" />
      </div>
    );
  }

  return (
    <div className="rounded-lg border border-[var(--border)] p-4 space-y-2">
      <div className="flex items-center gap-2">
        <History className="w-4 h-4 text-[var(--text-muted)]" />
        <h4 className="font-semibold text-sm">最近运行</h4>
      </div>
      {runs.length === 0 ? (
        <EmptyState
          icon={<span className="text-2xl">🕘</span>}
          title="暂无运行记录"
          description="启动一个运行工作流后，这里会显示历史记录。"
        />
      ) : (
        <div className="divide-y divide-[var(--border)]/50">
          {runs.map((run) => (
            <button
              key={run.run_id}
              onClick={() => onSelectRun(run.run_id)}
              className="w-full flex items-center gap-3 py-2 text-left hover:bg-[var(--panel-hover)] rounded px-1 transition-colors"
            >
              <div className="flex-1 min-w-0">
                <p className="text-sm truncate">{run.run_id.slice(0, 12)}…</p>
                <p className="text-xs text-[var(--text-faint)]">
                  {new Date(run.updated_at).toLocaleString()}
                </p>
              </div>
              <Badge tone={STATUS_TONE[run.status]}>{run.status}</Badge>
            </button>
          ))}
        </div>
      )}
      <div className="pt-1">
        <Button size="sm" variant="ghost" onClick={load}>
          刷新
        </Button>
      </div>
    </div>
  );
}
