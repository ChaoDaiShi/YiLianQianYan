import { cn } from "./cn";

export type ProgressBarTone = "default" | "success" | "warning" | "danger" | "info";

export interface ProgressBarProps {
  value: number; // 0 to 100
  variant?: "bar" | "ring";
  tone?: ProgressBarTone;
  size?: "sm" | "md" | "lg";
  showValue?: boolean;
  className?: string;
  "aria-label"?: string;
}

const toneBarStyles: Record<ProgressBarTone, string> = {
  default: "bg-[var(--accent-primary)]",
  success: "bg-[var(--success)]",
  warning: "bg-[var(--warning)]",
  danger: "bg-[var(--danger)]",
  info: "bg-[var(--info)]",
};

const toneStrokeStyles: Record<ProgressBarTone, string> = {
  default: "stroke-[var(--accent-primary)]",
  success: "stroke-[var(--success)]",
  warning: "stroke-[var(--warning)]",
  danger: "stroke-[var(--danger)]",
  info: "stroke-[var(--info)]",
};

export default function ProgressBar({
  value,
  variant = "bar",
  tone = "default",
  size = "md",
  showValue = false,
  className,
  "aria-label": ariaLabel,
}: ProgressBarProps) {
  const clamped = Math.max(0, Math.min(100, Math.round(value)));

  if (variant === "ring") {
    const dimMap = { sm: 32, md: 48, lg: 64 };
    const strokeWidthMap = { sm: 3, md: 4, lg: 5 };
    const dim = dimMap[size];
    const strokeWidth = strokeWidthMap[size];
    const radius = (dim - strokeWidth) / 2;
    const circumference = 2 * Math.PI * radius;
    const strokeDashoffset = circumference - (clamped / 100) * circumference;

    return (
      <div
        role="progressbar"
        aria-valuenow={clamped}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={ariaLabel}
        className={cn("relative inline-flex items-center justify-center shrink-0", className)}
        style={{ width: dim, height: dim }}
      >
        <svg className="h-full w-full -rotate-90" viewBox={`0 0 ${dim} ${dim}`}>
          {/* Background circle */}
          <circle
            cx={dim / 2}
            cy={dim / 2}
            r={radius}
            fill="none"
            strokeWidth={strokeWidth}
            className="stroke-[var(--surface-muted)]"
          />
          {/* Progress circle */}
          <circle
            cx={dim / 2}
            cy={dim / 2}
            r={radius}
            fill="none"
            strokeWidth={strokeWidth}
            strokeDasharray={circumference}
            strokeDashoffset={strokeDashoffset}
            strokeLinecap="round"
            className={cn("transition-all duration-[var(--motion-normal)]", toneStrokeStyles[tone])}
          />
        </svg>
        {showValue && (
          <span className="absolute text-[10px] font-mono font-medium text-[var(--text-primary)] tabular-nums">
            {clamped}%
          </span>
        )}
      </div>
    );
  }

  // Bar variant
  const heightMap = { sm: "h-1.5", md: "h-2.5", lg: "h-4" };

  return (
    <div className={cn("w-full flex items-center gap-2.5", className)}>
      <div
        role="progressbar"
        aria-valuenow={clamped}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={ariaLabel}
        className={cn(
          "w-full overflow-hidden rounded-full bg-[var(--surface-muted)] border border-[var(--border-soft)]",
          heightMap[size]
        )}
      >
        <div
          className={cn(
            "h-full rounded-full transition-all duration-[var(--motion-normal)] ease-out",
            toneBarStyles[tone]
          )}
          style={{ width: `${clamped}%` }}
        />
      </div>
      {showValue && (
        <span className="text-xs font-mono font-medium text-[var(--text-secondary)] tabular-nums shrink-0">
          {clamped}%
        </span>
      )}
    </div>
  );
}
