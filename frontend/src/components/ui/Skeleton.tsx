import { cn } from "./cn";

export default function Skeleton({ className }: { className?: string }) {
  return (
    <div
      className={cn(
        "animate-pulse rounded-[var(--radius-sm)] bg-[var(--surface-muted)]",
        className
      )}
      aria-hidden="true"
    />
  );
}
