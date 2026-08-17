import { cn } from "./cn";

interface EmptyStateProps {
  icon?: React.ReactNode;
  title: string;
  description?: string;
  action?: React.ReactNode;
  className?: string;
}

export default function EmptyState({ icon, title, description, action, className }: EmptyStateProps) {
  return (
    <div className={cn("flex flex-col items-center justify-center py-16 px-6 text-center", className)}>
      {icon && (
        <div className="mb-4 rounded-[var(--radius-lg)] bg-[var(--accent-soft)] p-3 text-[var(--accent-primary)]">
          {icon}
        </div>
      )}
      <h3 className="text-base font-medium text-[var(--text)]">{title}</h3>
      {description && (
        <p className="mt-1.5 max-w-sm text-sm text-[var(--text-secondary)]">{description}</p>
      )}
      {action && <div className="mt-5">{action}</div>}
    </div>
  );
}
