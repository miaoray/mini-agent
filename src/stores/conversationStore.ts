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
  /** LLM thinking content, shown in collapsible section for assistant messages */
  thinking?: string;
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
  activeThinking: string | null;
  /** Conversation ID for activeThinking; only show when it matches currentConversationId */
  activeThinkingConversationId: string | null;
  /** Deferred message from chat-done; flushed when thinking typewriter completes */
  pendingChatDone: { conversationId: string; messageId: string; content: string; thinking?: string } | null;
  error: string | null;
  setCurrentConversation: (conversationId: string | null) => void;
  setConversations: (conversations: Conversation[]) => void;
  setMessagesForConversation: (conversationId: string, messages: ChatMessage[]) => void;
  upsertMessage: (message: ChatMessage) => void;
  appendDelta: (conversationId: string, messageId: string, delta: string) => void;
  replaceDelta: (conversationId: string, messageId: string, content: string) => void;
  setStreaming: (conversationId: string | null, messageId: string | null, streaming: boolean) => void;
  clearStreaming: () => void;
  setActiveThinking: (thinking: string | null, conversationId?: string | null) => void;
  /** Append thinking for same conversation (multi-turn); replaces if different conv */
  appendActiveThinking: (thinking: string, conversationId: string) => void;
  setPendingChatDone: (pending: { conversationId: string; messageId: string; content: string; thinking?: string } | null) => void;
  /** Flush pending message; returns conversationId if flushed for hydrate */
  flushPendingMessage: () => string | null;
  setWaiting: (waiting: boolean) => void;
  setError: (error: string | null) => void;
  upsertPendingApproval: (approval: PendingApproval) => void;
  resolveApproval: (approvalId: string) => void;
  setApprovalBusy: (approvalId: string, busy: boolean) => void;
  clearMessages: () => void;
};

export const useConversationStore = create<ConversationState>((set, get) => ({
  currentConversationId: null,
  conversations: [],
  messagesByConversation: {},
  pendingApprovals: [],
  approvalBusy: {},
  activeConversationId: null,
  activeMessageId: null,
  isStreaming: false,
  activeThinking: null,
  activeThinkingConversationId: null,
  pendingChatDone: null,
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
      const messages = state.messagesByConversation[conversationId] ?? [];
      const messageIndex = messages.findIndex((item) => item.id === messageId);
      if (messageIndex === -1) return state;
      const next = [...messages];
      next[messageIndex] = {
        ...next[messageIndex],
        content: next[messageIndex].content + delta,
      };
      return {
        messagesByConversation: {
          ...state.messagesByConversation,
          [conversationId]: next,
        },
      };
    }),
  replaceDelta: (conversationId, messageId, content) =>
    set((state) => {
      const messages = state.messagesByConversation[conversationId] ?? [];
      const messageIndex = messages.findIndex((item) => item.id === messageId);
      if (messageIndex === -1) return state;
      const next = [...messages];
      next[messageIndex] = { ...next[messageIndex], content };
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
      activeThinking: null,
      activeThinkingConversationId: null,
    }),
  setActiveThinking: (thinking, conversationId) =>
    set({
      activeThinking: thinking,
      activeThinkingConversationId: thinking != null ? (conversationId ?? null) : null,
    }),
  appendActiveThinking: (thinking, conversationId) =>
    set((state) => {
      if (!thinking) return state;
      const sameConv = state.activeThinkingConversationId === conversationId && state.activeThinking;
      const next = sameConv
        ? `${state.activeThinking}\n\n${thinking}`
        : thinking;
      return {
        activeThinking: next,
        activeThinkingConversationId: conversationId,
      };
    }),
  setPendingChatDone: (pending) => set({ pendingChatDone: pending }),
  flushPendingMessage: (): string | null => {
    const state = get();
    const pending = state.pendingChatDone;
    if (!pending) return null;
    const existing = state.messagesByConversation[pending.conversationId] ?? [];
    const messageIndex = existing.findIndex((item: ChatMessage) => item.id === pending.messageId);
    const next =
      messageIndex >= 0
        ? existing.map((m: ChatMessage, i: number) =>
            i === messageIndex
              ? { ...m, content: pending.content, thinking: pending.thinking }
              : m
          )
        : [
            ...existing,
            {
              id: pending.messageId,
              conversationId: pending.conversationId,
              role: "assistant" as const,
              content: pending.content,
              thinking: pending.thinking,
            },
          ];
    set({
      pendingChatDone: null,
      activeThinking: null,
      activeThinkingConversationId: null,
      messagesByConversation: {
        ...state.messagesByConversation,
        [pending.conversationId]: next,
      },
    });
    return pending.conversationId;
  },
  setWaiting: (waiting) => set({ isStreaming: waiting }),
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
  clearMessages: () =>
    set({
      messagesByConversation: {},
      pendingApprovals: [],
      approvalBusy: {},
      currentConversationId: null,
      activeConversationId: null,
      activeMessageId: null,
      isStreaming: false,
      activeThinking: null,
      activeThinkingConversationId: null,
      pendingChatDone: null,
      error: null,
    }),
}));
