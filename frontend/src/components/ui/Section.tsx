import { useState, type ReactNode } from "react";
import { ChevronRight } from "lucide-react";
import { cn } from "./cn";

export interface SectionProps {
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  collapsible?: boolean;
  defaultOpen?: boolean;
  className?: string;
  headerClassName?: string;
  children: ReactNode;
}

export default function Section({
  title,
  description,
  actions,
  collapsible = false,
  defaultOpen = true,
  className,
  headerClassName,
  children,
}: SectionProps) {
  const [open, setOpen] = useState(defaultOpen);

  return (
    <section
      className={cn(
        "rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface)] transition-[border-color,background-color] duration-[var(--motion-fast)]",
        className,
      )}
    >
      <div
        className={cn(
          "flex items-center justify-between gap-3 border-b border-[var(--border-soft)] px-5 py-3.5",
          headerClassName,
        )}
      >
        <div className="flex min-w-0 items-center gap-2.5">
          {collapsible && (
            <button
              type="button"
              onClick={() => setOpen((prev) => !prev)}
              aria-expanded={open}
              aria-label={open ? "收起分区" : "展开分区"}
              className="rounded p-0.5 text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)] transition-transform duration-[var(--motion-fast)]"
            >
              <ChevronRight
                className={cn(
                  "h-4 w-4 transition-transform duration-[var(--motion-fast)]",
                  open && "rotate-90",
                )}
              />
            </button>
          )}
          <div className="min-w-0">
            {typeof title === "string" ? (
              <h3 className="text-sm font-semibold tracking-wide text-[var(--text-primary)]">
                {title}
              </h3>
            ) : (
              title
            )}
            {description && (
              <p className="mt-0.5 text-xs text-[var(--text-secondary)]">
                {description}
              </p>
            )}
          </div>
        </div>

        {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
      </div>

      {(!collapsible || open) && <div className="p-5">{children}</div>}
    </section>
  );
}
