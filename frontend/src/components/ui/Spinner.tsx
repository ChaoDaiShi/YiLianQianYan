import { cn } from "./cn";

export default function Spinner({ className }: { className?: string }) {
  return (
    <div
      className={cn(
        "w-5 h-5 border-2 border-[var(--border)] border-t-[var(--accent)] rounded-full animate-spin",
        className
      )}
      role="status"
      aria-label="加载中"
    />
  );
}
