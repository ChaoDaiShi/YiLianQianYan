import { useState } from "react";
import { Check, Copy } from "lucide-react";
import { cn } from "./cn";

export interface CodeBlockProps {
  code: string;
  language?: string;
  showLineNumbers?: boolean;
  maxHeight?: string;
  className?: string;
  title?: string;
}

export default function CodeBlock({
  code,
  language,
  showLineNumbers = false,
  maxHeight = "360px",
  className,
  title,
}: CodeBlockProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Ignore clipboard write failure
    }
  };

  const lines = code.split("\n");

  return (
    <div
      className={cn(
        "group relative flex flex-col overflow-hidden rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] font-mono text-xs",
        className
      )}
    >
      {/* Top Header bar with language / title and copy button */}
      <div className="flex h-8 shrink-0 items-center justify-between border-b border-[var(--border-soft)] bg-[var(--surface-muted)] px-3 text-[var(--text-secondary)]">
        <span className="font-sans font-medium uppercase tracking-wider text-[11px] text-[var(--text-secondary)]">
          {(title || language || "TEXT").toUpperCase()}
        </span>

        <button
          type="button"
          onClick={handleCopy}
          aria-label={copied ? "已复制代码" : "复制代码"}
          className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)] transition-colors"
        >
          {copied ? (
            <>
              <Check className="h-3 w-3 text-[var(--success)]" />
              <span>已复制</span>
            </>
          ) : (
            <>
              <Copy className="h-3 w-3" />
              <span>复制</span>
            </>
          )}
        </button>
      </div>

      {/* Code body */}
      <div
        style={{ maxHeight }}
        className="min-h-0 overflow-auto p-3 text-[var(--text-primary)] leading-relaxed scrollbar-thin"
      >
        {showLineNumbers ? (
          <table className="w-full border-collapse">
            <tbody>
              {lines.map((line, idx) => (
                <tr key={idx} className="hover:bg-[var(--surface-hover)]">
                  <td className="w-8 select-none pr-3 text-right text-[var(--text-faint)]">
                    {idx + 1}
                  </td>
                  <td className="whitespace-pre break-all">{line}</td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <pre className="whitespace-pre-wrap break-all">{code}</pre>
        )}
      </div>
    </div>
  );
}
