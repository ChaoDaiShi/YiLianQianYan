import { type ReactNode } from "react";
import { RotateCw } from "lucide-react";
import { cn } from "./cn";
import SearchField from "./SearchField";

export interface FilterBarCount {
  total: number;
  filtered?: number;
  label?: string;
}

export interface FilterBarProps {
  search?: {
    value: string;
    onChange: (value: string) => void;
    placeholder?: string;
    onClear?: () => void;
  };
  filters?: ReactNode;
  count?: number | FilterBarCount;
  onRefresh?: () => void;
  refreshing?: boolean;
  actions?: ReactNode;
  className?: string;
}

export default function FilterBar({
  search,
  filters,
  count,
  onRefresh,
  refreshing = false,
  actions,
  className,
}: FilterBarProps) {
  const renderCount = () => {
    if (count === undefined) return null;
    if (typeof count === "number") {
      return (
        <span className="text-xs text-[var(--text-secondary)] font-mono tabular-nums">
          共 {count} 项
        </span>
      );
    }
    const { total, filtered, label = "项" } = count;
    if (filtered !== undefined && filtered !== total) {
      return (
        <span className="text-xs text-[var(--text-secondary)] font-mono tabular-nums">
          显示 {filtered} / {total} {label}
        </span>
      );
    }
    return (
      <span className="text-xs text-[var(--text-secondary)] font-mono tabular-nums">
        共 {total} {label}
      </span>
    );
  };

  return (
    <div
      className={cn(
        "flex flex-wrap items-center justify-between gap-3 rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface)] p-3 transition-colors duration-[var(--motion-fast)]",
        className
      )}
    >
      <div className="flex flex-1 flex-wrap items-center gap-2.5 min-w-[240px]">
        {search && (
          <div className="w-full sm:w-64">
            <SearchField
              size="sm"
              value={search.value}
              onChange={search.onChange}
              onClear={search.onClear}
              placeholder={search.placeholder ?? "搜索..."}
            />
          </div>
        )}
        {filters && <div className="flex flex-wrap items-center gap-2">{filters}</div>}
      </div>

      <div className="flex shrink-0 items-center gap-3">
        {renderCount()}

        {onRefresh && (
          <button
            type="button"
            onClick={onRefresh}
            disabled={refreshing}
            aria-label="刷新"
            className="inline-flex h-8 w-8 items-center justify-center rounded-[var(--radius-sm)] border border-[var(--border-soft)] bg-[var(--surface-solid)] text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-outline)] disabled:opacity-50 transition-colors"
          >
            <RotateCw
              className={cn(
                "h-3.5 w-3.5 transition-transform duration-[var(--motion-fast)]",
                refreshing && "animate-spin"
              )}
            />
          </button>
        )}

        {actions}
      </div>
    </div>
  );
}
