import { cn } from "./cn";

export type PanelDensity = "compact" | "standard" | "relaxed";

export interface PanelProps extends React.HTMLAttributes<HTMLDivElement> {
  padding?: boolean;
  density?: PanelDensity;
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
  children,
  ...props
}: PanelProps) {
  return (
    <div
      className={cn(
        "ui-panel min-w-0 glass-surface shadow-card-token rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface)] transition-[background-color,border-color,box-shadow] duration-[var(--motion-fast)]",
        padding && densityPaddings[density],
        className
      )}
      {...props}
    >
      {children}
    </div>
  );
}
