import { useState, type ReactNode } from "react";
import { Check, Copy } from "lucide-react";
import { cn } from "./cn";

export interface KeyValueRowProps {
  label: ReactNode;
  value: ReactNode;
  copyable?: boolean | string;
  mono?: boolean;
  hint?: ReactNode;
  className?: string;
  vertical?: boolean;
}

export default function KeyValueRow({
  label,
  value,
  copyable = false,
  mono = false,
  hint,
  className,
  vertical = false,
}: KeyValueRowProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    const textToCopy = typeof copyable === "string" ? copyable : String(value ?? "");
    if (!textToCopy) return;

    try {
      await navigator.clipboard.writeText(textToCopy);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Ignore clipboard write failure
    }
  };

  return (
    <div
      className={cn(
        "py-2 text-xs",
        vertical
          ? "flex flex-col gap-1"
          : "flex items-start justify-between gap-4 border-b border-[var(--border-soft)] last:border-b-0",
        className
      )}
    >
      <div className="shrink-0 text-[var(--text-secondary)]">
        <span>{label}</span>
        {hint && <span className="ml-1 text-[var(--text-faint)]">{hint}</span>}
      </div>

      <div className="flex min-w-0 items-center justify-end gap-1.5 text-right">
        <span
          className={cn(
            "text-[var(--text-primary)] break-all",
            mono && "font-mono text-xs"
          )}
        >
          {value}
        </span>

        {copyable && (
          <button
            type="button"
            onClick={handleCopy}
            aria-label={copied ? "已复制" : "复制"}
            className="shrink-0 rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)] transition-colors"
          >
            {copied ? (
              <Check className="h-3.5 w-3.5 text-[var(--success)]" />
            ) : (
              <Copy className="h-3.5 w-3.5" />
            )}
          </button>
        )}
      </div>
    </div>
  );
}
