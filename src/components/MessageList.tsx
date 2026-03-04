import type { ChatMessage } from "../stores/conversationStore";
import MessageBubble from "./MessageBubble";

function parseAssistantContent(content: string): { toolProcess: string; finalContent: string } {
  if (!content) return { toolProcess: "", finalContent: "" };
  const trimmedContent = content.trimStart();
  if (trimmedContent.includes("Called tool")) {
    const idx = content.indexOf("\n\n");
    if (idx >= 0) {
      return {
        toolProcess: content.slice(0, idx).trimEnd(),
        finalContent: content.slice(idx + 2).trimStart(),
      };
    }
    return { toolProcess: content.trimEnd(), finalContent: "" };
  }
  return { toolProcess: "", finalContent: trimmedContent };
}

type MessageListProps = {
  messages: ChatMessage[];
};

export default function MessageList({ messages }: MessageListProps) {
  if (messages.length === 0) {
    return <p className="message-empty">No messages yet.</p>;
  }

  return (
    <section className="message-list">
      {messages.map((message) => {
        if (message.role === "user") {
          return (
            <MessageBubble
              key={message.id}
              role="user"
              content={message.content}
              streaming={false}
            />
          );
        }
        const { toolProcess, finalContent } = parseAssistantContent(message.content);
        return (
          <div key={message.id} className="assistant-message-group">
            {message.thinking ? (
              <section className="thinking-panel" data-testid="thinking-panel">
                <p className="thinking-label">Thinking</p>
                <pre className="thinking-content">{message.thinking}</pre>
              </section>
            ) : null}
            {toolProcess ? (
              <MessageBubble
                role="assistant"
                content={toolProcess}
                variant="tool-process"
              />
            ) : null}
            <MessageBubble
              role="assistant"
              content={finalContent}
              variant="final"
            />
          </div>
        );
      })}
    </section>
  );
}
