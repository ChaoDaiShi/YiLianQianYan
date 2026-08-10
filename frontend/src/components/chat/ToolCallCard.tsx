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
  Wrench,
} from "lucide-react";
import type { ToolCallRecord } from "../../types";
import { Badge } from "../ui";

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
}

const TOOL_META: Record<string, { label: string; icon: React.ReactNode }> = {
  bash: { label: "执行命令", icon: <Terminal className="h-3.5 w-3.5" /> },
  read_file: { label: "读取文件", icon: <FileText className="h-3.5 w-3.5" /> },
  write_file: { label: "写入文件", icon: <Pencil className="h-3.5 w-3.5" /> },
  edit_file: { label: "编辑文件", icon: <Pencil className="h-3.5 w-3.5" /> },
  grep: { label: "搜索内容", icon: <Search className="h-3.5 w-3.5" /> },
  glob: { label: "查找文件", icon: <FolderSearch className="h-3.5 w-3.5" /> },
  http_request: { label: "HTTP 请求", icon: <Globe className="h-3.5 w-3.5" /> },
  load_skill: { label: "加载技能", icon: <BookOpen className="h-3.5 w-3.5" /> },
  write_todos: { label: "更新计划", icon: <ListTodo className="h-3.5 w-3.5" /> },
  process: { label: "进程管理", icon: <Cog className="h-3.5 w-3.5" /> },
  screenshot: { label: "截取屏幕", icon: <Camera className="h-3.5 w-3.5" /> },
  mouse: { label: "鼠标操作", icon: <MousePointer className="h-3.5 w-3.5" /> },
  keyboard: { label: "键盘操作", icon: <Keyboard className="h-3.5 w-3.5" /> },
  upscale_image: { label: "放大图片", icon: <Wrench className="h-3.5 w-3.5" /> },
};

const TOOL_ARG_DISPLAY: Record<
  string,
  (args: Record<string, unknown>) => string
> = {
  bash: (args) => String(args.command || ""),
  read_file: (args) => String(args.path || ""),
  write_file: (args) => String(args.path || ""),
  edit_file: (args) => `${args.path || ""}: ${args.find || ""} → ${args.replace || ""}`,
  grep: (args) => String(args.pattern || ""),
  glob: (args) => String(args.pattern || ""),
  http_request: (args) => `${args.method || "GET"} ${args.url || ""}`,
  screenshot: (args) => {
    const parts: string[] = [];
    if (args.monitor !== undefined) parts.push(`显示器 ${args.monitor}`);
    if (args.x !== undefined && args.y !== undefined) {
      parts.push(`区域 (${args.x}, ${args.y})`);
      if (args.width && args.height) parts.push(`${args.width} × ${args.height}`);
    }
    return parts.length > 0 ? parts.join(" · ") : "全屏";
  },
  mouse: (args) => String(args.action || ""),
  keyboard: (args) => String(args.action || ""),
};

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
              className="max-h-[400px] max-w-full rounded-lg border border-[var(--border)]"
            />
          ))}
        </div>
      )}
      {textLines.some(Boolean) && (
        <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded-lg border border-[var(--border)] bg-[var(--input-bg)] p-2 font-mono text-xs text-[var(--text-muted)]">
          {textLines.join("\n")}
        </pre>
      )}
    </div>
  );
}

export default function ToolCallCard({
  name,
  args,
  status,
  result,
  verificationStatus = "not_requested",
  verificationReason,
  riskLevel,
  approvalStatus,
}: ToolCallCardProps) {
  const [expanded, setExpanded] = useState(false);
  const meta = TOOL_META[name] || {
    label: name,
    icon: <Wrench className="h-3.5 w-3.5" />,
  };
  const argDisplay = TOOL_ARG_DISPLAY[name]?.(args) || JSON.stringify(args);
  const rejected = approvalStatus === "rejected";
  const cancelled = approvalStatus === "cancelled" || approvalStatus === "expired";
  const executionLabel = rejected
    ? "用户拒绝，未执行"
    : cancelled
      ? "已取消，未执行"
      : status === "running"
        ? "执行中"
        : status === "success"
          ? "已执行"
          : status === "blocked"
            ? "待审批"
            : "执行失败";
  const executionTone =
    status === "success"
      ? "success"
      : status === "error"
        ? "danger"
        : status === "blocked"
          ? "warning"
          : "accent";
  const verificationMeta = {
    not_requested: { label: "未验证", tone: "default" as const },
    pending: { label: "验证中", tone: "warning" as const },
    passed: { label: "已验证", tone: "success" as const },
    failed: { label: "验证失败", tone: "danger" as const },
  }[verificationStatus];

  return (
    <div className="mb-2 overflow-hidden rounded-xl border border-[var(--border)] bg-[var(--panel)]">
      <button
        type="button"
        onClick={() => setExpanded((open) => !open)}
        className="flex w-full items-center gap-2 px-3 py-2.5 text-sm transition-colors hover:bg-[var(--panel-hover)]"
        aria-expanded={expanded}
      >
        <span className="shrink-0 text-[var(--accent)]">{meta.icon}</span>
        <span className="shrink-0 font-mono text-xs font-semibold">{meta.label}</span>
        <span className="min-w-0 flex-1 truncate text-left font-mono text-xs text-[var(--text-faint)]">
          {argDisplay}
        </span>
        {(riskLevel === "high" || riskLevel === "critical") && (
          <ShieldAlert className="h-3.5 w-3.5 shrink-0 text-[var(--warning)]" />
        )}
        <Badge tone={executionTone} className="hidden shrink-0 min-[720px]:inline-flex">
          {status === "running" ? (
            <LoaderCircle className="h-3 w-3 animate-spin" />
          ) : status === "success" ? (
            <CheckCircle2 className="h-3 w-3" />
          ) : status === "blocked" ? (
            <ShieldAlert className="h-3 w-3" />
          ) : (
            <CircleX className="h-3 w-3" />
          )}
          {executionLabel}
        </Badge>
        <Badge tone={verificationMeta.tone} className="hidden shrink-0 min-[820px]:inline-flex">
          {verificationMeta.label}
        </Badge>
        {expanded ? (
          <ChevronUp className="h-4 w-4 shrink-0 text-[var(--text-faint)]" />
        ) : (
          <ChevronDown className="h-4 w-4 shrink-0 text-[var(--text-faint)]" />
        )}
      </button>

      {expanded && (
        <div className="border-t border-[var(--border)] bg-[var(--panel-2)] px-3 py-3">
          <div className="flex flex-wrap gap-1.5 min-[720px]:hidden">
            <Badge tone={executionTone}>{executionLabel}</Badge>
            <Badge tone={verificationMeta.tone}>{verificationMeta.label}</Badge>
          </div>
          <div className="mt-2">
            <span className="text-xs font-medium text-[var(--text-muted)]">参数</span>
            <pre className="mt-1 overflow-x-auto rounded-lg border border-[var(--border)] bg-[var(--input-bg)] p-2 font-mono text-xs">
              {JSON.stringify(args, null, 2)}
            </pre>
          </div>
          {status === "blocked" && (
            <div className="mt-2 rounded-lg border border-[var(--warning)]/30 bg-[var(--warning)]/10 px-3 py-2 text-xs text-[var(--warning)]">
              该操作等待用户确认，尚未执行。
            </div>
          )}
          {result && status !== "blocked" && (
            <div className="mt-2">
              <span className="text-xs font-medium text-[var(--text-muted)]">执行结果</span>
              <ToolResultContent result={result} />
            </div>
          )}
          {verificationReason && verificationStatus === "failed" && (
            <div className="mt-2 rounded-lg border border-[var(--danger)]/30 bg-[var(--danger)]/10 p-2 text-xs leading-5 text-[var(--danger)]">
              <span className="font-semibold">验证说明：</span>
              {verificationReason}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
