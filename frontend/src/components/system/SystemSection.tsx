import type { ReactNode } from "react";

export default function SystemSection({
  title,
  description,
  children,
  className = "",
}: {
  title: string;
  description?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={`system-section ${className}`} aria-labelledby={`system-section-${title}`}>
      <div className="mb-3 flex items-end justify-between gap-4">
        <div>
          <h2 id={`system-section-${title}`} className="text-sm font-semibold text-[var(--text-primary)]">{title}</h2>
          {description ? <p className="mt-1 text-xs text-[var(--text-secondary)]">{description}</p> : null}
        </div>
      </div>
      {children}
    </section>
  );
}
