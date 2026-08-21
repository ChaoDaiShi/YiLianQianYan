import { useState } from "react";
import { ChevronDown, ChevronUp } from "lucide-react";
import { Badge } from "../../components/ui";
import { presentExecutionError } from "../../components/chat/errorDisplay";
import {
  formatElapsed,
  formatToolActionSummary,
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

function formatValue(value: unknown): string {
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function ExecutionHistoryItem({ record }: { record: ExecutionRecord }) {
  const [expanded, setExpanded] = useState(false);
  const action = formatToolActionSummary(record.name, record.args);
  const elapsed =
    record.startedAt !== undefined
      ? formatElapsed(record.startedAt, record.finishedAt ?? Date.now())
      : null;
  const failedPresentation =
    record.executionStatus === "failed" && record.result
      ? presentExecutionError(record.result)
      : null;
  const verificationPresentation =
    record.verificationStatus === "failed" && record.verificationReason
      ? presentExecutionError(record.verificationReason)
      : null;
  const detailsId = `execution-details-${record.toolCallId}`;
  const hasArgs = Object.keys(record.args).length > 0;

  return (
    <li
      className="execution-history-item overflow-hidden rounded-xl border border-[var(--border)] bg-[var(--panel)]"
      data-execution-status={record.executionStatus}
      data-verification-status={record.verificationStatus}
    >
      <button
        type="button"
        className="execution-history-action w-full p-3 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[var(--focus-ring-soft)]"
        onClick={() => setExpanded((current) => !current)}
        aria-expanded={expanded}
        aria-controls={detailsId}
        aria-label={`${expanded ? "收起" : "展开"}执行详情：${action}`}
      >
        <span className="flex items-start justify-between gap-2">
          <span className="min-w-0 flex-1">
            <span className="block text-xs font-semibold leading-5 text-[var(--text-primary)]">
              {action}
            </span>
            <span className="mt-0.5 flex min-w-0 flex-wrap items-center gap-x-1.5 text-[10px] text-[var(--text-faint)]">
              <span>{formatToolDisplayName(record.name)}</span>
              {elapsed ? <span aria-hidden="true">·</span> : null}
              {elapsed ? <span>{elapsed}</span> : null}
            </span>
          </span>
          <span className="flex shrink-0 items-center gap-1.5 text-[10px] text-[var(--text-faint)]">
            <span>#{record.sequence}</span>
            {expanded ? (
              <ChevronUp className="h-3.5 w-3.5" aria-hidden="true" />
            ) : (
              <ChevronDown className="h-3.5 w-3.5" aria-hidden="true" />
            )}
          </span>
        </span>

        <span className="mt-2 flex flex-wrap gap-1.5">
          <Badge tone={executionTone(record.executionStatus)}>
            {EXECUTION_LABELS[record.executionStatus]}
          </Badge>
          <Badge tone={verificationTone(record.verificationStatus)}>
            {VERIFICATION_LABELS[record.verificationStatus]}
          </Badge>
        </span>

        {failedPresentation ? (
          <span className="mt-2 block text-xs leading-5 text-[var(--danger-fg)]">
            {failedPresentation.message}
          </span>
        ) : null}
        {verificationPresentation ? (
          <span className="mt-2 block text-xs leading-5 text-[var(--danger-fg)]">
            {verificationPresentation.message}
          </span>
        ) : null}
      </button>

      {expanded ? (
        <div id={detailsId} className="execution-history-details">
          <dl className="space-y-2 text-xs">
            <div>
              <dt className="text-[var(--text-faint)]">Tool Name</dt>
              <dd className="font-mono">{record.name}</dd>
            </div>
            <div>
              <dt className="text-[var(--text-faint)]">Tool Call ID</dt>
              <dd className="font-mono">{record.toolCallId}</dd>
            </div>
            {typeof record.args.command === "string" ? (
              <div>
                <dt className="text-[var(--text-faint)]">Command</dt>
                <dd className="execution-history-technical-value">
                  {record.args.command}
                </dd>
              </div>
            ) : null}
            {hasArgs ? (
              <div>
                <dt className="text-[var(--text-faint)]">Arguments</dt>
                <dd className="execution-history-technical-value">
                  {formatValue(record.args)}
                </dd>
              </div>
            ) : null}
            {record.reason ? (
              <div>
                <dt className="text-[var(--text-faint)]">操作原因</dt>
                <dd>{record.reason}</dd>
              </div>
            ) : null}
            {record.riskLevel !== "unknown" ? (
              <div>
                <dt className="text-[var(--text-faint)]">风险等级</dt>
                <dd>{record.riskLevel}</dd>
              </div>
            ) : null}
            {elapsed ? (
              <div>
                <dt className="text-[var(--text-faint)]">执行耗时</dt>
                <dd>{elapsed}</dd>
              </div>
            ) : null}
            {record.result ? (
              <div>
                <dt className="text-[var(--text-faint)]">
                  {record.executionStatus === "failed" ? "错误日志" : "执行结果"}
                </dt>
                <dd className="execution-history-technical-value">
                  {record.result}
                </dd>
              </div>
            ) : null}
            {record.verificationReason ? (
              <div>
                <dt className="text-[var(--text-faint)]">验证信息</dt>
                <dd className="execution-history-technical-value">
                  {record.verificationReason}
                </dd>
              </div>
            ) : null}
          </dl>
        </div>
      ) : null}
    </li>
  );
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
        <ExecutionHistoryItem key={record.toolCallId} record={record} />
      ))}
    </ol>
  );
}
