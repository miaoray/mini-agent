type MessageBubbleProps = {
  role: "user" | "assistant";
  content: string;
  streaming?: boolean;
  variant?: "tool-process" | "final";
};

export default function MessageBubble({
  role,
  content,
  streaming = false,
  variant = "final",
}: MessageBubbleProps) {
  const roleLabel =
    role === "user"
      ? "You"
      : variant === "tool-process"
        ? "工具调用"
        : "Assistant";
  const showContent = content || streaming;
  return (
    <article
      className={`message-bubble ${role} ${variant}`}
      data-testid={`message-bubble-${role}`}
    >
      <p className="message-role">{roleLabel}</p>
      {showContent ? <p className="message-content">{content}</p> : null}
      {streaming ? <p className="message-streaming">Streaming...</p> : null}
    </article>
  );
}
