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

  const toolCalls = message.tool_calls || [];

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

      {/* Render tool calls that powered this message */}
      {toolCalls.map((tc: any) => (
        <ToolCallCard
          key={tc.toolCallId}
          toolCallId={tc.toolCallId}
          name={tc.name}
          args={tc.args}
          status={tc.status}
          result={tc.result}
        />
      ))}
    </div>
  );
}
