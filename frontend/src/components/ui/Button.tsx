import { cn } from "./cn";

type Variant = "primary" | "secondary" | "ghost" | "danger";
type Size = "sm" | "md" | "lg";

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
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
  sm: "rounded-[var(--radius-sm)] px-2.5 py-1 text-xs",
  md: "rounded-[var(--radius-md)] px-3.5 py-2 text-sm",
  lg: "rounded-[var(--radius-md)] px-5 py-2.5 text-sm",
};

export default function Button({
  variant = "primary",
  size = "md",
  className,
  disabled,
  children,
  ...props
}: ButtonProps) {
  return (
    <button
      className={cn(
        "focus-ring-token inline-flex items-center justify-center gap-1.5 font-medium transition-colors duration-[var(--motion-fast)] focus-visible:outline-none disabled:pointer-events-none disabled:opacity-40",
        variants[variant],
        sizes[size],
        className
      )}
      disabled={disabled}
      {...props}
    >
      {children}
    </button>
  );
}
