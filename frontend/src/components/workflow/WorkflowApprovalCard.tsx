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
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");

  const run = (action: (onEvent: (e: WorkflowApprovalEvent) => void) => Promise<void>) => {
    setBusy(true);
    setMessage("");
    action((event) => {
      if (event.type === "workflow_approval_error") {
        setMessage(event.error || "审批处理失败");
      }
      if (event.type === "approval_resolved" || event.type === "workflow_run_updated") {
        setMessage("已处理，正在刷新…");
      }
    }).finally(() => {
      setBusy(false);
      onDecision();
    });
  };

  const handleApprove = () => run((cb) => approveWorkflowApproval(approval.approval_id, cb));
  const handleReject = () => run((cb) => rejectWorkflowApproval(approval.approval_id, cb));
  const handleCancel = async () => {
    setBusy(true);
    setMessage("");
    await cancelWorkflowApprovalAction(approval.approval_id);
    setBusy(false);
    onDecision();
  };

  return (
    <div className="rounded-lg border border-[var(--warning)]/30 bg-[var(--warning)]/8 p-4 space-y-3">
      <div className="flex items-center gap-2">
        <AlertTriangle className="w-4 h-4 text-[var(--warning)]" />
        <h4 className="font-semibold text-sm">需要审批</h4>
        <Badge tone={RISK_TONE[approval.risk_level] || "default"}>
          风险 {approval.risk_level}
        </Badge>
      </div>
      <div className="text-sm space-y-1">
        <p>
          <span className="text-[var(--text-muted)]">工具：</span>
          <code className="px-1.5 py-0.5 rounded text-xs bg-[var(--panel-2)]">{approval.tool_name}</code>
        </p>
        {approval.reason && <p className="text-xs text-[var(--text-muted)]">{approval.reason}</p>}
      </div>
      {message && <p className="text-xs text-[var(--text-muted)]">{message}</p>}
      <div className="flex items-center gap-2">
        <Button size="sm" variant="primary" onClick={handleApprove} disabled={busy}>
          <Check className="w-3.5 h-3.5" />
          批准
        </Button>
        <Button size="sm" variant="secondary" onClick={handleReject} disabled={busy}>
          <X className="w-3.5 h-3.5" />
          拒绝
        </Button>
        <Button size="sm" variant="ghost" onClick={handleCancel} disabled={busy}>
          <Ban className="w-3.5 h-3.5" />
          取消
        </Button>
      </div>
    </div>
  );
}
