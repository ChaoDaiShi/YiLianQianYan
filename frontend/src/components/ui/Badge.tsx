import { cn } from "./cn";

export type BadgeTone = "default" | "success" | "warning" | "danger" | "info" | "accent";

const tones: Record<BadgeTone, string> = {
  default: "border-[var(--border-soft)] bg-[var(--surface-muted)] text-[var(--text-secondary)]",
  success: "border-[var(--success-border)] bg-[var(--success-soft)] text-[var(--success-fg)]",
  warning: "border-[var(--warning-border)] bg-[var(--warning-soft)] text-[var(--warning-fg)]",
  danger: "border-[var(--danger-border)] bg-[var(--danger-soft)] text-[var(--danger-fg)]",
  info: "border-[var(--info-border)] bg-[var(--info-soft)] text-[var(--info-fg)]",
  accent: "border-[var(--accent-border)] bg-[var(--accent-soft)] text-[var(--accent-soft-fg)]",
};

export interface BadgeProps {
  tone?: BadgeTone;
  dot?: "static" | "pulse";
  className?: string;
  children: React.ReactNode;
}

export default function Badge({
  tone = "default",
  dot,
  className,
  children,
}: BadgeProps) {
  return (
    <span
      className={cn(
        "ui-badge inline-flex items-center gap-1.5 rounded-[var(--radius-sm)] border px-2 py-0.5 text-xs font-medium transition-colors duration-[var(--motion-fast)]",
        tones[tone],
        className
      )}
    >
      {dot && (
        <span
          className={cn(
            "h-1.5 w-1.5 rounded-full bg-current shrink-0",
            dot === "pulse" && "animate-pulse"
          )}
          aria-hidden="true"
        />
      )}
      {children}
    </span>
  );
}
