import { useState } from "react";
import {
  Terminal,
  FileText,
  Pencil,
  Search,
  FolderSearch,
  Globe,
  BookOpen,
  ListTodo,
  Cog,
  Camera,
  MousePointer,
  Keyboard,
  Wrench,
} from "lucide-react";
import { Badge } from "../ui";

interface ToolCallCardProps {
  toolCallId: string;
  name: string;
  args: Record<string, unknown>;
  status: "running" | "success" | "error" | "blocked";
  result?: string;
}

const TOOL_META: Record<string, { label: string; icon: React.ReactNode }> = {
  bash: { label: "执行命令", icon: <Terminal className="w-3.5 h-3.5" /> },
  read_file: { label: "读取文件", icon: <FileText className="w-3.5 h-3.5" /> },
  write_file: { label: "写入文件", icon: <Pencil className="w-3.5 h-3.5" /> },
  edit_file: { label: "编辑文件", icon: <Pencil className="w-3.5 h-3.5" /> },
  grep: { label: "搜索内容", icon: <Search className="w-3.5 h-3.5" /> },
  glob: { label: "查找文件", icon: <FolderSearch className="w-3.5 h-3.5" /> },
  http_request: { label: "HTTP 请求", icon: <Globe className="w-3.5 h-3.5" /> },
  load_skill: { label: "加载技能", icon: <BookOpen className="w-3.5 h-3.5" /> },
  write_todos: { label: "更新计划", icon: <ListTodo className="w-3.5 h-3.5" /> },
  process: { label: "进程管理", icon: <Cog className="w-3.5 h-3.5" /> },
  screenshot: { label: "截图", icon: <Camera className="w-3.5 h-3.5" /> },
  mouse: { label: "鼠标", icon: <MousePointer className="w-3.5 h-3.5" /> },
  keyboard: { label: "键盘", icon: <Keyboard className="w-3.5 h-3.5" /> },
  upscale_image: { label: "图片放大", icon: <Wrench className="w-3.5 h-3.5" /> },
};

const TOOL_ARG_DISPLAY: Record<string, (args: Record<string, unknown>) => string> = {
  bash: (args) => (args.command as string) || "",
  read_file: (args) => (args.path as string) || "",
  write_file: (args) => (args.path as string) || "",
  edit_file: (args) => `${args.path}: "${args.find}" → "${args.replace}"`,
  grep: (args) => (args.pattern as string) || "",
  glob: (args) => (args.pattern as string) || "",
  http_request: (args) => `${args.method || "GET"} ${args.url || ""}`,
  screenshot: (args) => {
    const parts: string[] = [];
    if (args.monitor !== undefined) parts.push(`显示器${args.monitor}`);
    if (args.x !== undefined && args.y !== undefined) {
      parts.push(`区域(${args.x},${args.y})`);
      if (args.width && args.height) parts.push(`${args.width}x${args.height}`);
    }
    return parts.length > 0 ? parts.join(" ") : "全屏";
  },
  mouse: (args) => (args.action as string) || "",
  keyboard: (args) => (args.action as string) || "",
};

export function ToolResultContent({ result }: { result: string }) {
  const lines = result.split("\n");
  const imageLines: string[] = [];
  const textLines: string[] = [];

  for (const line of lines) {
    if (line.startsWith("data:image/")) imageLines.push(line);
    else textLines.push(line);
  }

  return (
    <div className="mt-1">
      {imageLines.length > 0 && (
        <div className="space-y-2 mb-2">
          {imageLines.map((uri, i) => (
            <img
              key={i}
              src={uri}
              alt={`截图 ${i + 1}`}
              className="max-w-full rounded border border-[var(--border)]"
              style={{ maxHeight: "400px" }}
            />
          ))}
        </div>
      )}
      {textLines.length > 0 && (
        <pre className="text-xs p-2 rounded bg-[var(--input-bg)] border border-[var(--border)] overflow-x-auto max-h-48 overflow-y-auto whitespace-pre-wrap font-mono text-[var(--text-muted)]">
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
}: ToolCallCardProps) {
  const [expanded, setExpanded] = useState(name === "screenshot");
  const meta = TOOL_META[name] || { label: name, icon: <Wrench className="w-3.5 h-3.5" /> };
  const argDisplay = TOOL_ARG_DISPLAY[name]?.(args) || JSON.stringify(args);

  return (
    <div className="mb-2 ml-4 border border-[var(--border)] rounded-xl overflow-hidden bg-[var(--panel)]/60">
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-2 px-3 py-2 text-sm hover:bg-[var(--panel-hover)] transition-colors"
      >
        <span className="flex-shrink-0">
          {status === "running" && (
            <span className="inline-block w-3 h-3 border-2 border-[var(--accent)] border-t-transparent rounded-full animate-spin" />
          )}
          {status === "success" && <Badge tone="success">✓</Badge>}
          {status === "error" && <Badge tone="danger">✗</Badge>}
          {status === "blocked" && <Badge tone="warning">⛔</Badge>}
        </span>
        <span className="text-[var(--accent)]">{meta.icon}</span>
        <span className="font-medium font-mono text-xs">{meta.label}</span>
        <span className="text-[var(--text-faint)] truncate flex-1 text-left text-xs font-mono">
          {argDisplay}
        </span>
        <span className="text-[var(--text-faint)] text-xs">{expanded ? "收起" : "展开"}</span>
      </button>

      {expanded && (
        <div className="px-3 py-2 border-t border-[var(--border)] bg-[var(--panel-2)]/50">
          <div className="mb-2">
            <span className="text-xs font-medium text-[var(--text-muted)]">参数</span>
            <pre className="text-xs mt-1 p-2 rounded bg-[var(--input-bg)] border border-[var(--border)] overflow-x-auto font-mono">
              {JSON.stringify(args, null, 2)}
            </pre>
          </div>
          {status === "blocked" && (
            <div className="mt-2 px-3 py-2 rounded-lg bg-amber-500/10 border border-amber-500/30 text-xs text-amber-400">
              <div className="font-medium mb-0.5">⚠️ 需要用户批准，尚未执行</div>
              {result && <div className="whitespace-pre-wrap">{result}</div>}
            </div>
          )}
          {result && status !== "blocked" && (
            <div>
              <span className="text-xs font-medium text-[var(--text-muted)]">结果</span>
              <ToolResultContent result={result} />
            </div>
          )}
        </div>
      )}
    </div>
  );
}
