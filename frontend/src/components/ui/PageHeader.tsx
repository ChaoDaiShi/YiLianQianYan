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
        "page-header shadow-card-token mx-4 mt-4 flex items-start justify-between gap-4 rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface)] px-5 py-4 transition-[background-color,border-color,box-shadow] duration-[var(--motion-fast)]",
        className
      )}
    >
      <div className="min-w-0">
        <h2 className="page-title font-semibold text-[var(--text)] tracking-tight">{title}</h2>
        {description && (
          <p className="page-description text-[var(--text-muted)] mt-0.5">{description}</p>
        )}
      </div>
      {actions && <div className="page-header-actions flex items-center gap-2">{actions}</div>}
    </div>
  );
}
