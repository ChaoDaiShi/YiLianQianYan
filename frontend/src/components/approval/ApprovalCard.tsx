import { ShieldAlert, ShieldCheck } from "lucide-react";
import type { PendingApproval } from "../../types/approval";
import { Badge } from "../ui";
import Button from "../ui/Button";

interface ApprovalCardProps {
  approval: PendingApproval;
  resolving?: boolean;
  onApprove: (approval: PendingApproval) => void;
  onReject: (approval: PendingApproval) => void;
}

/** Field names whose values should be masked when displayed. */
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
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(args)) {
    const key = k.toLowerCase();
    out[k] = SENSITIVE_KEYS.some((s) => key.includes(s))
      ? "••••••••"
      : v;
  }
  return out;
}

function riskBadgeTone(risk: string): "danger" | "warning" | "accent" {
  if (risk === "critical") return "danger";
  if (risk === "high") return "warning";
  return "accent";
}

function riskLabel(risk: string): string {
  switch (risk) {
    case "critical":
      return "CRITICAL";
    case "high":
      return "HIGH";
    case "medium":
      return "MEDIUM";
    default:
      return risk.toUpperCase();
  }
}

export default function ApprovalCard({
  approval,
  resolving = false,
  onApprove,
  onReject,
}: ApprovalCardProps) {
  const isCritical = approval.risk_level === "critical";
  const masked = maskArgs(approval.arguments || {});

  return (
    <div
      className={`mb-2 ml-4 rounded-xl border overflow-hidden ${
        isCritical ? "border-red-500/40" : "border-amber-500/40"
      } bg-[var(--panel)]/60`}
    >
      <div
        className={`flex items-center gap-2 px-3 py-2 text-sm ${
          isCritical ? "bg-red-500/10" : "bg-amber-500/10"
        }`}
      >
        {isCritical ? (
          <ShieldAlert className="w-4 h-4 text-red-400" />
        ) : (
          <ShieldCheck className="w-4 h-4 text-amber-400" />
        )}
        <span className="font-medium">需要用户批准</span>
        <Badge tone={riskBadgeTone(approval.risk_level)}>
          {riskLabel(approval.risk_level)}
        </Badge>
      </div>

      <div className="px-3 py-2 text-xs space-y-2">
        <div>
          <span className="text-[var(--text-muted)]">工具：</span>
          <span className="font-mono font-medium">{approval.tool_name}</span>
        </div>
        <div>
          <span className="text-[var(--text-muted)]">原因：</span>
          <span>{approval.reason || "高风险操作"}</span>
        </div>

        <div>
          <span className="text-[var(--text-muted)]">参数：</span>
          <pre className="mt-1 p-2 rounded bg-[var(--input-bg)] border border-[var(--border)] overflow-x-auto max-h-32 overflow-y-auto font-mono whitespace-pre-wrap">
            {JSON.stringify(masked, null, 2)}
          </pre>
        </div>

        <div className="flex items-center gap-2 pt-1">
          <Button
            size="sm"
            variant={isCritical ? "primary" : "primary"}
            disabled={resolving}
            onClick={() => onApprove(approval)}
          >
            {resolving ? "处理中…" : "允许本次"}
          </Button>
          <Button
            size="sm"
            variant="secondary"
            disabled={resolving}
            onClick={() => onReject(approval)}
          >
            {resolving ? "处理中…" : "拒绝"}
          </Button>
          <span className="text-[10px] text-[var(--text-faint)] ml-auto">
            当前版本尚未执行，等待你的决定
          </span>
        </div>
      </div>
    </div>
  );
}
