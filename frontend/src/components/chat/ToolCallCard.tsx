import { useState } from "react";
import {
  BookOpen,
  Camera,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  CircleX,
  Cog,
  FileText,
  FolderSearch,
  Globe,
  Keyboard,
  ListTodo,
  LoaderCircle,
  MousePointer,
  Pencil,
  Search,
  ShieldAlert,
  Terminal,
} from "lucide-react";
import type { ToolCallRecord } from "../../types";
import { presentExecutionError } from "./errorDisplay";
import {
  formatElapsed,
  formatToolActivity,
  formatToolDisplayName,
  formatToolResultSummary,
  formatToolStatus,
} from "./toolDisplay";

interface ToolCallCardProps {
  toolCallId: string;
  name: string;
  args: Record<string, unknown>;
  status: ToolCallRecord["status"];
  result?: string;
  verificationStatus?: ToolCallRecord["verificationStatus"];
  verificationReason?: string;
  riskLevel?: ToolCallRecord["riskLevel"];
  approvalStatus?: ToolCallRecord["approvalStatus"];
  startedAt?: number;
  finishedAt?: number;
  compact?: boolean;
}

const TOOL_ICONS: Record<string, React.ReactNode> = {
  bash: <Terminal className="h-4 w-4" />,
  powershell: <Terminal className="h-4 w-4" />,
  read_file: <FileText className="h-4 w-4" />,
  write_file: <Pencil className="h-4 w-4" />,
  edit_file: <Pencil className="h-4 w-4" />,
  grep: <Search className="h-4 w-4" />,
  glob: <FolderSearch className="h-4 w-4" />,
  http_request: <Globe className="h-4 w-4" />,
  load_skill: <BookOpen className="h-4 w-4" />,
  write_todos: <ListTodo className="h-4 w-4" />,
  process: <Cog className="h-4 w-4" />,
  screenshot: <Camera className="h-4 w-4" />,
  mouse: <MousePointer className="h-4 w-4" />,
  keyboard: <Keyboard className="h-4 w-4" />,
};

function statusTone(status: ToolCallRecord["status"]): string {
  if (status === "success") return "text-[var(--success)]";
  if (status === "error") return "text-[var(--danger)]";
  if (status === "blocked") return "text-[var(--accent-gold)]";
  return "text-[var(--accent-blue)]";
}

function statusIcon(status: ToolCallRecord["status"]) {
  if (status === "success") return <CheckCircle2 className="h-4 w-4" />;
  if (status === "error") return <CircleX className="h-4 w-4" />;
  if (status === "blocked") return <ShieldAlert className="h-4 w-4" />;
  return <LoaderCircle className="h-4 w-4 animate-pulse" />;
}

function formatValue(value: unknown): string {
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

export function ToolResultContent({ result }: { result: string }) {
  const imageLines: string[] = [];
  const textLines: string[] = [];

  for (const line of result.split("\n")) {
    if (line.startsWith("data:image/")) imageLines.push(line);
    else textLines.push(line);
  }

  return (
    <div className="mt-1">
      {imageLines.length > 0 && (
        <div className="mb-2 space-y-2">
          {imageLines.map((uri, index) => (
            <img
              key={index}
              src={uri}
              alt={`工具结果截图 ${index + 1}`}
              className="max-h-[400px] max-w-full rounded-lg border border-[var(--border-soft)]"
            />
          ))}
        </div>
      )}
      {textLines.some(Boolean) && (
        <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded-lg border border-[var(--border-soft)] bg-[var(--surface-muted)] p-3 font-mono text-xs text-[var(--text-secondary)]">
          {textLines.join("\n")}
        </pre>
      )}
    </div>
  );
}

export default function ToolCallCard({
  toolCallId,
  name,
  args,
  status,
  result,
  verificationStatus = "not_requested",
  verificationReason,
  riskLevel,
  approvalStatus,
  startedAt,
  finishedAt,
  compact = false,
}: ToolCallCardProps) {
  const [expanded, setExpanded] = useState(false);
  const label = formatToolDisplayName(name);
  const isBlocked = status === "blocked" || approvalStatus === "pending";
  const effectiveStatus = isBlocked ? "blocked" : status;
  const technicalArgs = formatValue(args);
  const elapsed = startedAt !== undefined
    ? formatElapsed(startedAt, finishedAt ?? Date.now())
    : null;
  const verificationLabel =
    verificationStatus === "passed"
      ? "已验证"
      : verificationStatus === "failed"
        ? "验证失败"
        : verificationStatus === "pending"
          ? "验证中"
          : null;
  const errorPresentation =
    effectiveStatus === "error" && result
      ? presentExecutionError(result)
      : null;

  return (
    <article
      className={
        "conversation-tool-card tool-card-status-" + effectiveStatus +
        " overflow-hidden rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] " +
        (compact ? "mb-2" : "mb-3")
      }
      data-status={effectiveStatus}
    >
      <button
        type="button"
        onClick={() => setExpanded((open) => !open)}
        className="flex w-full items-start gap-3 px-3.5 py-3 text-left transition-colors duration-[var(--motion-fast)] hover:bg-[var(--surface-hover)]"
        aria-expanded={expanded}
        aria-controls={"tool-details-" + toolCallId}
        aria-label={(expanded ? "收起" : "查看") + "技术详情：" + label}
      >
        <span className="tool-card-icon-well shrink-0">
          {TOOL_ICONS[name] || <Terminal className="h-4 w-4" />}
        </span>
        <span className={"tool-card-status-icon mt-0.5 shrink-0 " + statusTone(effectiveStatus)}>
          {statusIcon(effectiveStatus)}
        </span>
        <span className="min-w-0 flex-1">
          <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span className="text-sm font-medium text-[var(--text-primary)]">
              {formatToolStatus(effectiveStatus)}
            </span>
            <span className="flex items-center gap-1.5 text-xs text-[var(--text-secondary)]">
              {label}
            </span>
            {(riskLevel === "high" || riskLevel === "critical") && (
              <ShieldAlert
                className="h-3.5 w-3.5 text-[var(--accent-gold)]"
                aria-label="高风险操作"
              />
            )}
          </span>
          <span className="tool-card-summary mt-1 block truncate text-xs text-[var(--text-secondary)]">
            {effectiveStatus === "running"
              ? formatToolActivity(name, args)
              : effectiveStatus === "success"
                ? formatToolResultSummary(result)
                : effectiveStatus === "blocked"
                  ? "小昔涟正在等待你的许可"
                  : errorPresentation?.message ||
                    "这个操作没有成功，请展开错误日志了解原因。"}
          </span>
        </span>
        <span className="flex shrink-0 items-center gap-2 text-[var(--text-faint)]">
          {elapsed && <span className="hidden text-[10px] sm:inline">{elapsed}</span>}
          {expanded ? (
            <ChevronUp className="h-4 w-4" />
          ) : (
            <ChevronDown className="h-4 w-4" />
          )}
        </span>
      </button>

      {expanded && (
        <div
          id={"tool-details-" + toolCallId}
          className="tool-card-details border-t border-[var(--border-soft)] bg-[var(--surface-muted)] px-3.5 py-3"
        >
          <dl className="space-y-2 text-xs">
            <div>
              <dt className="text-[var(--text-faint)]">Tool Name</dt>
              <dd className="mt-0.5 break-all font-mono text-[var(--text-secondary)]">
                {name}
              </dd>
            </div>
            <div>
              <dt className="text-[var(--text-faint)]">Tool Call ID</dt>
              <dd className="mt-0.5 break-all font-mono text-[var(--text-secondary)]">
                {toolCallId}
              </dd>
            </div>
            {typeof args.command === "string" && (
              <div>
                <dt className="text-[var(--text-faint)]">Command</dt>
                <dd className="mt-0.5 overflow-x-auto whitespace-pre-wrap rounded-lg border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2 font-mono text-[var(--text-secondary)]">
                  {args.command}
                </dd>
              </div>
            )}
            <div>
              <dt className="text-[var(--text-faint)]">Arguments</dt>
              <dd className="mt-0.5 overflow-x-auto whitespace-pre-wrap rounded-lg border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2 font-mono text-[var(--text-secondary)]">
                {technicalArgs}
              </dd>
            </div>
            {elapsed && (
              <div>
                <dt className="text-[var(--text-faint)]">执行耗时</dt>
                <dd className="mt-0.5 text-[var(--text-secondary)]">{elapsed}</dd>
              </div>
            )}
            {verificationLabel && (
              <div>
                <dt className="text-[var(--text-faint)]">验证状态</dt>
                <dd className="mt-0.5 text-[var(--text-secondary)]">
                  {verificationLabel}
                  {verificationReason ? "：" + verificationReason : ""}
                </dd>
              </div>
            )}
            {result && effectiveStatus !== "blocked" && (
              <div>
                <dt className="text-[var(--text-faint)]">
                  {effectiveStatus === "error" ? "错误日志" : "Raw Result"}
                </dt>
                <dd>
                  <ToolResultContent result={result} />
                </dd>
              </div>
            )}
            {effectiveStatus === "blocked" && (
              <div className="rounded-lg border border-[var(--warning-border)] bg-[var(--warning-soft)] px-3 py-2 text-[var(--warning-fg)]">
                该操作等待用户确认，尚未执行。
              </div>
            )}
          </dl>
        </div>
      )}
    </article>
  );
}
