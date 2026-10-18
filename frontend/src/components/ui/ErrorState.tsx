import { useState, type ReactNode } from "react";
import { AlertTriangle, AlertCircle, ChevronDown, RotateCw } from "lucide-react";
import { cn } from "./cn";
import Button from "./Button";

export type ErrorSeverity = "error" | "warning";

export interface ErrorStateProps {
  title: string;
  description?: string;
  details?: string;
  severity?: ErrorSeverity;
  onRetry?: () => void;
  action?: ReactNode;
  className?: string;
  role?: "alert" | "status";
}

export default function ErrorState({
  title,
  description,
  details,
  severity = "error",
  onRetry,
  action,
  className,
  role = "alert",
}: ErrorStateProps) {
  const [showDetails, setShowDetails] = useState(false);

  const isWarning = severity === "warning";

  return (
    <div
      role={role}
      className={cn(
        "flex items-start gap-3 rounded-[var(--radius-lg)] border p-4 transition-colors duration-[var(--motion-fast)]",
        isWarning
          ? "border-[var(--warning-border)] bg-[var(--warning-soft)] text-[var(--warning-fg)]"
          : "border-[var(--danger-border)] bg-[var(--danger-soft)] text-[var(--danger-fg)]",
        className
      )}
    >
      <div className="shrink-0 mt-0.5">
        {isWarning ? (
          <AlertCircle className="h-4 w-4 text-[var(--warning)]" />
        ) : (
          <AlertTriangle className="h-4 w-4 text-[var(--danger)]" />
        )}
      </div>

      <div className="min-w-0 flex-1">
        <p className="text-sm font-semibold tracking-wide">{title}</p>
        {description && (
          <p className="mt-1 text-xs leading-relaxed text-[var(--text-secondary)]">
            {description}
          </p>
        )}

        {details && (
          <div className="mt-2.5">
            <button
              type="button"
              onClick={() => setShowDetails(!showDetails)}
              aria-expanded={showDetails}
              className="inline-flex items-center gap-1 text-xs font-medium text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
            >
              <ChevronDown
                className={cn(
                  "h-3.5 w-3.5 transition-transform duration-[var(--motion-fast)]",
                  showDetails && "rotate-180"
                )}
              />
              <span>{showDetails ? "收起详细信息" : "展开详细信息"}</span>
            </button>

            {showDetails && (
              <pre className="mt-2 max-h-48 overflow-auto rounded-[var(--radius-sm)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2.5 font-mono text-[11px] text-[var(--text-primary)] leading-normal whitespace-pre-wrap break-all scrollbar-thin">
                {details}
              </pre>
            )}
          </div>
        )}

        {(onRetry || action) && (
          <div className="mt-3 flex items-center gap-2">
            {onRetry && (
              <Button
                size="sm"
                variant="secondary"
                onClick={onRetry}
                className="h-7 text-xs"
              >
                <RotateCw className="mr-1 h-3 w-3" />
                重试
              </Button>
            )}
            {action}
          </div>
        )}
      </div>
    </div>
  );
}
