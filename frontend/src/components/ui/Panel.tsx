import { cn } from "./cn";

interface PanelProps extends React.HTMLAttributes<HTMLDivElement> {
  padding?: boolean;
}

export default function Panel({ className, padding = true, children, ...props }: PanelProps) {
  return (
    <div
      className={cn(
        "rounded-xl border border-[var(--border)] bg-[var(--panel)] backdrop-blur-sm",
        padding && "p-4",
        className
      )}
      {...props}
    >
      {children}
    </div>
  );
}
