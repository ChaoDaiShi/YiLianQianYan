import { forwardRef, type ButtonHTMLAttributes, type MouseEvent } from "react";
import Spinner from "./Spinner";
import { cn } from "./cn";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
export type ButtonSize = "sm" | "md" | "lg";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  loading?: boolean;
  loadingLabel?: string;
  iconOnly?: boolean;
}

const variants: Record<ButtonVariant, string> = {
  primary:
    "bg-[var(--accent-primary)] text-[var(--accent-fg)] shadow-sm hover:bg-[var(--accent-primary-hover)] hover:shadow active:translate-y-[0.5px] border border-transparent",
  secondary:
    "border border-[var(--border-soft)] bg-[var(--surface-solid)] text-[var(--text)] hover:bg-[var(--surface-hover)] hover:border-[var(--border-strong)] active:translate-y-[0.5px]",
  ghost:
    "border border-transparent bg-transparent text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)] active:translate-y-[0.5px]",
  danger:
    "border border-[var(--danger-border)] bg-[var(--danger)] text-[var(--danger-fg)] hover:bg-[var(--danger-hover)] active:translate-y-[0.5px]",
};

const sizes: Record<ButtonSize, string> = {
  sm: "min-h-[32px] rounded-[var(--radius-sm)] px-2.5 py-1 text-xs ui-button-sm",
  md: "min-h-[36px] rounded-[var(--radius-md)] px-3.5 py-2 text-sm",
  lg: "min-h-[44px] rounded-[var(--radius-md)] px-5 py-2.5 text-base",
};

const iconOnlySizes: Record<ButtonSize, string> = {
  sm: "h-8 w-8 min-h-[32px] p-0 rounded-[var(--radius-sm)]",
  md: "h-9 w-9 min-h-[36px] p-0 rounded-[var(--radius-md)]",
  lg: "h-11 w-11 min-h-[44px] p-0 rounded-[var(--radius-md)]",
};

const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  {
    type = "button",
    variant = "primary",
    size = "md",
    loading = false,
    loadingLabel,
    iconOnly = false,
    className,
    disabled,
    onClick,
    children,
    ...props
  },
  ref
) {
  const handleClick = (e: MouseEvent<HTMLButtonElement>) => {
    if (loading || disabled) {
      e.preventDefault();
      return;
    }
    onClick?.(e);
  };

  return (
    <button
      ref={ref}
      type={type}
      className={cn(
        "ui-button focus-ring-token inline-flex items-center justify-center font-medium transition-[background-color,border-color,box-shadow,color,transform] duration-[var(--motion-fast)] focus-visible:outline-none",
        variants[variant],
        iconOnly ? iconOnlySizes[size] : sizes[size],
        (disabled || loading) && "pointer-events-none opacity-50",
        className
      )}
      data-variant={variant}
      disabled={disabled}
      aria-disabled={loading || disabled ? "true" : undefined}
      aria-busy={loading ? "true" : undefined}
      onClick={handleClick}
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
          <Spinner className={iconOnly ? "h-3.5 w-3.5" : "h-3.5 w-3.5 shrink-0"} />
          {!iconOnly && <span>{loadingLabel ?? "处理中"}</span>}
        </span>
      </span>
    </button>
  );
});

export default Button;
