import { useCallback, useEffect, useState } from "react";
import { History, AlertCircle } from "lucide-react";
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

const STATUS_LABELS: Record<WorkflowRunStatus, string> = {
  created: "已创建",
  running: "运行中",
  waiting_approval: "待审批",
  completed: "已完成",
  failed: "已失败",
  cancelled: "已取消",
};

export default function WorkflowRecentRuns({
  onSelectRun,
  refreshKey,
  selectedRunId,
}: {
  onSelectRun: (runId: string) => void;
  refreshKey: number;
  selectedRunId?: string | null;
}) {
  const [runs, setRuns] = useState<WorkflowRunRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    setLoading(true);
    const res = await listWorkflowRuns({ limit: 20 });
    if (res.ok) {
      setRuns(res.data);
      setError("");
    } else {
      setError("error" in res && typeof res.error === "string" ? res.error : "无法加载最近运行记录");
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    load();
  }, [load, refreshKey]);

  return (
    <div className="rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-4 space-y-3">
      <div className="flex items-center justify-between gap-2 border-b border-[var(--border-soft)] pb-2">
        <div className="flex items-center gap-2">
          <History className="w-4 h-4 text-[var(--text-secondary)]" />
          <h4 className="font-semibold text-sm">最近运行</h4>
        </div>
        <Button size="sm" variant="ghost" onClick={load} className="h-7 px-2 text-xs">
          刷新
        </Button>
      </div>

      {loading && runs.length === 0 ? (
        <div className="flex justify-center py-6">
          <Spinner className="w-5 h-5" />
        </div>
      ) : error && runs.length === 0 ? (
        <div className="rounded-[var(--radius-sm)] border border-[var(--danger)]/30 bg-[var(--danger)]/10 p-3 text-xs text-[var(--danger)] space-y-2">
          <div className="flex items-center gap-1.5 font-medium">
            <AlertCircle className="w-3.5 h-3.5 shrink-0" />
            <span>加载最近运行失败</span>
          </div>
          <p className="text-[var(--text-secondary)]">{error}</p>
          <Button size="sm" variant="secondary" onClick={load} className="h-6 text-xs px-2">
            重试
          </Button>
        </div>
      ) : runs.length === 0 ? (
        <EmptyState
          icon={<span className="text-xl">🕘</span>}
          title="暂无运行记录"
          description="启动一个运行工作流后，这里会显示历史记录。"
          className="py-4"
        />
      ) : (
        <div className="divide-y divide-[var(--border-soft)]/60 max-h-[380px] overflow-y-auto scrollbar-thin">
          {runs.map((run) => {
            const isSelected = selectedRunId === run.run_id;
            const isRunning = run.status === "running";
            const failedNode = run.nodes?.find((n) => n.status === "failed" && n.error);
            return (
              <button
                key={run.run_id}
                type="button"
                onClick={() => onSelectRun(run.run_id)}
                className={`w-full flex flex-col gap-1.5 py-2.5 px-2 text-left rounded-[var(--radius-sm)] transition-colors ${
                  isSelected
                    ? "bg-[var(--accent-primary)]/10 ring-1 ring-[var(--accent-primary)]"
                    : "hover:bg-[var(--surface-hover)]"
                }`}
                aria-label={`运行 ${run.run_id}，状态 ${STATUS_LABELS[run.status] || run.status}`}
              >
                <div className="flex items-center justify-between gap-2 w-full">
                  <span className="text-xs font-mono font-medium truncate text-[var(--text)]">
                    {run.run_id.slice(0, 10)}…
                  </span>
                  <Badge tone={STATUS_TONE[run.status] || "default"}>
                    <span className="flex items-center gap-1">
                      {isRunning && (
                        <span className="h-1.5 w-1.5 rounded-full bg-[var(--accent-primary)] animate-pulse" />
                      )}
                      {STATUS_LABELS[run.status] || run.status}
                    </span>
                  </Badge>
                </div>

                <div className="flex items-center gap-2 text-[11px] text-[var(--text-faint)] flex-wrap">
                  <span>创建于 {new Date(run.created_at).toLocaleTimeString()}</span>
                  <span>·</span>
                  <span>更新于 {new Date(run.updated_at).toLocaleTimeString()}</span>
                </div>

                {run.status === "failed" && (
                  <p className="text-[11px] text-[var(--danger)] line-clamp-1 font-mono bg-[var(--danger)]/5 rounded px-1.5 py-0.5">
                    {failedNode?.error || "运行异常终止"}
                  </p>
                )}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
