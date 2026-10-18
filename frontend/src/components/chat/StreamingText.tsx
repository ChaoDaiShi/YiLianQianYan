import { useEffect, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

interface StreamingTextProps {
  text: string;
}

export default function StreamingText({ text }: StreamingTextProps) {
  const [displayedText, setDisplayedText] = useState(text);
  const lastUpdateRef = useRef(0);
  const rafIdRef = useRef<number | null>(null);

  useEffect(() => {
    const now = Date.now();
    // Throttle markdown parsing to at most once per 120ms during live streaming
    if (now - lastUpdateRef.current >= 120) {
      lastUpdateRef.current = now;
      setDisplayedText(text);
    } else {
      if (rafIdRef.current) cancelAnimationFrame(rafIdRef.current);
      rafIdRef.current = requestAnimationFrame(() => {
        lastUpdateRef.current = Date.now();
        setDisplayedText(text);
      });
    }

    return () => {
      if (rafIdRef.current) cancelAnimationFrame(rafIdRef.current);
    };
  }, [text]);

  return (
    <div className="prose prose-sm max-w-none overflow-x-auto text-[var(--text)]">
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{displayedText}</ReactMarkdown>
      <span className="ml-0.5 inline-block h-4 w-2 animate-pulse bg-[var(--accent-primary)] align-middle" />
    </div>
  );
}
