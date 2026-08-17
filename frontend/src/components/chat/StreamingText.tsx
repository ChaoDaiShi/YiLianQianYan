import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

interface StreamingTextProps { text: string }

export default function StreamingText({ text }: StreamingTextProps) {
  return (
    <div className="prose prose-sm max-w-none overflow-x-auto text-[var(--text)]">
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{text}</ReactMarkdown>
      <span className="ml-0.5 inline-block h-4 w-2 animate-pulse bg-[var(--accent-primary)] align-middle" />
    </div>
  );
}
