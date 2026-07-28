import { Message } from "../../types";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import ToolCallCard from "./ToolCallCard";

interface MessageBubbleProps {
  message: Message;
}

export default function MessageBubble({ message }: MessageBubbleProps) {
  const isUser = message.role === "user";
  const isTool = message.role === "tool";

  if (isTool) return null;

  // Safely handle tool_calls from both new (streaming) and old (database) formats
  const rawToolCalls = message.tool_calls;
  const toolCalls = Array.isArray(rawToolCalls) ? rawToolCalls : [];

  return (
    <div className="mb-4">
      <div className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
        <div
          className={`max-w-[85%] px-4 py-3 rounded-2xl ${
            isUser
              ? "bg-primary-500 text-white rounded-br-md"
              : "bg-gray-100 dark:bg-gray-800 text-gray-900 dark:text-gray-100 rounded-bl-md"
          }`}
        >
          {isUser ? (
            <p className="whitespace-pre-wrap text-sm leading-relaxed">{message.content}</p>
          ) : (
            <div className="prose prose-sm dark:prose-invert max-w-none overflow-x-auto">
              <ReactMarkdown remarkPlugins={[remarkGfm]}>{message.content}</ReactMarkdown>
            </div>
          )}
        </div>
      </div>

      {/* Render tool calls that powered this message (only for new-format records) */}
      {toolCalls.map((tc: any) => {
        const id = tc.toolCallId || tc.id || "";
        const name = tc.name || tc.function?.name || "";
        const args = tc.args || tc.function?.arguments || {};
        const status = tc.status || "success";
        const result = tc.result;
        return (
          <ToolCallCard
            key={id}
            toolCallId={id}
            name={name}
            args={typeof args === "string" ? {} : args}
            status={status}
            result={result}
          />
        );
      })}
    </div>
  );
}
