import type { HTMLAttributes, KeyboardEvent } from "react";
import { cn } from "./cn";

export type PanelDensity = "compact" | "standard" | "relaxed";
export type PanelVariant = "surface" | "solid";

export interface PanelProps extends HTMLAttributes<HTMLDivElement> {
  padding?: boolean;
  density?: PanelDensity;
  interactive?: boolean;
  variant?: PanelVariant;
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
  variant = "surface",
  onClick,
  onKeyDown,
  children,
  ...props
}: PanelProps) {
  const handleKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (interactive && (e.key === "Enter" || e.key === " ")) {
      e.preventDefault();
      onClick?.(e as unknown as React.MouseEvent<HTMLDivElement>);
    }
    onKeyDown?.(e);
  };

  return (
    <div
      role={interactive ? "button" : undefined}
      tabIndex={interactive ? 0 : undefined}
      onClick={onClick}
      onKeyDown={interactive ? handleKeyDown : onKeyDown}
      className={cn(
        "ui-panel min-w-0 rounded-[var(--radius-lg)] border border-[var(--border-soft)] shadow-card-token transition-[background-color,border-color,box-shadow,transform] duration-[var(--motion-fast)]",
        variant === "solid" ? "bg-[var(--surface-solid)]" : "glass-surface bg-[var(--surface)]",
        interactive &&
          "cursor-pointer hover:border-[var(--accent-border)] hover:-translate-y-0.5 hover:shadow-[var(--shadow-float)] active:translate-y-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-outline)]",
        padding && densityPaddings[density],
        className
      )}
      {...props}
    >
      {children}
    </div>
  );
}
