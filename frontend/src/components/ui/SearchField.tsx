import { useEffect, useRef, type InputHTMLAttributes } from "react";
import { Search, X } from "lucide-react";
import { cn } from "./cn";

export interface SearchFieldProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "onChange" | "size"> {
  value: string;
  onChange: (value: string) => void;
  onClear?: () => void;
  autoFocusOnSlash?: boolean;
  size?: "sm" | "md";
  shortcutHint?: string;
}

export default function SearchField({
  value,
  onChange,
  onClear,
  autoFocusOnSlash = false,
  size = "md",
  shortcutHint = "/",
  placeholder = "搜索...",
  className,
  "aria-label": ariaLabel = "搜索",
  ...props
}: SearchFieldProps) {
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!autoFocusOnSlash) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "/" && !e.ctrlKey && !e.metaKey && !e.altKey) {
        const target = e.target as HTMLElement;
        const tagName = target?.tagName?.toLowerCase();
        const isEditable = target?.isContentEditable || tagName === "input" || tagName === "textarea" || tagName === "select";

        if (!isEditable) {
          e.preventDefault();
          inputRef.current?.focus();
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [autoFocusOnSlash]);

  const handleClear = () => {
    onChange("");
    onClear?.();
    inputRef.current?.focus();
  };

  return (
    <div
      className={cn(
        "relative flex items-center w-full rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] text-[var(--text)] transition-[border-color,box-shadow] duration-[var(--motion-fast)] focus-within:border-[var(--accent-primary)] focus-within:ring-2 focus-within:ring-[var(--focus-ring-soft)]",
        size === "sm" ? "h-8 text-xs" : "h-10 text-sm",
        className
      )}
    >
      <Search
        className={cn(
          "shrink-0 text-[var(--text-secondary)] pointer-events-none",
          size === "sm" ? "ml-2.5 h-3.5 w-3.5" : "ml-3 h-4 w-4"
        )}
        aria-hidden="true"
      />
      <input
        ref={inputRef}
        type="search"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        aria-label={ariaLabel}
        className={cn(
          "h-full w-full bg-transparent px-2.5 text-[var(--text)] placeholder:text-[var(--text-secondary)] focus:outline-none",
          // Hide native search cancel button in Webkit
          "[&::-webkit-search-cancel-button]:hidden"
        )}
        {...props}
      />
      {value ? (
        <button
          type="button"
          onClick={handleClear}
          aria-label="清空搜索"
          className={cn(
            "shrink-0 rounded p-0.5 text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)] transition-colors mr-1.5",
            size === "sm" ? "p-0.5" : "p-1"
          )}
        >
          <X className={size === "sm" ? "h-3 w-3" : "h-3.5 w-3.5"} />
        </button>
      ) : autoFocusOnSlash && shortcutHint ? (
        <span
          className={cn(
            "pointer-events-none select-none rounded border border-[var(--border-soft)] bg-[var(--surface-muted)] text-[var(--text-faint)] font-mono shrink-0 mr-2",
            size === "sm" ? "px-1 text-[10px]" : "px-1.5 py-0.5 text-xs"
          )}
          aria-hidden="true"
        >
          {shortcutHint}
        </span>
      ) : null}
    </div>
  );
}
