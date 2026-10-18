import { useRef, type KeyboardEvent, type ReactNode } from "react";
import { cn } from "./cn";

export interface TabItem {
  id: string;
  label: ReactNode;
  badge?: ReactNode;
  icon?: React.ComponentType<{ className?: string }>;
  disabled?: boolean;
  panelId?: string | false;
}

export interface TabsProps {
  tabs: TabItem[];
  activeId: string;
  onChange: (id: string) => void;
  variant?: "underline" | "segment";
  size?: "sm" | "md";
  className?: string;
  "aria-label"?: string;
}

export default function Tabs({
  tabs,
  activeId,
  onChange,
  variant = "underline",
  size = "md",
  className,
  "aria-label": ariaLabel,
}: TabsProps) {
  const tabListRef = useRef<HTMLDivElement>(null);

  const handleKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    const enabledTabs = tabs.filter((t) => !t.disabled);
    const currentIndex = enabledTabs.findIndex((t) => t.id === activeId);
    if (currentIndex === -1) return;

    let targetIndex = -1;

    if (e.key === "ArrowRight") {
      e.preventDefault();
      targetIndex = (currentIndex + 1) % enabledTabs.length;
    } else if (e.key === "ArrowLeft") {
      e.preventDefault();
      targetIndex = (currentIndex - 1 + enabledTabs.length) % enabledTabs.length;
    } else if (e.key === "Home") {
      e.preventDefault();
      targetIndex = 0;
    } else if (e.key === "End") {
      e.preventDefault();
      targetIndex = enabledTabs.length - 1;
    }

    if (targetIndex !== -1) {
      const nextTab = enabledTabs[targetIndex];
      onChange(nextTab.id);
      // Focus corresponding tab button
      const buttons = tabListRef.current?.querySelectorAll<HTMLButtonElement>('[role="tab"]:not([disabled])');
      buttons?.[targetIndex]?.focus();
    }
  };

  if (variant === "segment") {
    return (
      <div
        ref={tabListRef}
        role="tablist"
        aria-label={ariaLabel}
        className={cn(
          "inline-flex items-center rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-muted)] p-0.5",
          className
        )}
      >
        {tabs.map((tab) => {
          const isActive = tab.id === activeId;
          const Icon = tab.icon;
          const controlsId = tab.panelId === false ? undefined : (tab.panelId || `panel-${tab.id}`);
          return (
            <button
              key={tab.id}
              role="tab"
              type="button"
              id={`tab-${tab.id}`}
              aria-selected={isActive}
              aria-controls={controlsId}
              tabIndex={isActive ? 0 : -1}
              disabled={tab.disabled}
              onClick={() => onChange(tab.id)}
              onKeyDown={handleKeyDown}
              className={cn(
                "inline-flex items-center justify-center gap-1.5 rounded-[var(--radius-sm)] font-medium transition-[background-color,color,box-shadow] duration-[var(--motion-fast)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)] disabled:pointer-events-none disabled:opacity-50",
                size === "sm" ? "px-2.5 py-1 text-xs" : "px-3 py-1.5 text-sm",
                isActive
                  ? "bg-[var(--surface-solid)] text-[var(--text-primary)] shadow-sm"
                  : "text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-hover)]"
              )}
            >
              {Icon && <Icon className={size === "sm" ? "h-3.5 w-3.5" : "h-4 w-4"} />}
              <span>{tab.label}</span>
              {tab.badge && (
                <span className="ml-1 inline-flex items-center">{tab.badge}</span>
              )}
            </button>
          );
        })}
      </div>
    );
  }

  // Underline variant
  return (
    <div
      ref={tabListRef}
      role="tablist"
      aria-label={ariaLabel}
      className={cn(
        "flex items-center gap-4 border-b border-[var(--border-soft)]",
        className
      )}
    >
      {tabs.map((tab) => {
        const isActive = tab.id === activeId;
        const Icon = tab.icon;
        const controlsId = tab.panelId === false ? undefined : (tab.panelId || `panel-${tab.id}`);
        return (
          <button
            key={tab.id}
            role="tab"
            type="button"
            id={`tab-${tab.id}`}
            aria-selected={isActive}
            aria-controls={controlsId}
            tabIndex={isActive ? 0 : -1}
            disabled={tab.disabled}
            onClick={() => onChange(tab.id)}
            onKeyDown={handleKeyDown}
            className={cn(
              "group relative inline-flex items-center gap-2 font-medium transition-colors duration-[var(--motion-fast)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)] disabled:pointer-events-none disabled:opacity-50",
              size === "sm" ? "pb-2 text-xs" : "pb-3 text-sm",
              isActive
                ? "text-[var(--accent-primary)] font-semibold"
                : "text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
            )}
          >
            {Icon && <Icon className={size === "sm" ? "h-3.5 w-3.5" : "h-4 w-4"} />}
            <span>{tab.label}</span>
            {tab.badge && (
              <span className="inline-flex items-center">{tab.badge}</span>
            )}
            {/* Active underline indicator */}
            {isActive && (
              <span
                className="absolute bottom-0 left-0 right-0 h-0.5 rounded-full bg-[var(--accent-primary)]"
                aria-hidden="true"
              />
            )}
          </button>
        );
      })}
    </div>
  );
}
