import { create } from "zustand";

export type Conversation = {
  id: string;
  title: string;
  provider_id: string;
  user_id: string | null;
  created_at: number;
  updated_at: number;
};

export type ChatMessage = {
  id: string;
  conversationId: string;
  role: "user" | "assistant";
  content: string;
};

export type PendingApproval = {
  approvalId: string;
  conversationId: string;
  messageId: string;
  actionType: string;
  path: string;
  content?: string;
};

type ConversationState = {
  currentConversationId: string | null;
  conversations: Conversation[];
  messagesByConversation: Record<string, ChatMessage[]>;
  pendingApprovals: PendingApproval[];
  approvalBusy: Record<string, boolean>;
  activeConversationId: string | null;
  activeMessageId: string | null;
  isStreaming: boolean;
  error: string | null;
  setCurrentConversation: (conversationId: string | null) => void;
  setConversations: (conversations: Conversation[]) => void;
  setMessagesForConversation: (conversationId: string, messages: ChatMessage[]) => void;
  upsertMessage: (message: ChatMessage) => void;
  appendDelta: (conversationId: string, messageId: string, delta: string) => void;
  setStreaming: (conversationId: string | null, messageId: string | null, streaming: boolean) => void;
  clearStreaming: () => void;
  setError: (error: string | null) => void;
  upsertPendingApproval: (approval: PendingApproval) => void;
  resolveApproval: (approvalId: string) => void;
  setApprovalBusy: (approvalId: string, busy: boolean) => void;
};

export const useConversationStore = create<ConversationState>((set) => ({
  currentConversationId: null,
  conversations: [],
  messagesByConversation: {},
  pendingApprovals: [],
  approvalBusy: {},
  activeConversationId: null,
  activeMessageId: null,
  isStreaming: false,
  error: null,
  setCurrentConversation: (conversationId) => set({ currentConversationId: conversationId }),
  setConversations: (conversations) => set({ conversations }),
  setMessagesForConversation: (conversationId, messages) =>
    set((state) => ({
      messagesByConversation: {
        ...state.messagesByConversation,
        [conversationId]: messages,
      },
    })),
  upsertMessage: (message) =>
    set((state) => {
      const existing = state.messagesByConversation[message.conversationId] ?? [];
      const messageIndex = existing.findIndex((item) => item.id === message.id);
      if (messageIndex === -1) {
        return {
          messagesByConversation: {
            ...state.messagesByConversation,
            [message.conversationId]: [...existing, message],
          },
        };
      }

      const next = [...existing];
      next[messageIndex] = message;
      return {
        messagesByConversation: {
          ...state.messagesByConversation,
          [message.conversationId]: next,
        },
      };
    }),
  appendDelta: (conversationId, messageId, delta) =>
    set((state) => {
      const existing = state.messagesByConversation[conversationId] ?? [];
      const messageIndex = existing.findIndex((item) => item.id === messageId);
      if (messageIndex === -1) {
        return {
          messagesByConversation: {
            ...state.messagesByConversation,
            [conversationId]: [
              ...existing,
              {
                id: messageId,
                conversationId,
                role: "assistant",
                content: delta,
              },
            ],
          },
        };
      }

      const next = [...existing];
      const current = next[messageIndex];
      next[messageIndex] = {
        ...current,
        content: `${current.content}${delta}`,
      };
      return {
        messagesByConversation: {
          ...state.messagesByConversation,
          [conversationId]: next,
        },
      };
    }),
  setStreaming: (conversationId, messageId, streaming) =>
    set({
      activeConversationId: conversationId,
      activeMessageId: messageId,
      isStreaming: streaming,
    }),
  clearStreaming: () =>
    set({
      activeConversationId: null,
      activeMessageId: null,
      isStreaming: false,
    }),
  setError: (error) => set({ error }),
  upsertPendingApproval: (approval) =>
    set((state) => {
      const withoutCurrent = state.pendingApprovals.filter((item) => item.approvalId !== approval.approvalId);
      return {
        pendingApprovals: [...withoutCurrent, approval],
      };
    }),
  resolveApproval: (approvalId) =>
    set((state) => {
      const nextBusy = { ...state.approvalBusy };
      delete nextBusy[approvalId];
      return {
        pendingApprovals: state.pendingApprovals.filter((item) => item.approvalId !== approvalId),
        approvalBusy: nextBusy,
      };
    }),
  setApprovalBusy: (approvalId, busy) =>
    set((state) => {
      const next = { ...state.approvalBusy };
      if (busy) {
        next[approvalId] = true;
      } else {
        delete next[approvalId];
      }
      return { approvalBusy: next };
    }),
}));
