import { cn } from "./cn";

type Tone = "default" | "success" | "warning" | "danger" | "info" | "accent";

const tones: Record<Tone, string> = {
  default: "border-[var(--border-soft)] bg-[var(--surface-muted)] text-[var(--text-secondary)]",
  success: "border-[var(--success)]/30 bg-[var(--success)]/12 text-[var(--success)]",
  warning: "border-[var(--warning)]/30 bg-[var(--warning)]/12 text-[var(--warning)]",
  danger: "border-[var(--danger)]/30 bg-[var(--danger)]/12 text-[var(--danger)]",
  info: "border-[var(--info)]/30 bg-[var(--info)]/12 text-[var(--info)]",
  accent: "border-[var(--accent-primary)]/25 bg-[var(--accent-soft)] text-[var(--accent-primary)]",
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
