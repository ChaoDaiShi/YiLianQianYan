import { cn } from "./cn";

interface PanelProps extends React.HTMLAttributes<HTMLDivElement> {
  padding?: boolean;
}

export default function Panel({ className, padding = true, children, ...props }: PanelProps) {
  return (
    <div
      className={cn(
        "rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--panel)] shadow-[var(--shadow-soft)]",
        padding && "p-4",
        className
      )}
      {...props}
    >
      {children}
    </div>
  );
}
