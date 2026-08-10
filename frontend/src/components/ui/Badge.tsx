import { cn } from "./cn";

type Tone = "default" | "success" | "warning" | "danger" | "info" | "accent";

const tones: Record<Tone, string> = {
  default: "bg-[var(--panel-2)] text-[var(--text-muted)] border-[var(--border)]",
  success: "bg-[var(--success)]/12 text-[var(--success)] border-[var(--success)]/30",
  warning: "bg-[var(--warning)]/12 text-[var(--warning)] border-[var(--warning)]/30",
  danger: "bg-[var(--danger)]/12 text-[var(--danger)] border-[var(--danger)]/30",
  info: "bg-[var(--info)]/12 text-[var(--info)] border-[var(--info)]/30",
  accent: "bg-[var(--accent)]/12 text-[var(--accent)] border-[var(--accent)]/30",
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
        "inline-flex items-center gap-1 px-2 py-0.5 rounded-[var(--radius-md)] text-xs border font-medium",
        tones[tone],
        className
      )}
    >
      {children}
    </span>
  );
}
