import { useEffect, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

interface StreamingTextProps {
  text: string;
}

export default function StreamingText({ text }: StreamingTextProps) {
  const [displayedText, setDisplayedText] = useState(text);
  const lastUpdateRef = useRef(0);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    const now = Date.now();
    const elapsed = now - lastUpdateRef.current;

    if (elapsed >= 120) {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
      lastUpdateRef.current = now;
      setDisplayedText(text);
    } else if (!timerRef.current) {
      timerRef.current = setTimeout(() => {
        lastUpdateRef.current = Date.now();
        setDisplayedText(text);
        timerRef.current = null;
      }, 120 - elapsed);
    }
  }, [text]);

  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  return (
    <div className="prose prose-sm max-w-none overflow-x-auto text-[var(--text)]">
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{displayedText}</ReactMarkdown>
      <span className="ml-0.5 inline-block h-4 w-2 animate-pulse bg-[var(--accent-primary)] align-middle" />
    </div>
  );
}
