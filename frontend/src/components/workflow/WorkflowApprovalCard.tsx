import { useState } from "react";
import { AlertTriangle, Check, X, Ban } from "lucide-react";
import { Button, Badge } from "../ui";
import type { PendingApproval } from "../../types/approval";
import {
  approveWorkflowApproval,
  rejectWorkflowApproval,
  cancelWorkflowApprovalAction,
  type WorkflowApprovalEvent,
} from "../../api/client";

const RISK_TONE: Record<string, "warning" | "danger" | "info" | "default"> = {
  low: "default",
  medium: "info",
  high: "warning",
  critical: "danger",
};

export default function WorkflowApprovalCard({
  approval,
  onDecision,
}: {
  approval: PendingApproval;
  onDecision: () => void;
}) {
  const [busyAction, setBusyAction] = useState<"approve" | "reject" | "cancel" | null>(null);
  const [message, setMessage] = useState("");

  const busy = busyAction !== null;

  const run = (
    actionType: "approve" | "reject",
    action: (onEvent: (e: WorkflowApprovalEvent) => void) => Promise<void>
  ) => {
    setBusyAction(actionType);
    setMessage("");
    action((event) => {
      if (event.type === "workflow_approval_error") {
        setMessage(event.error || "审批处理失败");
      }
      if (event.type === "approval_resolved" || event.type === "workflow_run_updated") {
        setMessage("已处理，正在刷新…");
      }
    }).finally(() => {
      setBusyAction(null);
      onDecision();
    });
  };

  const handleApprove = () => {
    if (busy) return;
    run("approve", (cb) => approveWorkflowApproval(approval.approval_id, cb));
  };

  const handleReject = () => {
    if (busy) return;
    run("reject", (cb) => rejectWorkflowApproval(approval.approval_id, cb));
  };

  const handleCancel = async () => {
    if (busy) return;
    setBusyAction("cancel");
    setMessage("");
    try {
      await cancelWorkflowApprovalAction(approval.approval_id);
    } catch {
      setMessage("取消审批操作失败");
    } finally {
      setBusyAction(null);
      onDecision();
    }
  };

  return (
    <div
      className="rounded-[var(--radius-md)] border border-[var(--warning)]/40 bg-[var(--surface-solid)] p-4 space-y-3 shadow-card-token"
      role="region"
      aria-label="工作流审批"
    >
      <div className="flex items-center justify-between gap-2 border-b border-[var(--border-soft)] pb-2.5">
        <div className="flex items-center gap-2 min-w-0">
          <AlertTriangle className="w-4 h-4 shrink-0 text-[var(--warning)]" />
          <h4 className="font-semibold text-sm truncate">需要审批执行</h4>
        </div>
        <Badge tone={RISK_TONE[approval.risk_level] || "default"}>
          风险 {approval.risk_level}
        </Badge>
      </div>

      <div className="text-sm space-y-1.5">
        <div className="flex items-center gap-2">
          <span className="text-xs text-[var(--text-secondary)]">执行工具：</span>
          <code className="px-2 py-0.5 rounded text-xs bg-[var(--surface-muted)] font-mono text-[var(--text)] border border-[var(--border-soft)]">
            {approval.tool_name}
          </code>
        </div>
        {approval.reason && (
          <p className="text-xs text-[var(--text-secondary)] leading-relaxed">
            {approval.reason}
          </p>
        )}
      </div>

      {message && (
        <p className="text-xs text-[var(--text-secondary)] rounded bg-[var(--surface-muted)] px-2 py-1">
          {message}
        </p>
      )}

      <div className="flex items-center gap-2 pt-1 border-t border-[var(--border-soft)]">
        <Button
          size="sm"
          variant="primary"
          onClick={handleApprove}
          disabled={busy}
          loading={busyAction === "approve"}
          loadingLabel="批准中…"
        >
          <Check className="w-3.5 h-3.5" />
          批准
        </Button>
        <Button
          size="sm"
          variant="secondary"
          onClick={handleReject}
          disabled={busy}
          loading={busyAction === "reject"}
          loadingLabel="拒绝中…"
        >
          <X className="w-3.5 h-3.5" />
          拒绝
        </Button>
        <Button
          size="sm"
          variant="ghost"
          onClick={handleCancel}
          disabled={busy}
          loading={busyAction === "cancel"}
          loadingLabel="取消中…"
          className="ml-auto text-[var(--text-secondary)] hover:text-[var(--danger)]"
        >
          <Ban className="w-3.5 h-3.5" />
          取消
        </Button>
      </div>
    </div>
  );
}
