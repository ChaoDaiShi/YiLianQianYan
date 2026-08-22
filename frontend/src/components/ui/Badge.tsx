import { cn } from "./cn";

type Tone = "default" | "success" | "warning" | "danger" | "info" | "accent";

const tones: Record<Tone, string> = {
  default: "border-[var(--border-soft)] bg-[var(--surface-muted)] text-[var(--text-secondary)]",
  success: "border-[var(--success-border)] bg-[var(--success-soft)] text-[var(--success-fg)]",
  warning: "border-[var(--warning-border)] bg-[var(--warning-soft)] text-[var(--warning-fg)]",
  danger: "border-[var(--danger-border)] bg-[var(--danger-soft)] text-[var(--danger-fg)]",
  info: "border-[var(--info-border)] bg-[var(--info-soft)] text-[var(--info-fg)]",
  accent: "border-[var(--accent-border)] bg-[var(--accent-soft)] text-[var(--accent-soft-fg)]",
};

export default function Badge({
  tone = "default",
  className,
  children,
}: {
  tone?: Tone;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-[var(--radius-sm)] border px-2 py-0.5 text-xs font-medium transition-colors duration-[var(--motion-fast)]",
        tones[tone],
        className
      )}
    >
      {children}
    </span>
  );
}
