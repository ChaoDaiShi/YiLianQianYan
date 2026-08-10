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
        "mx-4 mt-4 px-5 py-4 rounded-[var(--radius-md)] border border-[var(--border)] flex items-start justify-between gap-4 bg-[var(--panel)] shadow-[var(--shadow-soft)]",
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
