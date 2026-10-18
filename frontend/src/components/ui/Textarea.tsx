import { cn } from "./cn";
import { useId, type TextareaHTMLAttributes } from "react";

export interface TextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  label?: string;
  hint?: string;
  error?: string;
}

export default function Textarea({
  label,
  hint,
  error,
  className,
  id,
  ...props
}: TextareaProps) {
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
      <textarea
        id={inputId}
        aria-invalid={error ? "true" : undefined}
        aria-describedby={describedBy}
        className={cn(
          "ui-textarea min-h-[80px] w-full resize-y rounded-[var(--radius-md)] border bg-[var(--surface-solid)] px-3 py-2 text-[var(--text)] transition-colors duration-[var(--motion-fast)]",
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
