import type { ChatMessage } from "../stores/conversationStore";
import MessageBubble from "./MessageBubble";

type MessageListProps = {
  messages: ChatMessage[];
  streamingMessageId: string | null;
  isStreaming: boolean;
};

export default function MessageList({ messages, streamingMessageId, isStreaming }: MessageListProps) {
  if (messages.length === 0) {
    return <p className="message-empty">No messages yet.</p>;
  }

  return (
    <section className="message-list">
      {messages.map((message) => (
        <MessageBubble
          key={message.id}
          role={message.role}
          content={message.content}
          streaming={isStreaming && message.id === streamingMessageId}
        />
      ))}
    </section>
  );
}
