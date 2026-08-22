import { cn } from "./cn";

export default function Spinner({ className }: { className?: string }) {
  return (
    <div
      className={cn(
        "h-5 w-5 animate-spin rounded-full border-2 border-[var(--border-soft)] border-t-[var(--accent-primary)]",
        className
      )}
      role="status"
      aria-label="加载中"
    />
  );
}
