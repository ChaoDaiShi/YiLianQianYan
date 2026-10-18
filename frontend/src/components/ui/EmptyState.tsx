import type { ReactNode } from "react";
import { Search, AlertCircle, WifiOff } from "lucide-react";
import { cn } from "./cn";

export type EmptyStateVariant = "default" | "search" | "unavailable" | "offline";

export interface EmptyStateProps {
  variant?: EmptyStateVariant;
  icon?: ReactNode;
  title: string;
  description?: string;
  action?: ReactNode;
  className?: string;
}

function DefaultRippleArt() {
  return (
    <div className="relative mb-5 flex h-20 w-20 items-center justify-center">
      {/* Outer ripple ring (no animate-ping, purely static or subtle motion) */}
      <div className="absolute inset-0 rounded-full border border-[var(--accent-primary)]/20" />
      {/* Mid ripple disc */}
      <div className="absolute inset-2 rounded-full bg-[var(--accent-soft)] backdrop-blur-sm border border-[var(--border-soft)]" />
      {/* Core droplet */}
      <div className="relative flex h-10 w-10 items-center justify-center rounded-full bg-[var(--cyrene-gradient-primary)] text-[var(--accent-fg)] shadow-[0_4px_12px_rgba(41,38,58,0.15)]">
        <span className="text-base select-none">✦</span>
      </div>
    </div>
  );
}

function SearchEmptyArt() {
  return (
    <div className="mb-4 flex h-12 w-12 items-center justify-center rounded-2xl border border-[var(--border-soft)] bg-[var(--surface-muted)] text-[var(--text-secondary)] shadow-sm">
      <Search className="h-5 w-5" />
    </div>
  );
}

function UnavailableEmptyArt() {
  return (
    <div className="mb-4 flex h-12 w-12 items-center justify-center rounded-2xl border border-[var(--warning-border)] bg-[var(--warning-soft)] text-[var(--warning-fg)] shadow-sm">
      <AlertCircle className="h-5 w-5" />
    </div>
  );
}

function OfflineEmptyArt() {
  return (
    <div className="mb-4 flex h-12 w-12 items-center justify-center rounded-2xl border border-[var(--danger-border)] bg-[var(--danger-soft)] text-[var(--danger-fg)] shadow-sm">
      <WifiOff className="h-5 w-5" />
    </div>
  );
}

export default function EmptyState({
  variant = "default",
  icon,
  title,
  description,
  action,
  className,
}: EmptyStateProps) {
  const renderArt = () => {
    if (icon) {
      return (
        <div className="mb-4 flex h-12 w-12 items-center justify-center rounded-2xl border border-[var(--border-soft)] bg-[var(--surface-muted)] text-[var(--accent-primary)] shadow-sm">
          {icon}
        </div>
      );
    }
    switch (variant) {
      case "search":
        return <SearchEmptyArt />;
      case "unavailable":
        return <UnavailableEmptyArt />;
      case "offline":
        return <OfflineEmptyArt />;
      default:
        return <DefaultRippleArt />;
    }
  };

  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center py-12 px-6 text-center animate-fade-in",
        className
      )}
    >
      {renderArt()}
      <h3 className="text-base font-medium text-[var(--text-primary)] tracking-tight">
        {title}
      </h3>
      {description && (
        <p className="mt-1.5 max-w-sm text-sm text-[var(--text-secondary)] leading-relaxed">
          {description}
        </p>
      )}
      {action && <div className="mt-5">{action}</div>}
    </div>
  );
}
