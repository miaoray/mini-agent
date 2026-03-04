import { invoke } from "@tauri-apps/api/core";
import { useConversationStore, type ChatMessage } from "../stores/conversationStore";

type BackendMessage = {
  id: string;
  conversation_id: string;
  role: "user" | "assistant";
  content: string;
  created_at: number;
};

function toChatMessage(message: BackendMessage): ChatMessage {
  return {
    id: message.id,
    conversationId: message.conversation_id,
    role: message.role,
    content: message.content,
  };
}

/**
 * Hydrate: 从 DB 拉取消息并与内存中的消息合并，作为会话的权威数据源。
 * 触发时机：1) 切换会话时 (currentConversationId 变化)  2) chat-done 后 (eventBridge)
 * 合并逻辑：db 为主，memory 中 content 更长或带 thinking 的覆盖 db；memoryOnly 追加到末尾
 */
export async function hydrateConversationMessages(conversationId: string): Promise<void> {
  const dbMessages = await invoke<BackendMessage[]>("list_messages", {
    conversationId,
  });
  const dbChatMessages = dbMessages.map(toChatMessage);
  const dbIds = new Set(dbChatMessages.map((m) => m.id));
  const memoryMessages =
    useConversationStore.getState().messagesByConversation[conversationId] ?? [];
  const memoryOnly = memoryMessages.filter((m) => !dbIds.has(m.id));
  const memoryById = new Map(memoryMessages.map((m) => [m.id, m]));
  const merged = dbChatMessages.map((dbMsg) => {
    const mem = memoryById.get(dbMsg.id);
    if (mem && (mem.content.length > dbMsg.content.length || mem.thinking)) {
      return mem;
    }
    return dbMsg;
  });
  const final = [...merged, ...memoryOnly];
  console.debug(
    "[hydrate] conv=",
    conversationId,
    "db_count=",
    dbChatMessages.length,
    "memory_count=",
    memoryMessages.length,
    "memoryOnly_count=",
    memoryOnly.length,
    "merged_count=",
    final.length,
    "assistant_contents=",
    final.filter((m) => m.role === "assistant").map((m) => ({
      id: m.id,
      len: m.content.length,
      hasThinking: !!m.thinking,
    }))
  );
  useConversationStore.getState().setMessagesForConversation(conversationId, final);
}
