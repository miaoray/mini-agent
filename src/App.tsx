import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import ChatView from "./components/ChatView";
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

function App() {
  const {
    conversations,
    currentConversationId,
    setCurrentConversation,
    setConversations,
    setMessagesForConversation,
  } = useConversationStore((state) => state);

  useEffect(() => {
    void refreshConversations();
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

  async function handleNewChat() {
    const conversationId = await invoke<string>("create_conversation");
    await refreshConversations();
    setCurrentConversation(conversationId);
  }

  return (
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
  );
}

export default App;
