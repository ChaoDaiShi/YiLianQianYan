import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

interface StreamingTextProps { text: string }

export default function StreamingText({ text }: StreamingTextProps) {
  return (
    <div className="prose prose-sm max-w-none overflow-x-auto text-[var(--text)]">
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{text}</ReactMarkdown>
      <span className="inline-block w-2 h-4 bg-primary-500 animate-pulse ml-0.5 align-middle" />
    </div>
  );
}
