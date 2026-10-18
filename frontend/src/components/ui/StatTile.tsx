import { type ReactNode } from "react";
import { cn } from "./cn";
import ProgressBar, { type ProgressBarTone } from "./ProgressBar";

export interface StatTileProps {
  label: ReactNode;
  value: ReactNode;
  unit?: string;
  description?: ReactNode;
  icon?: React.ComponentType<{ className?: string }>;
  progress?: number;
  progressVariant?: "bar" | "ring";
  tone?: ProgressBarTone;
  badge?: ReactNode;
  className?: string;
}

export default function StatTile({
  label,
  value,
  unit,
  description,
  icon: Icon,
  progress,
  progressVariant = "bar",
  tone = "default",
  badge,
  className,
}: StatTileProps) {
  return (
    <div
      className={cn(
        "flex flex-col justify-between rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-4 shadow-card-token transition-all duration-[var(--motion-fast)]",
        className
      )}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="flex items-center gap-2">
          {Icon && (
            <div className="rounded-[var(--radius-sm)] bg-[var(--surface-muted)] p-1.5 text-[var(--text-secondary)]">
              <Icon className="h-4 w-4" />
            </div>
          )}
          <span className="text-xs font-medium text-[var(--text-secondary)]">
            {label}
          </span>
        </div>
        {badge}
      </div>

      <div className="mt-3 flex items-baseline justify-between gap-2">
        <div className="flex items-baseline gap-1.5">
          <span className="font-mono text-2xl font-semibold tracking-tight text-[var(--text-primary)] tabular-nums">
            {value}
          </span>
          {unit && (
            <span className="text-xs font-medium text-[var(--text-secondary)]">
              {unit}
            </span>
          )}
        </div>

        {progress !== undefined && progressVariant === "ring" && (
          <ProgressBar
            value={progress}
            variant="ring"
            size="sm"
            tone={tone}
            showValue
          />
        )}
      </div>

      {progress !== undefined && progressVariant === "bar" && (
        <div className="mt-3">
          <ProgressBar value={progress} variant="bar" size="sm" tone={tone} />
        </div>
      )}

      {description && (
        <p className="mt-2 text-xs text-[var(--text-secondary)]">
          {description}
        </p>
      )}
    </div>
  );
}
