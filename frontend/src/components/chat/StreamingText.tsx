import { useEffect, useRef, useState } from "react";
import ConversationMarkdown from "./ConversationMarkdown";

interface StreamingTextProps {
  text: string;
}

export default function StreamingText({ text }: StreamingTextProps) {
  const [displayed, setDisplayed] = useState<{ text: string; revealFrom?: number }>({ text, revealFrom: 0 });
  const latestText = useRef(text);
  const lastUpdateRef = useRef(0);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    latestText.current = text;
    const now = Date.now();
    const elapsed = now - lastUpdateRef.current;
    const flush = () => {
      lastUpdateRef.current = Date.now();
      timerRef.current = null;
      const next = latestText.current;
      setDisplayed(current => current.text === next ? current : {
        text: next,
        revealFrom: next.startsWith(current.text) ? current.text.length : undefined,
      });
    };

    if (elapsed >= 120) {
      if (timerRef.current !== null) clearTimeout(timerRef.current);
      flush();
    } else if (timerRef.current === null) {
      timerRef.current = setTimeout(flush, 120 - elapsed);
    }
  }, [text]);

  useEffect(() => {
    return () => {
      if (timerRef.current !== null) clearTimeout(timerRef.current);
      timerRef.current = null;
    };
  }, []);

  return (
    <div className="min-w-0">
      <ConversationMarkdown text={displayed.text} revealFrom={displayed.revealFrom} />
      <span aria-hidden="true" className="ml-0.5 inline-block h-4 w-2 animate-pulse bg-[var(--accent-primary)] align-middle motion-reduce:animate-none" />
    </div>
  );
}
