import { cn } from "./cn";

type Tone = "default" | "success" | "warning" | "danger" | "accent";

const tones: Record<Tone, string> = {
  default: "bg-[var(--panel-2)] text-[var(--text-muted)] border-[var(--border)]",
  success: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30",
  warning: "bg-amber-500/15 text-amber-400 border-amber-500/30",
  danger: "bg-red-500/15 text-red-400 border-red-500/30",
  accent: "bg-[var(--accent)]/15 text-[var(--accent)] border-[var(--accent)]/30",
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
        "inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-xs border font-medium",
        tones[tone],
        className
      )}
    >
      {children}
    </span>
  );
}
