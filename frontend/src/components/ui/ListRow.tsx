import { type KeyboardEvent, type ReactNode } from "react";
import { cn } from "./cn";

export type ListRowStatusBarTone = "default" | "success" | "warning" | "danger" | "info" | "running";

export interface ListRowProps {
  selected?: boolean;
  onSelect?: () => void;
  onActivate?: () => void;
  leading?: ReactNode;
  title: ReactNode;
  meta?: ReactNode;
  trailing?: ReactNode;
  statusBar?: ListRowStatusBarTone | string;
  disabled?: boolean;
  className?: string;
  role?: "option" | "button" | "listitem";
  "aria-label"?: string;
}

const statusBarTones: Record<ListRowStatusBarTone, string> = {
  default: "bg-[var(--border-strong)]",
  success: "bg-[var(--success)]",
  warning: "bg-[var(--warning)]",
  danger: "bg-[var(--danger)]",
  info: "bg-[var(--info)]",
  running: "bg-[var(--status-running)]",
};

export default function ListRow({
  selected = false,
  onSelect,
  onActivate,
  leading,
  title,
  meta,
  trailing,
  statusBar,
  disabled = false,
  className,
  role = "option",
  "aria-label": ariaLabel,
}: ListRowProps) {
  const handleKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (disabled) return;
    if (e.key === "Enter") {
      e.preventDefault();
      if (onActivate) {
        onActivate();
      } else {
        onSelect?.();
      }
    } else if (e.key === " ") {
      e.preventDefault();
      onSelect?.();
    }
  };

  const statusColorClass = statusBar
    ? (statusBarTones[statusBar as ListRowStatusBarTone] ?? statusBar)
    : null;

  return (
    <div
      role={role}
      tabIndex={disabled ? -1 : 0}
      aria-selected={role === "option" ? selected : undefined}
      aria-current={role !== "option" && selected ? "true" : undefined}
      aria-disabled={disabled || undefined}
      aria-label={ariaLabel}
      onClick={disabled ? undefined : onSelect}
      onKeyDown={handleKeyDown}
      className={cn(
        "group relative flex items-center justify-between gap-3 rounded-[var(--radius-md)] border p-3 text-left transition-[background-color,border-color,box-shadow] duration-[var(--motion-fast)] select-none",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-outline)]",
        selected
          ? "border-[var(--accent-primary)] bg-[var(--accent-soft)] shadow-sm"
          : "border-[var(--border-soft)] bg-[var(--surface-solid)] hover:border-[var(--border-strong)] hover:bg-[var(--surface-hover)]",
        disabled && "pointer-events-none opacity-50 bg-[var(--surface-muted)]",
        className
      )}
    >
      {/* Left colored status bar indicator */}
      {statusColorClass && (
        <span
          className={cn(
            "absolute left-0 top-2 bottom-2 w-1 rounded-r-full transition-colors",
            statusColorClass
          )}
          aria-hidden="true"
        />
      )}

      <div className="flex min-w-0 flex-1 items-center gap-3">
        {leading && <div className="shrink-0">{leading}</div>}

        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            {typeof title === "string" ? (
              <span className="truncate text-sm font-medium text-[var(--text-primary)]">
                {title}
              </span>
            ) : (
              title
            )}
          </div>
          {meta && (
            <div className="mt-1 flex items-center gap-2 text-xs text-[var(--text-secondary)]">
              {meta}
            </div>
          )}
        </div>
      </div>

      {trailing && <div className="flex shrink-0 items-center gap-2">{trailing}</div>}
    </div>
  );
}
