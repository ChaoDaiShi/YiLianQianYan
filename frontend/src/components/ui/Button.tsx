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
    "bg-[image:var(--cyrene-gradient-primary)] text-white shadow-[0_2px_10px_-2px_rgba(234,145,185,0.55)] border border-white/30 hover:brightness-105 active:scale-[0.97]",
  secondary:
    "border border-[var(--border-soft)] bg-[var(--surface-solid)] text-[var(--text)] hover:bg-[var(--surface-hover)] active:scale-[0.98]",
  ghost: "bg-transparent text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)] active:scale-[0.98]",
  danger: "bg-[#e05a80] hover:bg-[#d1476e] text-white shadow-[0_2px_8px_rgba(224,90,128,0.35)] active:scale-[0.97]",
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
