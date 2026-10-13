import { useState } from "react";
import { Check, Copy, Info } from "lucide-react";
import { formatIdentifier, identifierLabel, type IdentifierKind } from "../utils/identifiers";

export default function Identifier({ kind, value, copyable = true, className = "" }: {
  kind: IdentifierKind;
  value: string;
  copyable?: boolean;
  className?: string;
}) {
  const [copiedValue, setCopiedValue] = useState<string | null>(null);
  const [expandedValue, setExpandedValue] = useState<string | null>(null);
  const [copyError, setCopyError] = useState(false);
  const label = identifierLabel(kind);
  const code = formatIdentifier(kind, value);
  const copied = copiedValue === value;
  const expanded = expandedValue === value;

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopiedValue(value);
      setCopyError(false);
    } catch {
      setExpandedValue(value);
      setCopyError(true);
    }
  };

  return <span className={`identifier-code inline-flex max-w-full flex-wrap items-center gap-1 align-middle ${className}`} data-identifier-kind={kind} data-full-id={value}>
    <code className="font-mono" title={`完整${label}：${value}`}>{code}</code>
    {copyable && <>
      <button type="button" className="inline-flex shrink-0 items-center rounded p-0.5 text-[var(--text-faint)] hover:text-[var(--accent)]" aria-label={`${copied ? "已复制" : "复制"}完整${label} ${code}`} title={copied ? "已复制完整值" : "复制完整值"} onClick={() => void copy()}>
        {copied ? <Check size={12} /> : <Copy size={12} />}
      </button>
      <button type="button" className="inline-flex shrink-0 items-center rounded p-0.5 text-[var(--text-faint)] hover:text-[var(--accent)]" aria-label={`${expanded ? "收起" : "查看"}完整${label} ${code}`} aria-expanded={expanded} title={expanded ? "收起完整值" : "查看完整值"} onClick={() => { setExpandedValue(expanded ? null : value); setCopyError(false); }}><Info size={12} /></button>
    </>}
    {expanded && <code data-identifier-detail className="w-full select-all whitespace-normal break-all rounded bg-[var(--surface-muted)] p-1.5 font-mono text-xs font-normal">{value}</code>}
    {copyError && expanded && <span role="status" className="w-full text-xs font-normal text-[var(--text-faint)]">复制失败，可选中完整值手动复制。</span>}
  </span>;
}
