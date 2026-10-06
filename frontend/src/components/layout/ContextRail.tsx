import type { LucideIcon } from "lucide-react";
import { Tooltip } from "../ui";
import { cn } from "../ui/cn";

export type ContextRailStatusTone = "neutral" | "info" | "success" | "warning" | "danger";

export interface ContextRailItem {
  id: string;
  label: string;
  icon: LucideIcon;
  controls: string;
  expanded: boolean;
  badge?: number | string;
  statusTone?: ContextRailStatusTone;
  onSelect: () => void;
}

interface ContextRailProps {
  label: string;
  items: ContextRailItem[];
  className?: string;
}

export default function ContextRail({ label, items, className }: ContextRailProps) {
  return (
    <aside className={cn("context-rail", className)} aria-label={label}>
      <div className="context-rail-items">
        {items.map((item) => {
          const Icon = item.icon;
          return (
            <Tooltip key={item.id} content={item.label} side="left">
              <button
                type="button"
                className="context-rail-action"
                data-active={item.expanded ? "true" : "false"}
                data-status={item.statusTone ?? "neutral"}
                aria-label={item.label}
                aria-controls={item.controls}
                aria-expanded={item.expanded}
                onClick={item.onSelect}
              >
                <Icon aria-hidden="true" />
                {item.badge !== undefined ? (
                  <span className="context-rail-badge" aria-label={`${item.badge} 项`}>
                    {item.badge}
                  </span>
                ) : item.statusTone && item.statusTone !== "neutral" ? (
                  <span className="context-rail-status" aria-hidden="true" />
                ) : null}
              </button>
            </Tooltip>
          );
        })}
      </div>
    </aside>
  );
}

