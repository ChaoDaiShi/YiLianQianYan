import { useState } from "react";

interface ToolCallCardProps {
  toolCallId: string;
  name: string;
  args: Record<string, unknown>;
  status: "running" | "success" | "error";
  result?: string;
}

const TOOL_LABELS: Record<string, string> = {
  bash: "💻 执行命令",
  read_file: "📖 读取文件",
  write_file: "✍️ 写入文件",
  edit_file: "✏️ 编辑文件",
  grep: "🔍 搜索内容",
  glob: "📂 查找文件",
  http_request: "🌐 HTTP请求",
  load_skill: "📚 加载技能",
  write_todos: "📋 更新计划",
  process: "⚙️ 进程管理",
  screenshot: "📸 截图",
  mouse: "🖱️ 鼠标",
  keyboard: "⌨️ 键盘",
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

/**
 * Parse tool result text and split into image URIs + plain text segments.
 * Lines starting with "data:image/" are rendered as <img> tags;
 * everything else is rendered as <pre> text.
 */
function ToolResultContent({ result }: { result: string }) {
  const lines = result.split("\n");
  const imageLines: string[] = [];
  const textLines: string[] = [];

  for (const line of lines) {
    if (line.startsWith("data:image/")) {
      imageLines.push(line);
    } else {
      textLines.push(line);
    }
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
              className="max-w-full rounded border border-gray-200 dark:border-gray-600"
              style={{ maxHeight: "400px" }}
            />
          ))}
        </div>
      )}
      {textLines.length > 0 && (
        <pre className="text-xs p-2 rounded bg-gray-100 dark:bg-gray-800 overflow-x-auto max-h-48 overflow-y-auto whitespace-pre-wrap">
          {textLines.join("\n")}
        </pre>
      )}
    </div>
  );
}

export default function ToolCallCard({
  toolCallId: _toolCallId,
  name,
  args,
  status,
  result,
}: ToolCallCardProps) {
  const [expanded, setExpanded] = useState(false);

  const label = TOOL_LABELS[name] || `🔧 ${name}`;
  const argDisplay = TOOL_ARG_DISPLAY[name]?.(args) || JSON.stringify(args);

  return (
    <div className="mb-2 ml-4 border border-gray-200 dark:border-gray-700 rounded-xl overflow-hidden">
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-2 px-3 py-2 text-sm hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
      >
        {/* Status indicator */}
        <span className="flex-shrink-0">
          {status === "running" && (
            <span className="inline-block w-3 h-3 border-2 border-primary-500 border-t-transparent rounded-full animate-spin" />
          )}
          {status === "success" && <span className="text-green-500">✓</span>}
          {status === "error" && <span className="text-red-500">✗</span>}
        </span>

        <span className="font-medium">{label}</span>
        <span className="text-gray-400 truncate flex-1 text-left">{argDisplay}</span>
        <span className="text-gray-400 text-xs">{expanded ? "收起" : "展开"}</span>
      </button>

      {expanded && (
        <div className="px-3 py-2 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50">
          <div className="mb-2">
            <span className="text-xs font-medium text-gray-500">参数:</span>
            <pre className="text-xs mt-1 p-2 rounded bg-gray-100 dark:bg-gray-800 overflow-x-auto">
              {JSON.stringify(args, null, 2)}
            </pre>
          </div>
          {result && (
            <div>
              <span className="text-xs font-medium text-gray-500">结果:</span>
              <ToolResultContent result={result} />
            </div>
          )}
        </div>
      )}
    </div>
  );
}
