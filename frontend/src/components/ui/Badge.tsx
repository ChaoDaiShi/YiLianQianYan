import { type ReactNode, type ComponentType } from "react";
import { Shield, ShieldAlert, ShieldCheck, AlertTriangle } from "lucide-react";
import { cn } from "./cn";

export type BadgeTone = "default" | "success" | "warning" | "danger" | "info" | "accent" | "running";

const tones: Record<BadgeTone, string> = {
  default: "border-[var(--border-soft)] bg-[var(--surface-muted)] text-[var(--text-secondary)]",
  success: "border-[var(--success-border)] bg-[var(--success-soft)] text-[var(--success-fg)]",
  warning: "border-[var(--warning-border)] bg-[var(--warning-soft)] text-[var(--warning-fg)]",
  danger: "border-[var(--danger-border)] bg-[var(--danger-soft)] text-[var(--danger-fg)]",
  info: "border-[var(--info-border)] bg-[var(--info-soft)] text-[var(--info-fg)]",
  accent: "border-[var(--accent-border)] bg-[var(--accent-soft)] text-[var(--accent-soft-fg)]",
  running: "border-[var(--accent-primary)]/40 bg-[var(--accent-soft)] text-[var(--accent-primary)]",
};

export interface BadgeProps {
  tone?: BadgeTone;
  dot?: "static" | "pulse";
  className?: string;
  children: ReactNode;
}

export default function Badge({
  tone = "default",
  dot,
  className,
  children,
}: BadgeProps) {
  return (
    <span
      className={cn(
        "ui-badge inline-flex items-center gap-1.5 rounded-[var(--radius-sm)] border px-2 py-0.5 text-xs font-medium transition-colors duration-[var(--motion-fast)]",
        tones[tone],
        className
      )}
    >
      {dot && (
        <span
          className={cn(
            "h-1.5 w-1.5 rounded-full bg-current shrink-0",
            dot === "pulse" && "animate-pulse"
          )}
          aria-hidden="true"
        />
      )}
      {children}
    </span>
  );
}

export type RiskLevel = "low" | "medium" | "high" | "critical";

export interface RiskBadgeProps {
  risk: RiskLevel;
  className?: string;
  showIcon?: boolean;
}

const riskConfig: Record<
  RiskLevel,
  { label: string; tone: BadgeTone; icon: ComponentType<{ className?: string }> }
> = {
  low: { label: "低风险", tone: "info", icon: ShieldCheck },
  medium: { label: "中风险", tone: "warning", icon: Shield },
  high: { label: "高风险", tone: "danger", icon: AlertTriangle },
  critical: { label: "极高风险", tone: "danger", icon: ShieldAlert },
};

export function RiskBadge({ risk, className, showIcon = true }: RiskBadgeProps) {
  const conf = riskConfig[risk] ?? riskConfig.low;
  const Icon = conf.icon;

  return (
    <Badge
      tone={conf.tone}
      className={cn(
        risk === "critical" && "border-2 border-[var(--danger)] font-semibold shadow-sm",
        className
      )}
    >
      {showIcon && <Icon className="h-3 w-3 shrink-0" />}
      <span>{conf.label}</span>
    </Badge>
  );
}

export interface StatusBadgeProps {
  label: ReactNode;
  tone?: BadgeTone;
  icon?: ComponentType<{ className?: string }>;
  dot?: "static" | "pulse";
  className?: string;
}

export function StatusBadge({
  label,
  tone = "default",
  icon: Icon,
  dot,
  className,
}: StatusBadgeProps) {
  return (
    <Badge tone={tone} dot={dot} className={className}>
      {Icon && <Icon className="h-3 w-3 shrink-0" />}
      <span>{label}</span>
    </Badge>
  );
}
