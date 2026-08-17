import { cn } from "./cn";

interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  label?: string;
  hint?: string;
}

export default function Input({ label, hint, className, id, ...props }: InputProps) {
  const inputId = id || (label ? `input-${label}` : undefined);
  return (
    <div className="w-full">
      {label && (
        <label htmlFor={inputId} className="block text-sm font-medium text-[var(--text)] mb-1.5">
          {label}
        </label>
      )}
      <input
        id={inputId}
        className={cn(
          "w-full rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] px-3 py-2 text-sm text-[var(--text)] transition-colors duration-[var(--motion-fast)]",
          "placeholder:text-[var(--text-secondary)] focus:border-[var(--accent-primary)] focus:outline-none focus:ring-2 focus:ring-[var(--accent-primary)]/30",
          className
        )}
        {...props}
      />
      {hint && <p className="mt-1 text-xs text-[var(--text-muted)]">{hint}</p>}
    </div>
  );
}
