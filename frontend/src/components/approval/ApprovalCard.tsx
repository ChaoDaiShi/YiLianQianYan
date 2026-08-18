import { ShieldAlert, ShieldCheck } from "lucide-react";
import type { PendingApproval } from "../../types/approval";
import { formatToolDisplayName } from "../chat/toolDisplay";
import { Button } from "../ui";

interface ApprovalCardProps {
  approval: PendingApproval;
  resolving?: boolean;
  onApprove: (approval: PendingApproval) => void;
  onReject: (approval: PendingApproval) => void;
}

const SENSITIVE_KEYS = [
  "api_key",
  "apikey",
  "token",
  "password",
  "secret",
  "authorization",
  "cookie",
];

function maskArgs(args: Record<string, unknown>): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(args).map(([key, value]) => [
      key,
      SENSITIVE_KEYS.some((sensitive) => key.toLowerCase().includes(sensitive))
        ? "••••••••"
        : value,
    ]),
  );
}

function riskTone(risk: string): string {
  return risk === "critical" || risk === "high"
    ? "border-[var(--danger-border)] bg-[var(--danger-soft)] text-[var(--danger-fg)]"
    : "border-[var(--warning-border)] bg-[var(--warning-soft)] text-[var(--warning-fg)]";
}

function riskLabel(risk: string): string {
  const labels: Record<string, string> = {
    critical: "严重风险",
    high: "高风险",
    medium: "中风险",
    low: "低风险",
  };
  return labels[risk] || "风险待确认";
}

function targetFromArgs(args: Record<string, unknown>): string | null {
  for (const key of ["target", "path", "file", "url", "command"]) {
    const value = args[key];
    if (typeof value === "string" && value.trim()) return value;
  }
  return null;
}

export default function ApprovalCard({
  approval,
  resolving = false,
  onApprove,
  onReject,
}: ApprovalCardProps) {
  const isCritical = approval.risk_level === "critical";
  const target = targetFromArgs(approval.arguments || {});
  const maskedArgs = JSON.stringify(maskArgs(approval.arguments || {}), null, 2);

  return (
    <section
      className={
        "conversation-approval-card overflow-hidden rounded-[var(--radius-lg)] border bg-[var(--surface-solid)] " +
        (isCritical
          ? "border-[var(--danger-border)]"
          : "border-[var(--warning-border)]")
      }
      aria-label="等待用户确认"
    >
      <div
        className={
          "approval-card-header flex items-center gap-2 border-b px-4 py-3 " +
          (isCritical
            ? "border-[var(--danger-border)] bg-[var(--danger-soft)]"
            : "border-[var(--warning-border)] bg-[var(--warning-soft)]")
        }
      >
        {isCritical ? (
          <ShieldAlert className="h-4 w-4 text-[var(--danger)]" />
        ) : (
          <ShieldCheck className="h-4 w-4 text-[var(--accent-gold)]" />
        )}
        <div>
          <p className="approval-card-title text-sm font-semibold text-[var(--text-primary)]">
            <span className="approval-card-star" aria-hidden="true">✦</span>
            需要你的许可
          </p>
          <p className="mt-0.5 text-xs text-[var(--text-secondary)]">
            这一步在得到确认前不会执行
          </p>
        </div>
        <span
          className={
            "ml-auto rounded-full border px-2 py-0.5 text-[10px] font-medium " +
            riskTone(approval.risk_level)
          }
        >
          {riskLabel(approval.risk_level)}
        </span>
      </div>

      <div className="approval-card-details space-y-3 px-4 py-3 text-xs">
        <div>
          <span className="text-[var(--text-faint)]">操作</span>
          <p className="mt-0.5 font-medium text-[var(--text-primary)]">
            {formatToolDisplayName(approval.tool_name)}
          </p>
        </div>
        {target && (
          <div>
            <span className="text-[var(--text-faint)]">目标</span>
            <p className="mt-0.5 break-all font-mono text-[var(--text-secondary)]">
              {target}
            </p>
          </div>
        )}
        {approval.reason && (
          <div>
            <span className="text-[var(--text-faint)]">原因</span>
            <p className="mt-0.5 leading-5 text-[var(--text-secondary)]">
              {approval.reason}
            </p>
          </div>
        )}

        <details className="approval-card-technical rounded-lg border border-[var(--border-soft)] bg-[var(--surface-muted)] px-3 py-2">
          <summary className="cursor-pointer select-none text-[var(--text-secondary)] hover:text-[var(--text-primary)]">
            查看技术详情
          </summary>
          <dl className="mt-2 space-y-2 font-mono text-[11px]">
            <div>
              <dt className="font-sans text-[var(--text-faint)]">Tool Name</dt>
              <dd className="mt-0.5 break-all text-[var(--text-secondary)]">
                {approval.tool_name}
              </dd>
            </div>
            <div>
              <dt className="font-sans text-[var(--text-faint)]">Arguments</dt>
              <dd className="mt-0.5 max-h-32 overflow-auto whitespace-pre-wrap break-all text-[var(--text-secondary)]">
                {maskedArgs}
              </dd>
            </div>
          </dl>
        </details>

        <div className="approval-card-actions flex items-center justify-end gap-2 pt-1">
          <Button
            size="sm"
            variant="secondary"
            disabled={resolving}
            onClick={() => onReject(approval)}
            aria-label="拒绝此次工具操作"
          >
            {resolving ? "提交中…" : "拒绝"}
          </Button>
          <Button
            size="sm"
            disabled={resolving}
            onClick={() => onApprove(approval)}
            aria-label="允许此次工具操作"
          >
            {resolving ? "提交中…" : "允许本次"}
          </Button>
        </div>
      </div>
    </section>
  );
}
