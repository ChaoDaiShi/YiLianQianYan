import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { ChevronDown, ChevronUp, X } from "lucide-react";
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

interface ExecutionHistoryItemProps {
  record: ExecutionRecord;
  expanded: boolean;
  onToggle: () => void;
  onClose: () => void;
}

function ExecutionHistoryItem({
  record,
  expanded,
  onToggle,
  onClose,
}: ExecutionHistoryItemProps) {
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const action = formatToolActionSummary(record.name, record.args);
  const elapsed =
    record.startedAt !== undefined && record.finishedAt !== undefined
      ? formatElapsed(record.startedAt, record.finishedAt)
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

  useEffect(() => {
    if (!expanded) return;

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
        triggerRef.current?.focus();
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [expanded, onClose]);

  return (
    <li
      className="execution-history-item !rounded-md !border-0 !bg-transparent !shadow-none"
      data-execution-status={record.executionStatus}
      data-verification-status={record.verificationStatus}
    >
      <button
        ref={triggerRef}
        type="button"
        className="execution-history-action w-full px-2 py-2 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[var(--focus-ring-soft)]"
        onClick={onToggle}
        aria-expanded={expanded}
        aria-controls={detailsId}
        aria-label={`${expanded ? "收起" : "展开"}执行详情：${action}`}
      >
        <span className="flex items-start justify-between gap-2">
          <span className="min-w-0 flex-1">
            <span className="block truncate text-xs font-normal leading-5 text-[var(--text-primary)]">
              {action}
            </span>
            <span className="mt-0.5 flex min-w-0 flex-wrap items-center gap-x-1.5 text-[10px] text-[var(--text-faint)]">
              <span>{formatToolDisplayName(record.name)}</span>
              <span aria-hidden="true">·</span>
              <span>{EXECUTION_LABELS[record.executionStatus]}</span>
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

        {record.verificationStatus !== "not_requested" && <span className="mt-1 flex flex-wrap gap-1.5">
          <Badge tone={verificationTone(record.verificationStatus)}>
            {VERIFICATION_LABELS[record.verificationStatus]}
          </Badge>
        </span>}

        {failedPresentation ? (
          <span className="mt-2 block text-xs leading-5 text-[var(--danger-fg)]">
            {failedPresentation.message}
          </span>
        ) : null}
        {record.verificationStatus === "failed" ? (
          <span className="mt-2 block text-xs leading-5 text-[var(--danger-fg)]">
            {verificationPresentation?.message || "操作结果未能通过验证检查"}
          </span>
        ) : null}
      </button>

      {expanded
        ? createPortal(
          <div
            className="execution-history-modal-backdrop fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/45 backdrop-blur-sm animate-in fade-in duration-150"
            onClick={(event) => {
              if (event.target === event.currentTarget) {
                onClose();
                triggerRef.current?.focus();
              }
            }}
          >
            <div
              ref={panelRef}
              id={detailsId}
              className="execution-history-details relative flex flex-col w-full max-w-2xl max-h-[85vh] rounded-2xl border border-[var(--accent-border)] bg-[var(--surface-solid)] shadow-2xl overflow-hidden animate-in zoom-in-95 duration-150"
              role="dialog"
              aria-modal="false"
              aria-label={`执行详情：${action}`}
            >
              <header className="execution-history-details-header flex items-center justify-between border-b border-[var(--divider)] px-5 py-3.5 bg-[var(--surface-muted)]/50">
                <div className="min-w-0">
                  <p className="text-[10px] font-semibold uppercase tracking-[0.12em] text-[var(--accent-primary)]">
                    执行详情
                  </p>
                  <h4 className="mt-1 truncate text-sm font-semibold text-[var(--text-primary)]">
                    {action}
                  </h4>
                </div>
                <button
                  type="button"
                  className="rounded-lg p-1.5 text-[var(--text-faint)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
                  onClick={() => {
                    onClose();
                    triggerRef.current?.focus();
                  }}
                  aria-label="关闭执行详情"
                >
                  <X className="h-4 w-4" aria-hidden="true" />
                </button>
              </header>
              <div className="execution-history-details-scroll scrollbar-thin">
              <dl className="min-w-0 max-w-full space-y-3 text-xs">
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
          </div>
        </div>,
        document.body,
      )
      : null}
    </li>
  );
}

export default function ExecutionHistory({
  records,
}: {
  records: ExecutionRecord[];
}) {
  const [expandedId, setExpandedId] = useState<string | null>(null);

  if (records.length === 0) {
    return (
      <div className="rounded-xl border border-dashed border-[var(--border-soft)] px-4 py-6 text-center text-xs text-[var(--text-faint)]">
        暂无执行记录
      </div>
    );
  }

  return (
    <ol className="space-y-0.5" aria-label="执行步骤">
      {records.map((record) => (
        <ExecutionHistoryItem
          key={record.toolCallId}
          record={record}
          expanded={expandedId === record.toolCallId}
          onToggle={() =>
            setExpandedId((current) =>
              current === record.toolCallId ? null : record.toolCallId,
            )
          }
          onClose={() => setExpandedId(null)}
        />
      ))}
    </ol>
  );
}
