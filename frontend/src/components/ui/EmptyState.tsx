import { cn } from "./cn";

interface EmptyStateProps {
  icon?: React.ReactNode;
  title: string;
  description?: string;
  action?: React.ReactNode;
  className?: string;
}

function DefaultRippleArt() {
  return (
    <div className="relative mb-5 flex h-20 w-20 items-center justify-center">
      {/* Outer ripple ring */}
      <div className="absolute inset-0 rounded-full border border-[#ea91b9]/25 animate-ping opacity-60" />
      {/* Mid ripple disc */}
      <div className="absolute inset-2 rounded-full bg-gradient-to-tr from-[#ea91b9]/15 via-[#ad9be8]/15 to-transparent backdrop-blur-sm border border-white/50" />
      {/* Core droplet */}
      <div className="relative flex h-10 w-10 items-center justify-center rounded-full bg-gradient-to-br from-[#f09ec3] to-[#ad9be8] text-white shadow-[0_4px_12px_rgba(234,145,185,0.45)]">
        <span className="text-base select-none">✦</span>
      </div>
    </div>
  );
}

export default function EmptyState({ icon, title, description, action, className }: EmptyStateProps) {
  return (
    <div className={cn("flex flex-col items-center justify-center py-16 px-6 text-center animate-fade-in", className)}>
      {icon ? (
        <div className="mb-4 flex h-12 w-12 items-center justify-center rounded-2xl border border-[#ea91b9]/30 bg-gradient-to-br from-[#f9dce9]/90 to-[#f4f0fc]/90 p-3 text-[var(--accent-primary)] shadow-sm">
          {icon}
        </div>
      ) : (
        <DefaultRippleArt />
      )}
      <h3 className="text-base font-medium text-[var(--text)] tracking-tight">{title}</h3>
      {description && (
        <p className="mt-1.5 max-w-sm text-sm text-[var(--text-secondary)] leading-relaxed">{description}</p>
      )}
      {action && <div className="mt-5">{action}</div>}
    </div>
  );
}
