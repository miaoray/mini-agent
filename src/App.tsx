import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import ChatView from "./components/ChatView";
import Sidebar from "./components/Sidebar";
import { useConversationStore, type Conversation } from "./stores/conversationStore";
import "./App.css";

function App() {
  const { conversations, currentConversationId, setCurrentConversation, setConversations } =
    useConversationStore((state) => state);

  useEffect(() => {
    void refreshConversations();
  }, []);

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
