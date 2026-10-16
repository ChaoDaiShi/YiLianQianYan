import Spinner from "./Spinner";
import { cn } from "./cn";

type Variant = "primary" | "secondary" | "ghost" | "danger";
type Size = "sm" | "md" | "lg";

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
  loading?: boolean;
  loadingLabel?: string;
}

const variants: Record<Variant, string> = {
  primary:
    "bg-[var(--accent-primary)] text-[var(--accent-fg)] hover:bg-[var(--accent-primary-hover)]",
  secondary:
    "border border-[var(--border-soft)] bg-[var(--surface-solid)] text-[var(--text)] hover:bg-[var(--surface-hover)]",
  ghost: "bg-transparent text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]",
  danger: "bg-[var(--danger)] text-[var(--danger-fg)] hover:opacity-90",
};

const sizes: Record<Size, string> = {
  sm: "min-h-[32px] rounded-[var(--radius-sm)] px-2.5 py-1 text-xs ui-button-sm",
  md: "min-h-[36px] rounded-[var(--radius-md)] px-3.5 py-2 text-sm",
  lg: "min-h-[44px] rounded-[var(--radius-md)] px-5 py-2.5 text-base",
};

export default function Button({
  variant = "primary",
  size = "md",
  loading = false,
  loadingLabel,
  className,
  disabled,
  children,
  ...props
}: ButtonProps) {
  const isDisabled = disabled || loading;

  return (
    <button
      className={cn(
        "ui-button focus-ring-token inline-flex items-center justify-center font-medium transition-colors duration-[var(--motion-fast)] focus-visible:outline-none disabled:pointer-events-none disabled:opacity-40",
        variants[variant],
        sizes[size],
        className
      )}
      data-variant={variant}
      disabled={isDisabled}
      aria-busy={loading ? "true" : undefined}
      {...props}
    >
      <span className="grid items-center justify-items-center">
        <span
          className="col-start-1 row-start-1 inline-flex items-center justify-center gap-1.5"
          style={{ visibility: loading ? "hidden" : "visible" }}
          aria-hidden={loading ? "true" : undefined}
        >
          {children}
        </span>
        <span
          className="col-start-1 row-start-1 inline-flex items-center justify-center gap-1.5"
          style={{ visibility: loading ? "visible" : "hidden" }}
          aria-hidden={!loading ? "true" : undefined}
        >
          <Spinner className="h-3.5 w-3.5 shrink-0" />
          <span>{loadingLabel ?? "处理中"}</span>
        </span>
      </span>
    </button>
  );
}
