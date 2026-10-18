import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "../ui/cn";

export type PageShellVariant = "standard" | "workspace";
export type PageShellMaxWidth = "reading" | "form" | "dashboard" | "none";

export interface PageShellProps extends Omit<HTMLAttributes<HTMLDivElement>, "title"> {
  title?: ReactNode;
  description?: ReactNode;
  status?: ReactNode;
  actions?: ReactNode;
  variant?: PageShellVariant;
  maxWidth?: PageShellMaxWidth;
  className?: string;
  headerClassName?: string;
  contentClassName?: string;
  children: ReactNode;
}

const MAX_WIDTH_MAP: Record<PageShellMaxWidth, string> = {
  reading: "max-w-[860px]",
  form: "max-w-[1180px]",
  dashboard: "max-w-[1600px]",
  none: "max-w-none w-full",
};

export function PageShellHeader({
  title,
  description,
  status,
  actions,
  className,
}: {
  title?: ReactNode;
  description?: ReactNode;
  status?: ReactNode;
  actions?: ReactNode;
  className?: string;
}) {
  if (!title && !description && !status && !actions) {
    return null;
  }

  return (
    <header
      className={cn(
        "page-header sticky top-0 z-10 flex min-h-14 shrink-0 items-center justify-between gap-4 border-b border-[var(--border-soft)] bg-[var(--surface)]/90 px-[var(--page-padding-inline)] py-2.5 backdrop-blur-md transition-colors",
        className,
      )}
    >
      <div className="flex min-w-0 flex-1 flex-col gap-0.5 sm:flex-row sm:items-center sm:gap-3">
        <div className="flex min-w-0 items-center gap-2.5">
          {typeof title === "string" ? (
            <h1 className="truncate text-lg font-semibold tracking-tight text-[var(--text-primary)] sm:text-xl">
              {title}
            </h1>
          ) : (
            title
          )}
          {status && <div className="shrink-0">{status}</div>}
        </div>
        {description && (
          typeof description === "string" ? (
            <p className="page-description truncate text-xs text-[var(--text-secondary)] sm:text-sm">
              {description}
            </p>
          ) : (
            description
          )
        )}
      </div>

      {actions && (
        <div className="flex shrink-0 items-center gap-2">
          {actions}
        </div>
      )}
    </header>
  );
}

export default function PageShell({
  title,
  description,
  status,
  actions,
  variant = "standard",
  maxWidth = "dashboard",
  className,
  headerClassName,
  contentClassName,
  children,
  ...props
}: PageShellProps) {
  const isWorkspace = variant === "workspace";

  return (
    <div
      className={cn(
        "page-canvas relative flex h-full w-full flex-col overflow-hidden bg-transparent",
        className,
      )}
      {...props}
    >
      <PageShellHeader
        title={title}
        description={description}
        status={status}
        actions={actions}
        className={headerClassName}
      />

      {isWorkspace ? (
        <main
          className={cn(
            "flex-1 min-h-0 overflow-hidden flex flex-col",
            contentClassName,
          )}
        >
          {children}
        </main>
      ) : (
        <main
          className={cn(
            "flex-1 min-h-0 overflow-y-auto px-[var(--page-padding-inline)] py-[var(--page-padding-block)]",
            contentClassName,
          )}
        >
          <div className={cn("mx-auto w-full", MAX_WIDTH_MAP[maxWidth])}>
            {children}
          </div>
        </main>
      )}
    </div>
  );
}

PageShell.Header = PageShellHeader;
