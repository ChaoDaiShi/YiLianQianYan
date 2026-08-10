import { ShieldAlert, ShieldCheck } from "lucide-react";
import type { PendingApproval } from "../../types/approval";
import { Badge, Button } from "../ui";

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
      SENSITIVE_KEYS.some((sensitive) =>
        key.toLowerCase().includes(sensitive)
      )
        ? "••••••••"
        : value,
    ])
  );
}

function riskBadgeTone(risk: string): "danger" | "warning" | "accent" {
  if (risk === "critical") return "danger";
  if (risk === "high") return "warning";
  return "accent";
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

export default function ApprovalCard({
  approval,
  resolving = false,
  onApprove,
  onReject,
}: ApprovalCardProps) {
  const isCritical = approval.risk_level === "critical";

  return (
    <div className="overflow-hidden rounded-xl border border-[var(--warning)]/40 bg-[var(--panel)]">
      <div className="flex items-center gap-2 border-b border-[var(--border)] bg-[var(--warning)]/10 px-3 py-2.5">
        {isCritical ? (
          <ShieldAlert className="h-4 w-4 text-[var(--danger)]" />
        ) : (
          <ShieldCheck className="h-4 w-4 text-[var(--warning)]" />
        )}
        <span className="text-sm font-semibold">需要你的确认</span>
        <Badge tone={riskBadgeTone(approval.risk_level)} className="ml-auto">
          {riskLabel(approval.risk_level)}
        </Badge>
      </div>

      <div className="space-y-3 px-3 py-3 text-xs">
        <div>
          <span className="text-[var(--text-muted)]">工具：</span>
          <span className="font-mono font-semibold">{approval.tool_name}</span>
        </div>
        <div>
          <span className="text-[var(--text-muted)]">原因：</span>
          <span>{approval.reason || "该操作需要明确授权"}</span>
        </div>
        <div>
          <span className="text-[var(--text-muted)]">参数：</span>
          <pre className="mt-1 max-h-32 overflow-auto whitespace-pre-wrap rounded-lg border border-[var(--border)] bg-[var(--input-bg)] p-2 font-mono">
            {JSON.stringify(maskArgs(approval.arguments || {}), null, 2)}
          </pre>
        </div>

        <div className="flex items-center gap-2 pt-1">
          <Button
            size="sm"
            disabled={resolving}
            onClick={() => onApprove(approval)}
          >
            {resolving ? "提交中…" : "允许本次"}
          </Button>
          <Button
            size="sm"
            variant="secondary"
            disabled={resolving}
            onClick={() => onReject(approval)}
          >
            {resolving ? "提交中…" : "拒绝"}
          </Button>
        </div>
      </div>
    </div>
  );
}
