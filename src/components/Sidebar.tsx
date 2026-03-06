import type { Conversation } from "../stores/conversationStore";
import { useLocaleStore } from "../stores/localeStore";
import DebugPanel from "./DebugPanel";

const IconTrash = () => (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
    <path d="M3 6h18M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2" />
    <line x1="10" y1="11" x2="10" y2="17" />
    <line x1="14" y1="11" x2="14" y2="17" />
  </svg>
);
const IconPlus = () => (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
    <line x1="12" y1="5" x2="12" y2="19" />
    <line x1="5" y1="12" x2="19" y2="12" />
  </svg>
);
const IconCollapseSidebar = () => (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <rect x="3" y="4" width="18" height="16" rx="2" />
    <line x1="8" y1="4" x2="8" y2="20" />
  </svg>
);

const IconExpandSidebar = () => (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <rect x="3" y="4" width="18" height="16" rx="2" />
    <line x1="16" y1="4" x2="16" y2="20" />
  </svg>
);
const IconEllipsis = () => (
  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
    <circle cx="12" cy="12" r="1.5" fill="currentColor" />
    <circle cx="6" cy="12" r="1.5" fill="currentColor" />
    <circle cx="18" cy="12" r="1.5" fill="currentColor" />
  </svg>
);

type SidebarProps = {
  conversations: Conversation[];
  currentConversationId: string | null;
  onSelectConversation: (conversationId: string) => void;
  onNewChat: () => void;
  onClearAllConversations?: () => void;
  collapsed?: boolean;
  onToggleCollapse?: () => void;
};

export default function Sidebar({
  conversations,
  currentConversationId,
  onSelectConversation,
  onNewChat,
  onClearAllConversations,
  collapsed = false,
  onToggleCollapse,
}: SidebarProps) {
  const { locale, setLocale } = useLocaleStore();

  const toggleLocale = () => {
    setLocale(locale === "en" ? "zh" : "en");
  };

  return (
    <aside className={`sidebar ${collapsed ? "sidebar-collapsed" : ""}`}>
      <div className="sidebar-header">
        <h1 className="sidebar-title">Mini-Agent</h1>
        <div className="sidebar-actions">
          {!collapsed && (
            <>
              <button
                type="button"
                className="sidebar-icon-btn"
                onClick={onClearAllConversations}
                title="Clear all conversations"
                aria-label="Clear all conversations"
              >
                <IconTrash />
              </button>
              <button type="button" className="sidebar-icon-btn" onClick={onNewChat} title="New chat" aria-label="New chat">
                <IconPlus />
              </button>
            </>
          )}
          <button
            type="button"
            className="sidebar-icon-btn"
            onClick={onToggleCollapse}
            title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
            aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          >
            {collapsed ? <IconExpandSidebar /> : <IconCollapseSidebar />}
          </button>
        </div>
      </div>
      {!collapsed && (
      <nav className="conversation-list">
        <p className="conversation-section-label">Today</p>
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
              <span className="conversation-item-title">{conversation.title}</span>
              {currentConversationId === conversation.id ? (
                <span className="conversation-item-menu">
                  <IconEllipsis />
                </span>
              ) : null}
            </button>
          ))
        )}
        <p className="conversation-footer">You have reached the end of your chat history.</p>
      </nav>
      )}
      {!collapsed && (
      <div className="sidebar-user">
        <div className="sidebar-user-avatar" />
        <span className="sidebar-user-name">Guest</span>
        <button
          type="button"
          className="sidebar-lang-toggle"
          onClick={toggleLocale}
          title={locale === "en" ? "Switch to 中文" : "Switch to English"}
          aria-label={locale === "en" ? "Switch to Chinese" : "Switch to English"}
        >
          {locale === "en" ? "中文" : "EN"}
        </button>
        <span className="sidebar-user-caret">▾</span>
      </div>
      )}
      {!collapsed && <DebugPanel />}
    </aside>
  );
}
