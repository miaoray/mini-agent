import { useEffect, useRef, useState } from "react";

const TYPEWRITER_INTERVAL_MS = 16;

type ThinkingPanelProps = {
  content: string;
  /** When true, use typewriter effect; when false, show full content immediately (e.g. persisted) */
  streaming?: boolean;
  /** Called when typewriter finishes (streaming mode only) */
  onComplete?: () => void;
};

export default function ThinkingPanel({ content, streaming = false, onComplete }: ThinkingPanelProps) {
  const [displayLength, setDisplayLength] = useState(0);
  const prevContentRef = useRef("");
  const completedForRef = useRef<string | null>(null);

  useEffect(() => {
    if (!streaming) {
      setDisplayLength(content.length);
      prevContentRef.current = content;
      return;
    }
    const prev = prevContentRef.current;
    if (prev && content.length > prev.length && content.startsWith(prev)) {
      setDisplayLength(prev.length);
    } else if (!prev || !content.startsWith(prev)) {
      setDisplayLength(0);
    }
    prevContentRef.current = content;
  }, [content, streaming]);

  useEffect(() => {
    if (!streaming || displayLength >= content.length) return;
    const id = setInterval(() => {
      setDisplayLength((prev) => {
        const next = Math.min(prev + 2, content.length);
        return next;
      });
    }, TYPEWRITER_INTERVAL_MS);
    return () => clearInterval(id);
  }, [content.length, displayLength, streaming]);

  useEffect(() => {
    if (!streaming || content.length === 0 || displayLength < content.length) return;
    const key = `${content.length}`;
    if (completedForRef.current === key) return;
    completedForRef.current = key;
    onComplete?.();
  }, [streaming, content.length, displayLength, onComplete]);

  const displayContent = streaming ? content.slice(0, displayLength) : content;

  return (
    <section className="thinking-panel" data-testid="thinking-panel">
      <p className="thinking-label">Thinking</p>
      <div className="thinking-content">{displayContent}</div>
    </section>
  );
}
