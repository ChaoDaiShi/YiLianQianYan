import { cn } from "./cn";
import { useId } from "react";

interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  label?: string;
  hint?: string;
}

export default function Input({ label, hint, className, id, ...props }: InputProps) {
  const generatedId = useId();
  const inputId = id || generatedId;
  return (
    <div className="w-full">
      {label && (
        <label htmlFor={inputId} className="ui-label block font-medium text-[var(--text)] mb-1.5">
          {label}
        </label>
      )}
      <input
        id={inputId}
        className={cn(
          "ui-input w-full rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] px-3 py-2 text-[var(--text)] transition-colors duration-[var(--motion-fast)]",
          "focus-ring-token placeholder:text-[var(--text-secondary)] focus:border-[var(--accent-primary)] focus:outline-none",
          className
        )}
        {...props}
      />
      {hint && <p className="ui-hint mt-1 text-[var(--text-muted)]">{hint}</p>}
    </div>
  );
}
