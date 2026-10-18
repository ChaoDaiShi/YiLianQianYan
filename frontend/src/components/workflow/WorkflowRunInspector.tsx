import { useCallback, useEffect, useRef, useState } from "react";
import { Ban, Loader2, Navigation, AlertCircle } from "lucide-react";
import { Button, Badge, Spinner, Inspector } from "../ui";
import { listPendingApprovals } from "../../api/approvals";
import {
  getWorkflowRun,
  cancelWorkflowRun,
  type WorkflowRunRecord,
  type WorkflowNodeRun,
  type WorkflowNodeRunStatus,
  type WorkflowRunStatus,
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

const NODE_STATUS_LABELS: Record<WorkflowNodeRunStatus, string> = {
  pending: "等待中",
  ready: "就绪",
  running: "运行中",
  waiting_approval: "待审批",
  completed: "已完成",
  failed: "已失败",
  cancelled: "已取消",
  skipped: "已跳过",
};

const RUN_STATUS_LABELS: Record<WorkflowRunStatus, string> = {
  created: "已创建",
  running: "运行中",
  waiting_approval: "待审批",
  completed: "已完成",
  failed: "已失败",
  cancelled: "已取消",
};

const RUN_TONE: Record<string, "default" | "success" | "warning" | "danger" | "info"> = {
  created: "default",
  running: "info",
  waiting_approval: "warning",
  completed: "success",
  failed: "danger",
  cancelled: "default",
};

export interface WorkflowRunInspectorProps {
  runId: string;
  onClose: () => void;
  selectedNodeId?: string | null;
  onNodeSelect?: (id: string) => void;
  onNodeHighlight?: (id: string | null) => void;
  onLocateNode?: (id: string) => void;
  onRunLoaded?: (run: WorkflowRunRecord) => void;
}

export default function WorkflowRunInspector({
  runId,
  onClose,
  selectedNodeId,
  onNodeSelect,
  onNodeHighlight,
  onLocateNode,
  onRunLoaded,
}: WorkflowRunInspectorProps) {
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
      onRunLoaded?.(res.data);
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
      setError(res.error || "无法加载工作流运行详情");
    }
  }, [runId, onRunLoaded]);

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
    if (cancelling) return;
    setCancelling(true);
    const res = await cancelWorkflowRun(runId);
    if (res.ok) {
      setRun(res.data);
      onRunLoaded?.(res.data);
    } else {
      setError(res.error || "取消运行失败");
    }
    await load();
    setCancelling(false);
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
      <div className="rounded-[var(--radius-md)] border border-[var(--danger)]/30 bg-[var(--danger)]/10 p-4 text-sm text-[var(--danger)] space-y-2">
        <div className="flex items-center gap-1.5 font-medium">
          <AlertCircle className="w-4 h-4 shrink-0" />
          <span>无法加载运行：{error}</span>
        </div>
        <Button size="sm" variant="secondary" onClick={load}>
          重试
        </Button>
      </div>
    );
  }

  const selectedNodeExistsInRun =
    Boolean(selectedNodeId) && run?.nodes?.some((node) => node.node_id === selectedNodeId);

  return (
    <Inspector
      title={`运行 ${run!.run_id.slice(0, 8)}…`}
      badge={
        <Badge tone={RUN_TONE[run!.status] || "default"}>
          <span className="flex items-center gap-1">
            {run!.status === "running" && (
              <span className="h-1.5 w-1.5 rounded-full bg-[var(--accent-primary)] animate-pulse" />
            )}
            {RUN_STATUS_LABELS[run!.status] || run!.status}
          </span>
        </Badge>
      }
      description={`执行 ${run!.execution_id.slice(0, 8)}… · 创建于 ${new Date(run!.created_at).toLocaleTimeString()} · 更新于 ${new Date(run!.updated_at).toLocaleTimeString()}`}
      actions={
        !terminal ? (
          <Button
            size="sm"
            variant="secondary"
            onClick={handleCancel}
            disabled={cancelling}
            loading={cancelling}
            loadingLabel="取消中…"
          >
            <Ban className="w-3.5 h-3.5" />
            取消运行
          </Button>
        ) : undefined
      }
      onClose={onClose}
      bodyClassName="space-y-3.5 p-4"
    >

      {approval && <WorkflowApprovalCard approval={approval} onDecision={load} />}

      {/* Selected canvas node not found in this run callout */}
      {selectedNodeId && !selectedNodeExistsInRun && (
        <div className="rounded-[var(--radius-sm)] border border-[var(--border-soft)] bg-[var(--surface-muted)] p-2.5 text-xs text-[var(--text-secondary)]">
          <span className="font-mono font-medium text-[var(--text)]">{selectedNodeId}</span>
          ：此运行中没有该节点记录
        </div>
      )}

      <div className="space-y-2">
        <div className="flex items-center justify-between text-xs text-[var(--text-secondary)] px-1">
          <span>节点步骤 ({run!.nodes.length})</span>
          {polling && !terminal && (
            <span className="flex items-center gap-1 text-[var(--text-faint)]">
              <Loader2 className="w-3 h-3 animate-spin" />
              轮询中…
            </span>
          )}
        </div>

        <div className="space-y-1.5 divide-y divide-[var(--border-soft)]/50">
          {run!.nodes.map((node: WorkflowNodeRun) => (
            <NodeRow
              key={node.node_id}
              node={node}
              isSelected={selectedNodeId === node.node_id}
              onSelect={() => onNodeSelect?.(node.node_id)}
              onHighlight={(highlight) => onNodeHighlight?.(highlight ? node.node_id : null)}
              onLocate={() => onLocateNode?.(node.node_id)}
            />
          ))}
        </div>
      </div>
    </Inspector>
  );
}

function NodeRow({
  node,
  isSelected,
  onSelect,
  onHighlight,
  onLocate,
}: {
  node: WorkflowNodeRun;
  isSelected: boolean;
  onSelect: () => void;
  onHighlight: (highlight: boolean) => void;
  onLocate: () => void;
}) {
  const toneClasses: Record<string, string> = {
    completed: "text-[var(--success)]",
    failed: "text-[var(--danger)]",
    waiting_approval: "text-[var(--warning)]",
    running: "text-[var(--accent-primary)]",
    cancelled: "text-[var(--text-faint)]",
    skipped: "text-[var(--text-faint)]",
    pending: "text-[var(--text-muted)]",
    ready: "text-[var(--text-muted)]",
  };

  // Only calculate and display duration if both started_at and finished_at are present
  let durationText = "";
  if (node.started_at != null && node.finished_at != null) {
    const durationMs = Math.max(0, node.finished_at - node.started_at);
    durationText = durationMs < 1000 ? `${durationMs}ms` : `${(durationMs / 1000).toFixed(1)}s`;
  }

  return (
    <div
      onClick={onSelect}
      onMouseEnter={() => onHighlight(true)}
      onMouseLeave={() => onHighlight(false)}
      className={`group flex items-start gap-2.5 py-2 px-2 rounded-[var(--radius-sm)] cursor-pointer transition-colors ${
        isSelected
          ? "bg-[var(--accent-primary)]/10 ring-1 ring-[var(--accent-primary)]"
          : "hover:bg-[var(--surface-hover)]"
      }`}
      role="button"
      tabIndex={0}
      aria-label={`节点步骤 ${node.node_id}，状态 ${NODE_STATUS_LABELS[node.status] || node.status}`}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect();
        }
      }}
    >
      <span
        className={`w-5 flex-shrink-0 text-center font-bold text-sm mt-0.5 ${
          toneClasses[node.status] || "text-[var(--text-faint)]"
        }`}
      >
        {NODE_ICONS[node.status] || "○"}
      </span>

      <div className="flex-1 min-w-0 space-y-1">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2 min-w-0">
            <code className="text-xs font-mono font-semibold truncate text-[var(--text)]">
              {node.node_id}
            </code>
            <span
              className={`text-xs ${
                node.status === "failed"
                  ? "text-[var(--danger)] font-medium"
                  : node.status === "running"
                  ? "text-[var(--accent-primary)] font-medium"
                  : "text-[var(--text-secondary)]"
              }`}
            >
              {NODE_STATUS_LABELS[node.status] || node.status}
            </span>
          </div>

          <div className="flex items-center gap-1.5 shrink-0">
            {durationText && (
              <span className="text-[11px] text-[var(--text-faint)] font-mono">
                {durationText}
              </span>
            )}
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onLocate();
              }}
              className="inline-flex items-center gap-1 text-[11px] px-1.5 py-0.5 rounded text-[var(--accent-primary)] hover:bg-[var(--accent-primary)]/10 opacity-70 group-hover:opacity-100 transition-opacity"
              title="在画布中定位此节点"
              aria-label={`在画布中定位节点 ${node.node_id}`}
            >
              <Navigation className="w-3 h-3" />
              定位
            </button>
          </div>
        </div>

        {node.result?.summary && (
          <p className="text-xs text-[var(--text-secondary)] line-clamp-3 bg-[var(--surface-muted)]/70 rounded p-1.5 leading-relaxed">
            {node.result.summary}
          </p>
        )}

        {node.error && (
          <div className="text-xs text-[var(--danger)] bg-[var(--danger)]/8 border border-[var(--danger)]/20 rounded p-1.5 font-mono leading-relaxed break-all">
            {node.error}
          </div>
        )}
      </div>
    </div>
  );
}
