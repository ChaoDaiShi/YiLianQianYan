import { type ReactNode } from "react";
import { X } from "lucide-react";
import { cn } from "./cn";
import Tabs, { type TabItem } from "./Tabs";

export interface InspectorProps {
  title?: ReactNode;
  description?: ReactNode;
  badge?: ReactNode;
  actions?: ReactNode;
  onClose?: () => void;
  tabs?: {
    items: TabItem[];
    activeId: string;
    onChange: (id: string) => void;
  };
  emptyState?: ReactNode;
  footer?: ReactNode;
  children?: ReactNode;
  className?: string;
  headerClassName?: string;
  bodyClassName?: string;
}

export default function Inspector({
  title,
  description,
  badge,
  actions,
  onClose,
  tabs,
  emptyState,
  footer,
  children,
  className,
  headerClassName,
  bodyClassName,
}: InspectorProps) {
  if (!title && emptyState) {
    return (
      <div
        className={cn(
          "flex h-full w-full flex-col items-center justify-center rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-6",
          className
        )}
      >
        {emptyState}
      </div>
    );
  }

  return (
    <div
      className={cn(
        "flex h-full w-full flex-col overflow-hidden rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface-solid)] shadow-card-token transition-colors duration-[var(--motion-fast)]",
        className
      )}
    >
      {/* Header */}
      <div
        className={cn(
          "flex shrink-0 items-start justify-between gap-3 border-b border-[var(--border-soft)] p-4",
          headerClassName
        )}
      >
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            {typeof title === "string" ? (
              <h3 className="truncate text-base font-semibold text-[var(--text-primary)]">
                {title}
              </h3>
            ) : (
              title
            )}
            {badge}
          </div>
          {description && (
            <p className="mt-1 truncate text-xs text-[var(--text-secondary)]">
              {description}
            </p>
          )}
        </div>

        <div className="flex shrink-0 items-center gap-1.5">
          {actions}
          {onClose && (
            <button
              type="button"
              onClick={onClose}
              aria-label="关闭详情面板"
              className="rounded-[var(--radius-sm)] p-1 text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)] transition-colors"
            >
              <X className="h-4 w-4" />
            </button>
          )}
        </div>
      </div>

      {/* Tabs */}
      {tabs && (
        <div className="shrink-0 px-4 pt-2 border-b border-[var(--border-soft)] bg-[var(--surface-muted)]">
          <Tabs
            variant="underline"
            size="sm"
            tabs={tabs.items}
            activeId={tabs.activeId}
            onChange={tabs.onChange}
          />
        </div>
      )}

      {/* Scrollable Body */}
      <div className={cn("min-h-0 flex-1 overflow-y-auto p-4", bodyClassName)}>
        {children}
      </div>

      {/* Sticky Footer */}
      {footer && (
        <div className="flex shrink-0 items-center justify-end gap-2 border-t border-[var(--border-soft)] bg-[var(--surface-muted)] p-3">
          {footer}
        </div>
      )}
    </div>
  );
}
