import { cn } from "./cn";

export type SkeletonShape = "text" | "line" | "card" | "list" | "table";

export interface SkeletonProps {
  shape?: SkeletonShape;
  className?: string;
  count?: number;
}

export default function Skeleton({
  shape = "line",
  className,
  count = 1,
}: SkeletonProps) {
  const renderItem = (idx: number) => {
    switch (shape) {
      case "text":
        return (
          <div
            key={idx}
            className={cn(
              "h-3.5 w-3/4 rounded-[var(--radius-xs)] bg-[var(--surface-muted)] animate-pulse",
              className
            )}
          />
        );
      case "card":
        return (
          <div
            key={idx}
            className={cn(
              "h-28 w-full rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface-muted)] p-4 animate-pulse",
              className
            )}
          />
        );
      case "list":
        return (
          <div
            key={idx}
            className={cn(
              "flex h-12 w-full items-center gap-3 rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-muted)] px-3 animate-pulse",
              className
            )}
          >
            <div className="h-6 w-6 rounded-full bg-[var(--surface-hover)] shrink-0" />
            <div className="h-3 flex-1 rounded bg-[var(--surface-hover)]" />
            <div className="h-3 w-16 rounded bg-[var(--surface-hover)] shrink-0" />
          </div>
        );
      case "table":
        return (
          <div
            key={idx}
            className={cn(
              "flex h-10 w-full items-center gap-4 border-b border-[var(--border-soft)] px-4 animate-pulse",
              className
            )}
          >
            <div className="h-3 w-1/4 rounded bg-[var(--surface-muted)]" />
            <div className="h-3 w-1/3 rounded bg-[var(--surface-muted)]" />
            <div className="h-3 w-1/6 rounded bg-[var(--surface-muted)]" />
          </div>
        );
      default:
        return (
          <div
            key={idx}
            className={cn(
              "h-4 w-full rounded-[var(--radius-sm)] bg-[var(--surface-muted)] animate-pulse",
              className
            )}
          />
        );
    }
  };

  if (count > 1) {
    return (
      <div className="flex flex-col gap-2.5 w-full" aria-busy="true">
        {Array.from({ length: count }).map((_, i) => renderItem(i))}
      </div>
    );
  }

  return (
    <div aria-busy="true" className="w-full">
      {renderItem(0)}
    </div>
  );
}
