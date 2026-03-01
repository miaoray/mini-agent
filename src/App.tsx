import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import ChatView from "./components/ChatView";
import ConfigBanner from "./components/ConfigBanner";
import Sidebar from "./components/Sidebar";
import {
  useConversationStore,
  type ChatMessage,
  type Conversation,
} from "./stores/conversationStore";
import "./App.css";

type BackendMessage = {
  id: string;
  conversation_id: string;
  role: "user" | "assistant";
  content: string;
  created_at: number;
};

type ConfigCheckResponse = {
  hasApiKey: boolean;
};

function App() {
  const {
    conversations,
    currentConversationId,
    setCurrentConversation,
    setConversations,
    setMessagesForConversation,
  } = useConversationStore((state) => state);
  const [hasApiKey, setHasApiKey] = useState<boolean | null>(null);

  useEffect(() => {
    void refreshConversations();
    void loadConfigState();
  }, []);

  useEffect(() => {
    if (!currentConversationId) {
      return;
    }
    void hydrateConversationMessages(currentConversationId);
  }, [currentConversationId]);

  function toChatMessage(message: BackendMessage): ChatMessage {
    return {
      id: message.id,
      conversationId: message.conversation_id,
      role: message.role,
      content: message.content,
    };
  }

  async function hydrateConversationMessages(conversationId: string) {
    const dbMessages = await invoke<BackendMessage[]>("list_messages", {
      conversation_id: conversationId,
    });
    setMessagesForConversation(conversationId, dbMessages.map(toChatMessage));
  }

  async function refreshConversations() {
    const list = await invoke<Conversation[]>("list_conversations");
    setConversations(list);
    if (list.length > 0 && !useConversationStore.getState().currentConversationId) {
      setCurrentConversation(list[0].id);
    }
  }

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
        />
        <ChatView />
      </main>
    </>
  );
}

export default App;
