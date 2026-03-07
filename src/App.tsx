import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import ChatView from "./components/ChatView";
import ConfigBanner from "./components/ConfigBanner";
import Sidebar from "./components/Sidebar";
import { useConversationStore, type Conversation } from "./stores/conversationStore";
import { hydrateConversationMessages } from "./lib/conversationHydrate";
import { setupTauriListeners, teardownTauriListeners } from "./eventBridge";
import "./App.css";

type ConfigCheckResponse = {
  hasApiKey: boolean;
};

function App() {
  const {
    conversations,
    currentConversationId,
    setCurrentConversation,
    setConversations,
  } = useConversationStore((state) => state);
  const [hasApiKey, setHasApiKey] = useState<boolean | null>(null);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);

  useEffect(() => {
    void refreshConversations();
    void loadConfigState();
  }, []);

  useEffect(() => {
    void setupTauriListeners();
    return () => teardownTauriListeners();
  }, []);

  useEffect(() => {
    if (!currentConversationId) return;
    console.debug("[hydrate-trigger] currentConversationId changed to", currentConversationId);
    void hydrateConversationMessages(currentConversationId);
  }, [currentConversationId]);

  const LAST_CONV_KEY = "mini-agent-last-conversation-id";

  async function refreshConversations() {
    const list = await invoke<Conversation[]>("list_conversations");
    setConversations(list);
    if (list.length > 0 && !useConversationStore.getState().currentConversationId) {
      const saved = localStorage.getItem(LAST_CONV_KEY);
      const valid =
        saved && list.some((c) => c.id === saved);
      setCurrentConversation(valid ? saved : list[0].id);
    } else if (list.length === 0) {
      // If there are no conversations, reset the current conversation
      setCurrentConversation(null);
    }
  }

  useEffect(() => {
    if (currentConversationId) {
      localStorage.setItem(LAST_CONV_KEY, currentConversationId);
    }
  }, [currentConversationId]);

  async function loadConfigState() {
    try {
      const result = await invoke<ConfigCheckResponse>("check_config");
      setHasApiKey(result.hasApiKey);
    } catch (_error) {
      setHasApiKey(false);
    }
  }

  async function handleNewChat() {
    const conversationId = await invoke<string>("create_conversation");
    await refreshConversations();
    setCurrentConversation(conversationId);
  }

  async function handleClearAllConversations() {
    await invoke("clear_all_conversations");
    await refreshConversations(); // Refresh the conversation list after clearing
    useConversationStore.getState().clearMessages();
    localStorage.removeItem(LAST_CONV_KEY);
  }

  return (
    <>
      {hasApiKey !== null ? <ConfigBanner hasApiKey={hasApiKey} /> : null}
      <main className="app-layout">
        <Sidebar
          conversations={conversations}
          currentConversationId={currentConversationId}
          onSelectConversation={setCurrentConversation}
          onNewChat={() => {
            void handleNewChat();
          }}
          onClearAllConversations={() => {
            void handleClearAllConversations();
          }}
          collapsed={sidebarCollapsed}
          onToggleCollapse={() => setSidebarCollapsed((c) => !c)}
        />
        <ChatView />
      </main>
    </>
  );
}

export default App;
