import { cn } from "./cn";

interface PanelProps extends React.HTMLAttributes<HTMLDivElement> {
  padding?: boolean;
}

export default function Panel({ className, padding = true, children, ...props }: PanelProps) {
  return (
    <div
      className={cn(
        "glass-surface shadow-card-token rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface)] transition-[background-color,border-color,box-shadow] duration-[var(--motion-fast)]",
        padding && "p-4",
        className
      )}
      {...props}
    >
      {children}
    </div>
  );
}
