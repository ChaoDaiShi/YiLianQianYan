import { cn } from "./cn";

interface PageHeaderProps {
  title: string;
  description?: string;
  actions?: React.ReactNode;
  className?: string;
}

export default function PageHeader({ title, description, actions, className }: PageHeaderProps) {
  return (
    <div
      className={cn(
        "px-6 py-4 border-b border-[var(--border)] flex items-start justify-between gap-4 bg-[var(--panel)]/60 backdrop-blur-md",
        className
      )}
    >
      <div>
        <h2 className="font-semibold text-lg text-[var(--text)] tracking-tight">{title}</h2>
        {description && (
          <p className="text-sm text-[var(--text-muted)] mt-0.5">{description}</p>
        )}
      </div>
      {actions && <div className="flex items-center gap-2 flex-shrink-0">{actions}</div>}
    </div>
  );
}
