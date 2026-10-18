import { cn } from "./cn";

export interface SpinnerProps {
  className?: string;
  size?: "sm" | "md" | "lg";
  label?: string;
}

const sizeClasses = {
  sm: "h-3.5 w-3.5 border",
  md: "h-5 w-5 border-2",
  lg: "h-8 w-8 border-2",
};

export default function Spinner({
  className,
  size = "md",
  label = "加载中",
}: SpinnerProps) {
  return (
    <div
      className={cn(
        "rounded-full border-[var(--border-soft)] border-t-[var(--accent-primary)] animate-spin motion-reduce:animate-none motion-reduce:border-dotted",
        sizeClasses[size],
        className
      )}
      role="status"
      aria-label={label}
    />
  );
}
