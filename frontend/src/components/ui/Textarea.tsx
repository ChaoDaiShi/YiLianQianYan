import { cn } from "./cn";

interface TextareaProps extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {
  label?: string;
}

export default function Textarea({ label, className, id, ...props }: TextareaProps) {
  const inputId = id || (label ? `ta-${label}` : undefined);
  return (
    <div className="w-full">
      {label && (
        <label htmlFor={inputId} className="block text-sm font-medium text-[var(--text)] mb-1.5">
          {label}
        </label>
      )}
      <textarea
        id={inputId}
        className={cn(
          "min-h-[80px] w-full resize-y rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] px-3 py-2 text-sm text-[var(--text)] transition-colors duration-[var(--motion-fast)]",
          "focus-ring-token placeholder:text-[var(--text-secondary)] focus:border-[var(--accent-primary)] focus:outline-none",
          className
        )}
        {...props}
      />
    </div>
  );
}
