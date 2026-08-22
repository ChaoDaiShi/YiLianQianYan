import { useCallback, useEffect, useRef, useState } from "react";
import { X, Ban, Loader2 } from "lucide-react";
import { Button, Badge, Spinner } from "../ui";
import { listPendingApprovals } from "../../api/approvals";
import {
  getWorkflowRun,
  cancelWorkflowRun,
  type WorkflowRunRecord,
  type WorkflowNodeRun,
} from "../../api/client";
import type { PendingApproval } from "../../types/approval";
import WorkflowApprovalCard from "./WorkflowApprovalCard";

const NODE_ICONS: Record<string, string> = {
  completed: "✓",
  running: "▶",
  waiting_approval: "⚠",
  pending: "○",
  ready: "○",
  failed: "✗",
  cancelled: "⊘",
  skipped: "…",
};

const RUN_TONE: Record<string, "default" | "success" | "warning" | "danger" | "info"> = {
  created: "default",
  running: "info",
  waiting_approval: "warning",
  completed: "success",
  failed: "danger",
  cancelled: "default",
};

export default function WorkflowRunInspector({
  runId,
  onClose,
}: {
  runId: string;
  onClose: () => void;
}) {
  const [run, setRun] = useState<WorkflowRunRecord | null>(null);
  const [approval, setApproval] = useState<PendingApproval | null>(null);
  const [error, setError] = useState("");
  const [polling, setPolling] = useState(true);
  const [cancelling, setCancelling] = useState(false);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const terminal = run
    ? run.status === "completed" || run.status === "failed" || run.status === "cancelled"
    : false;

  const load = useCallback(async () => {
    const res = await getWorkflowRun(runId);
    if (res.ok) {
      setRun(res.data);
      setError("");
      if (res.data.status === "waiting_approval") {
        const pending = await listPendingApprovals();
        if (pending) {
          const match = pending.find((a) => a.workflow_run_id === runId);
          setApproval(match || null);
        }
      } else {
        setApproval(null);
      }
    } else {
      setError(res.error);
    }
  }, [runId]);

  useEffect(() => {
    load();
    timerRef.current = setInterval(load, 1000);
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [load]);

  useEffect(() => {
    if (terminal && timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
      setPolling(false);
    }
  }, [terminal]);

  const handleCancel = async () => {
    setCancelling(true);
    const res = await cancelWorkflowRun(runId);
    setCancelling(false);
    if (res.ok) setRun(res.data);
    else setError(res.error);
    load();
  };

  if (!run && !error) {
    return (
      <div className="flex justify-center py-12">
        <Spinner className="w-8 h-8" />
      </div>
    );
  }

  if (!run && error) {
    return (
      <div className="rounded-lg border border-[var(--danger)]/30 bg-[var(--danger)]/10 p-4 text-sm text-[var(--danger)]">
        无法加载运行：{error}
      </div>
    );
  }

  return (
    <div className="rounded-lg border border-[var(--border)] p-4 space-y-3">
      <div className="flex items-start justify-between gap-2">
        <div className="space-y-1">
          <h4 className="font-semibold text-sm">运行 {run!.run_id.slice(0, 8)}…</h4>
          <div className="flex items-center gap-2 text-xs text-[var(--text-muted)] flex-wrap">
            <Badge tone={RUN_TONE[run!.status] || "default"}>{run!.status}</Badge>
            <span>执行 {run!.execution_id.slice(0, 8)}…</span>
            <span>创建于 {new Date(run!.created_at).toLocaleString()}</span>
          </div>
        </div>
        <div className="flex items-center gap-1">
          {!terminal && (
            <Button size="sm" variant="secondary" onClick={handleCancel} disabled={cancelling}>
              {cancelling ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Ban className="w-3.5 h-3.5" />}
              取消运行
            </Button>
          )}
          <Button size="sm" variant="ghost" onClick={onClose} aria-label="关闭运行详情">
            <X className="w-4 h-4" />
          </Button>
        </div>
      </div>

      {approval && <WorkflowApprovalCard approval={approval} onDecision={load} />}

      <div className="space-y-1.5">
        {run!.nodes.map((node: WorkflowNodeRun) => (
          <NodeRow key={node.node_id} node={node} />
        ))}
      </div>

      {polling && !terminal && (
        <p className="text-xs text-[var(--text-faint)] flex items-center gap-1.5">
          <Loader2 className="w-3 h-3 animate-spin" />
          轮询中…
        </p>
      )}
    </div>
  );
}

function NodeRow({ node }: { node: WorkflowNodeRun }) {
  const tone: Record<string, string> = {
    completed: "text-[var(--success)]",
    failed: "text-[var(--danger)]",
    waiting_approval: "text-[var(--warning)]",
    running: "text-[var(--accent)]",
    cancelled: "text-[var(--text-faint)]",
  };
  return (
    <div className="flex items-start gap-2 py-1.5 border-b border-[var(--border)]/50 last:border-0">
      <span className={`w-5 flex-shrink-0 text-center ${tone[node.status] || "text-[var(--text-faint)]"}`}>
        {NODE_ICONS[node.status] || "○"}
      </span>
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <code className="text-xs font-mono">{node.node_id}</code>
          <span className="text-xs text-[var(--text-muted)]">{node.status}</span>
        </div>
        {node.result?.summary && (
          <p className="text-xs text-[var(--text-muted)] mt-0.5 line-clamp-3">{node.result.summary}</p>
        )}
        {node.error && <p className="text-xs text-[var(--danger)] mt-0.5 line-clamp-3">{node.error}</p>}
      </div>
    </div>
  );
}
