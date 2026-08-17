import { Badge } from "../../components/ui";
import {
  formatElapsed,
  formatToolDisplayName,
} from "../../components/chat/toolDisplay";
import type {
  ExecutionRecord,
  ExecutionStatus,
  VerificationStatus,
} from "./model";

const EXECUTION_LABELS: Record<ExecutionStatus, string> = {
  queued: "等待执行",
  awaiting_approval: "等待确认",
  running: "正在执行",
  interrupted: "执行已中断",
  succeeded: "已完成",
  failed: "执行失败",
  rejected: "用户拒绝，未执行",
  cancelled: "已取消，未执行",
};

const VERIFICATION_LABELS: Record<VerificationStatus, string> = {
  not_requested: "未验证",
  pending: "验证中",
  passed: "已验证",
  failed: "验证失败",
};

function executionTone(
  status: ExecutionStatus
): "default" | "success" | "warning" | "danger" | "accent" {
  if (status === "succeeded") return "success";
  if (status === "failed") return "danger";
  if (status === "running") return "accent";
  if (
    status === "awaiting_approval" ||
    status === "interrupted" ||
    status === "rejected" ||
    status === "cancelled"
  ) {
    return "warning";
  }
  return "default";
}

function verificationTone(
  status: VerificationStatus
): "default" | "success" | "warning" | "danger" {
  if (status === "passed") return "success";
  if (status === "failed") return "danger";
  if (status === "pending") return "warning";
  return "default";
}

export default function ExecutionHistory({
  records,
}: {
  records: ExecutionRecord[];
}) {
  if (records.length === 0) {
    return (
      <div className="rounded-xl border border-dashed border-[var(--border-soft)] px-4 py-6 text-center text-xs text-[var(--text-faint)]">
        暂无执行记录
      </div>
    );
  }

  return (
    <ol className="space-y-2" aria-label="执行步骤">
      {records.map((record) => (
        <li
          key={record.toolCallId}
          className="rounded-xl border border-[var(--border)] bg-[var(--panel)] p-3"
        >
          <div className="flex items-start justify-between gap-2">
            <div className="min-w-0">
              <p className="truncate text-xs font-semibold text-[var(--text-primary)]">
                {formatToolDisplayName(record.name)}
              </p>
              <p className="mt-1 truncate font-mono text-[10px] text-[var(--text-faint)]">
                {record.startedAt !== undefined
                  ? formatElapsed(record.startedAt, record.finishedAt ?? Date.now())
                  : record.toolCallId}
              </p>
            </div>
            <span className="shrink-0 text-[10px] text-[var(--text-faint)]">
              #{record.sequence}
            </span>
          </div>
          <div className="mt-2 flex flex-wrap gap-1.5">
            <Badge tone={executionTone(record.executionStatus)}>
              {EXECUTION_LABELS[record.executionStatus]}
            </Badge>
            <Badge tone={verificationTone(record.verificationStatus)}>
              {VERIFICATION_LABELS[record.verificationStatus]}
            </Badge>
          </div>
          {record.verificationReason && record.verificationStatus === "failed" && (
            <p className="mt-2 border-l-2 border-[var(--danger)] pl-2 text-xs leading-5 text-[var(--danger)]">
              {record.verificationReason}
            </p>
          )}
        </li>
      ))}
    </ol>
  );
}
