import { useId, type SelectHTMLAttributes } from "react";
import { ChevronDown } from "lucide-react";
import { cn } from "./cn";
import FormField from "./FormField";

export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

export interface SelectProps extends Omit<SelectHTMLAttributes<HTMLSelectElement>, "size"> {
  label?: string;
  hint?: string;
  error?: string;
  required?: boolean;
  options: SelectOption[];
  size?: "sm" | "md";
}

export default function Select({
  label,
  hint,
  error,
  required,
  options,
  size = "md",
  className,
  id: explicitId,
  disabled,
  ...props
}: SelectProps) {
  const generatedId = useId();
  const id = explicitId || generatedId;

  return (
    <FormField id={id} label={label} hint={hint} error={error} required={required}>
      {({ id: fieldId, "aria-describedby": describedBy, "aria-invalid": invalid }) => (
        <div className="relative w-full">
          <select
            id={fieldId}
            disabled={disabled}
            aria-describedby={describedBy}
            aria-invalid={invalid}
            className={cn(
              "ui-select w-full appearance-none rounded-[var(--radius-md)] border bg-[var(--surface-solid)] text-[var(--text)] transition-[border-color,box-shadow] duration-[var(--motion-fast)]",
              size === "sm" ? "h-8 px-2.5 pr-8 text-xs" : "h-10 px-3 pr-9 text-sm",
              invalid
                ? "border-[var(--danger)] focus:border-[var(--danger)] focus:ring-2 focus:ring-[var(--danger-soft)]"
                : "border-[var(--border-soft)] focus:border-[var(--accent-primary)] focus:ring-2 focus:ring-[var(--focus-ring-soft)]",
              disabled && "cursor-not-allowed opacity-50 bg-[var(--surface-muted)]",
              "focus:outline-none",
              className
            )}
            {...props}
          >
            {options.map((opt) => (
              <option key={opt.value} value={opt.value} disabled={opt.disabled}>
                {opt.label}
              </option>
            ))}
          </select>
          <ChevronDown
            className={cn(
              "pointer-events-none absolute top-1/2 -translate-y-1/2 text-[var(--text-secondary)]",
              size === "sm" ? "right-2 h-3.5 w-3.5" : "right-2.5 h-4 w-4"
            )}
            aria-hidden="true"
          />
        </div>
      )}
    </FormField>
  );
}
