type MessageBubbleProps = {
  role: "user" | "assistant";
  content: string;
  streaming?: boolean;
};

export default function MessageBubble({ role, content, streaming = false }: MessageBubbleProps) {
  return (
    <article className={`message-bubble ${role}`} data-testid={`message-bubble-${role}`}>
      <p className="message-role">{role === "user" ? "You" : "Assistant"}</p>
      <p className="message-content">{content}</p>
      {streaming ? <p className="message-streaming">Streaming...</p> : null}
    </article>
  );
}
