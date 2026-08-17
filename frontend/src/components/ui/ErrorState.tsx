import { AlertTriangle } from "lucide-react";
import { cn } from "./cn";

interface ErrorStateProps {
  title: string;
  description?: string;
  action?: React.ReactNode;
  className?: string;
}

export default function ErrorState({
  title,
  description,
  action,
  className,
}: ErrorStateProps) {
  return (
    <div
      role="alert"
      className={cn(
        "flex items-start gap-3 rounded-[var(--radius-lg)] border border-[var(--danger-border)] bg-[var(--danger-soft)] px-4 py-3",
        className
      )}
    >
      <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-[var(--danger)]" />
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium text-[var(--danger-fg)]">{title}</p>
        {description && (
          <p className="mt-1 text-xs leading-5 text-[var(--text-secondary)]">
            {description}
          </p>
        )}
        {action && <div className="mt-3">{action}</div>}
      </div>
    </div>
  );
}
