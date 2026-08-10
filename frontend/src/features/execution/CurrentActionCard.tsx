import {
  AlertCircle,
  Ban,
  CircleCheck,
  LoaderCircle,
  ShieldAlert,
  Unplug,
} from "lucide-react";
import ApprovalCard from "../../components/approval/ApprovalCard";
import type { PendingApproval } from "../../types/approval";
import type { ConnectionStatus, ExecutionRecord } from "./model";

interface CurrentActionCardProps {
  record: ExecutionRecord | null;
  connection: ConnectionStatus;
  approval?: PendingApproval;
  resolving?: boolean;
  onApprove: (approval: PendingApproval) => void;
  onReject: (approval: PendingApproval) => void;
}

export default function CurrentActionCard({
  record,
  connection,
  approval,
  resolving = false,
  onApprove,
  onReject,
}: CurrentActionCardProps) {
  if (record?.approvalStatus === "pending" && approval) {
    return (
      <ApprovalCard
        approval={approval}
        resolving={resolving}
        onApprove={onApprove}
        onReject={onReject}
      />
    );
  }

  if (!record) {
    const interrupted = connection === "interrupted" || connection === "error";
    return (
      <div className="rounded-xl border border-[var(--border)] bg-[var(--panel-2)] p-4">
        <div className="flex items-start gap-3">
          {interrupted ? (
            <Unplug className="mt-0.5 h-4 w-4 shrink-0 text-[var(--danger)]" />
          ) : (
            <CircleCheck className="mt-0.5 h-4 w-4 shrink-0 text-[var(--success)]" />
          )}
          <div>
            <p className="text-sm font-medium">
              {interrupted ? "执行流已中断" : "当前没有待处理动作"}
            </p>
            <p className="mt-1 text-xs leading-5 text-[var(--text-muted)]">
              {interrupted
                ? "连接未正常结束，请检查聊天区提示后重试。"
                : "新的工具调用或审批请求会显示在这里。"}
            </p>
          </div>
        </div>
      </div>
    );
  }

  const verificationFailed = record.verificationStatus === "failed";
  const executionFailed = record.executionStatus === "failed";
  const rejected = record.executionStatus === "rejected";
  const cancelled = record.executionStatus === "cancelled";
  const awaitingApproval = record.executionStatus === "awaiting_approval";
  const title = verificationFailed
    ? "验证失败"
    : executionFailed
      ? "执行失败"
      : rejected
        ? "用户拒绝，未执行"
        : cancelled
          ? "已取消，未执行"
          : awaitingApproval
            ? "等待用户确认"
            : `正在执行 ${record.name}`;
  const Icon = verificationFailed || executionFailed
    ? AlertCircle
    : rejected || cancelled
      ? Ban
      : awaitingApproval
        ? ShieldAlert
        : LoaderCircle;
  const iconColor = verificationFailed || executionFailed
    ? "text-[var(--danger)]"
    : rejected || cancelled || awaitingApproval
      ? "text-[var(--warning)]"
      : "text-[var(--accent)]";

  return (
    <div className="rounded-xl border border-[var(--border)] bg-[var(--panel-2)] p-4">
      <div className="flex items-start gap-3">
        <Icon
          className={`mt-0.5 h-4 w-4 shrink-0 ${iconColor} ${
            record.executionStatus === "running" ? "animate-spin" : ""
          }`}
        />
        <div className="min-w-0 flex-1">
          <p className="text-sm font-semibold">{title}</p>
          <p className="mt-1 truncate font-mono text-xs text-[var(--text-muted)]">
            {record.name}
          </p>
          {record.reason && (
            <p className="mt-2 text-xs leading-5 text-[var(--text-muted)]">
              {record.reason}
            </p>
          )}
          {record.result && executionFailed && (
            <pre className="mt-2 max-h-28 overflow-auto whitespace-pre-wrap rounded-lg border border-[var(--border)] bg-[var(--input-bg)] p-2 text-xs text-[var(--danger)]">
              {record.result}
            </pre>
          )}
          {record.verificationReason && verificationFailed && (
            <div className="mt-2 rounded-lg border border-[var(--danger)]/30 bg-[var(--danger)]/10 p-2 text-xs leading-5 text-[var(--danger)]">
              {record.verificationReason}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
