import { cn } from "./cn";
import { useId } from "react";

export interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  label?: string;
  hint?: string;
  error?: string;
}

export default function Input({ label, hint, error, className, id, ...props }: InputProps) {
  const generatedId = useId();
  const inputId = id || generatedId;
  const hintId = `${inputId}-hint`;
  const errorId = `${inputId}-error`;

  const describedBy = [
    error ? errorId : null,
    hint ? hintId : null,
  ].filter(Boolean).join(" ") || undefined;

  return (
    <div className="w-full">
      {label && (
        <label htmlFor={inputId} className="ui-label block font-medium text-[var(--text)] mb-1.5">
          {label}
        </label>
      )}
      <input
        id={inputId}
        aria-invalid={error ? "true" : undefined}
        aria-describedby={describedBy}
        className={cn(
          "ui-input w-full rounded-[var(--radius-md)] border bg-[var(--surface-solid)] px-3 py-2 text-[var(--text)] transition-colors duration-[var(--motion-fast)]",
          error
            ? "border-[var(--danger)] focus:border-[var(--danger)] focus:ring-[var(--danger-soft)]"
            : "border-[var(--border-soft)] focus-ring-token focus:border-[var(--accent-primary)]",
          "placeholder:text-[var(--text-secondary)] focus:outline-none",
          className
        )}
        {...props}
      />
      {error && (
        <p id={errorId} className="ui-error mt-1 text-xs text-[var(--danger)]" role="alert">
          {error}
        </p>
      )}
      {!error && hint && (
        <p id={hintId} className="ui-hint mt-1 text-xs text-[var(--text-muted)]">
          {hint}
        </p>
      )}
    </div>
  );
}
