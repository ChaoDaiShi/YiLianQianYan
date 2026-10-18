import { useId, type ReactNode } from "react";
import { cn } from "./cn";

export interface FormFieldProps {
  id?: string;
  label?: ReactNode;
  hint?: ReactNode;
  error?: ReactNode;
  required?: boolean;
  className?: string;
  children:
    | ReactNode
    | ((props: {
        id: string;
        "aria-describedby"?: string;
        "aria-invalid"?: boolean;
      }) => ReactNode);
}

export default function FormField({
  id: explicitId,
  label,
  hint,
  error,
  required,
  className,
  children,
}: FormFieldProps) {
  const generatedId = useId();
  const id = explicitId || generatedId;
  const hintId = hint ? `${id}-hint` : undefined;
  const errorId = error ? `${id}-error` : undefined;
  const describedBy = [errorId, hintId].filter(Boolean).join(" ") || undefined;

  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      {label && (
        <label
          htmlFor={id}
          className="text-xs font-medium text-[var(--text-secondary)] flex items-center gap-1"
        >
          {label}
          {required && (
            <span className="text-[var(--danger)]" aria-hidden="true">
              *
            </span>
          )}
        </label>
      )}

      {typeof children === "function"
        ? children({
            id,
            "aria-describedby": describedBy,
            "aria-invalid": Boolean(error),
          })
        : children}

      {error ? (
        <p
          id={errorId}
          role="alert"
          className="text-xs text-[var(--danger)] leading-relaxed"
        >
          {error}
        </p>
      ) : hint ? (
        <p
          id={hintId}
          className="text-xs text-[var(--text-faint)] leading-relaxed"
        >
          {hint}
        </p>
      ) : null}
    </div>
  );
}
