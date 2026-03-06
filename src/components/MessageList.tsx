import type { ChatMessage } from "../stores/conversationStore";
import MessageBubble from "./MessageBubble";
import ThinkingPanel from "./ThinkingPanel";

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
  /** Active streaming thinking; shown in-place before last assistant message */
  activeThinking?: string | null;
  activeThinkingConversationId?: string | null;
  currentConversationId?: string | null;
  onThinkingComplete?: () => void;
};

export default function MessageList({
  messages,
  activeThinking,
  activeThinkingConversationId,
  currentConversationId,
  onThinkingComplete,
}: MessageListProps) {
  if (messages.length === 0) {
    return <p className="message-empty">No messages yet.</p>;
  }

  const showActiveThinking =
    activeThinking &&
    (!activeThinkingConversationId || activeThinkingConversationId === currentConversationId);

  return (
    <section className="message-list">
      {messages.map((message, index) => {
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
        const isLastAssistant = index === messages.length - 1;
        const useActiveThinking = isLastAssistant && showActiveThinking;
        return (
          <div key={message.id} className="assistant-message-group">
            {useActiveThinking ? (
              <ThinkingPanel
                content={activeThinking!}
                streaming
                onComplete={onThinkingComplete}
              />
            ) : message.thinking ? (
              <ThinkingPanel content={message.thinking} />
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
