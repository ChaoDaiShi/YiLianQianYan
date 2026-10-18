import { cn } from "./cn";

export type PanelDensity = "compact" | "standard" | "relaxed";

export interface PanelProps extends React.HTMLAttributes<HTMLDivElement> {
  padding?: boolean;
  density?: PanelDensity;
  interactive?: boolean;
}

const densityPaddings: Record<PanelDensity, string> = {
  compact: "p-3.5",
  standard: "p-5",
  relaxed: "p-6",
};

export default function Panel({
  className,
  padding = true,
  density = "standard",
  interactive = false,
  children,
  ...props
}: PanelProps) {
  return (
    <div
      className={cn(
        "ui-panel min-w-0 glass-surface shadow-card-token rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface)] transition-[background-color,border-color,box-shadow,transform] duration-[var(--motion-fast)]",
        "shadow-[inset_0_1px_0_0_rgba(255,255,255,0.65),var(--shadow-card)]",
        interactive && "cursor-pointer hover:border-[var(--accent-border)] hover:-translate-y-0.5 hover:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.8),var(--cyrene-shadow-lg)] active:translate-y-0",
        padding && densityPaddings[density],
        className
      )}
      {...props}
    >
      {children}
    </div>
  );
}
