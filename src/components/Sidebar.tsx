import type { Conversation } from "../stores/conversationStore";
import DebugPanel from "./DebugPanel";

type SidebarProps = {
  conversations: Conversation[];
  currentConversationId: string | null;
  onSelectConversation: (conversationId: string) => void;
  onNewChat: () => void;
};

export default function Sidebar({
  conversations,
  currentConversationId,
  onSelectConversation,
  onNewChat,
}: SidebarProps) {
  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <h2>Conversations</h2>
        <button type="button" onClick={onNewChat}>
          New Chat
        </button>
      </div>
      <nav className="conversation-list">
        {conversations.length === 0 ? (
          <p className="conversation-empty">No conversations yet.</p>
        ) : (
          conversations.map((conversation) => (
            <button
              key={conversation.id}
              type="button"
              className={`conversation-item ${
                currentConversationId === conversation.id ? "active" : ""
              }`}
              onClick={() => onSelectConversation(conversation.id)}
            >
              {conversation.title}
            </button>
          ))
        )}
      </nav>
      <DebugPanel />
    </aside>
  );
}
