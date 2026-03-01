import type { Page } from "@playwright/test";

type InstallMockOptions = {
  hasApiKey: boolean;
};

export async function installMockTauri(page: Page, options: InstallMockOptions): Promise<void> {
  await page.addInitScript(
    ({ hasApiKey }) => {
      type Conversation = {
        id: string;
        title: string;
        provider_id: string;
        user_id: string | null;
        created_at: number;
        updated_at: number;
      };
      type Message = {
        id: string;
        conversation_id: string;
        role: "user" | "assistant";
        content: string;
        created_at: number;
      };
      type MockState = {
        conversations: Conversation[];
        messagesByConversation: Record<string, Message[]>;
        pendingApprovals: Array<{
          approval_id: string;
          conversation_id: string;
          message_id: string;
          action_type: string;
          payload: { path: string; content?: string };
        }>;
      };

      const STORAGE_KEY = "__mini_agent_e2e_mock_state__";
      let idCounter = 1;
      let callbackCounter = 1;
      const callbacks = new Map<number, (payload: unknown) => void>();
      const eventListeners = new Map<string, number[]>();

      function nowTs(): number {
        return Math.floor(Date.now() / 1000);
      }

      function nextId(prefix: string): string {
        const id = `${prefix}-${Date.now()}-${idCounter}`;
        idCounter += 1;
        return id;
      }

      function loadState(): MockState {
        try {
          const raw = window.localStorage.getItem(STORAGE_KEY);
          if (!raw) {
            return { conversations: [], messagesByConversation: {}, pendingApprovals: [] };
          }
          const parsed = JSON.parse(raw) as Partial<MockState>;
          return {
            conversations: parsed.conversations ?? [],
            messagesByConversation: parsed.messagesByConversation ?? {},
            pendingApprovals: parsed.pendingApprovals ?? [],
          };
        } catch {
          return { conversations: [], messagesByConversation: {}, pendingApprovals: [] };
        }
      }

      function saveState(state: MockState): void {
        window.localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
      }

      function emit(eventName: string, payload: unknown): void {
        const listeners = eventListeners.get(eventName) ?? [];
        for (const callbackId of listeners) {
          const callback = callbacks.get(callbackId);
          if (callback) {
            callback({ event: eventName, id: callbackId, payload });
          }
        }
      }

      function ensureInternals(): void {
        const internalRoot = (window as unknown as { __TAURI_INTERNALS__?: Record<string, unknown> })
          .__TAURI_INTERNALS__;
        if (!internalRoot) {
          (window as unknown as { __TAURI_INTERNALS__: Record<string, unknown> }).__TAURI_INTERNALS__ = {};
        }
        const eventRoot = (
          window as unknown as { __TAURI_EVENT_PLUGIN_INTERNALS__?: Record<string, unknown> }
        ).__TAURI_EVENT_PLUGIN_INTERNALS__;
        if (!eventRoot) {
          (
            window as unknown as { __TAURI_EVENT_PLUGIN_INTERNALS__: Record<string, unknown> }
          ).__TAURI_EVENT_PLUGIN_INTERNALS__ = {};
        }
      }

      ensureInternals();
      (window as unknown as { isTauri?: boolean }).isTauri = true;

      (
        window as unknown as { __TAURI_INTERNALS__: Record<string, unknown> }
      ).__TAURI_INTERNALS__.transformCallback = (callback: (payload: unknown) => void, once = false) => {
        const callbackId = callbackCounter;
        callbackCounter += 1;
        callbacks.set(callbackId, (payload) => {
          if (once) {
            callbacks.delete(callbackId);
          }
          callback(payload);
        });
        return callbackId;
      };

      (
        window as unknown as { __TAURI_INTERNALS__: Record<string, unknown> }
      ).__TAURI_INTERNALS__.unregisterCallback = (callbackId: number) => {
        callbacks.delete(callbackId);
      };

      (
        window as unknown as { __TAURI_EVENT_PLUGIN_INTERNALS__: Record<string, unknown> }
      ).__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener = (eventName: string, callbackId: number) => {
        const listeners = eventListeners.get(eventName) ?? [];
        eventListeners.set(
          eventName,
          listeners.filter((id) => id !== callbackId)
        );
      };

      (
        window as unknown as { __TAURI_INTERNALS__: Record<string, unknown> }
      ).__TAURI_INTERNALS__.invoke = async (cmd: string, args: Record<string, unknown> = {}) => {
        if (cmd === "plugin:event|listen") {
          const eventName = String(args.event ?? "");
          const handlerId = Number(args.handler);
          const listeners = eventListeners.get(eventName) ?? [];
          listeners.push(handlerId);
          eventListeners.set(eventName, listeners);
          return handlerId;
        }

        if (cmd === "plugin:event|unlisten") {
          const eventName = String(args.event ?? "");
          const eventId = Number(args.eventId);
          const listeners = eventListeners.get(eventName) ?? [];
          eventListeners.set(
            eventName,
            listeners.filter((id) => id !== eventId)
          );
          return null;
        }

        switch (cmd) {
          case "check_config":
            return { hasApiKey };
          case "list_conversations": {
            const state = loadState();
            return state.conversations;
          }
          case "create_conversation": {
            const state = loadState();
            const conversationId = nextId("conv");
            const timestamp = nowTs();
            state.conversations.unshift({
              id: conversationId,
              title: "New Chat",
              provider_id: "minimax",
              user_id: null,
              created_at: timestamp,
              updated_at: timestamp,
            });
            state.messagesByConversation[conversationId] = state.messagesByConversation[conversationId] ?? [];
            saveState(state);
            return conversationId;
          }
          case "list_messages": {
            const conversationId = String(args.conversationId ?? args.conversation_id ?? "");
            const state = loadState();
            return state.messagesByConversation[conversationId] ?? [];
          }
          case "send_message": {
            const conversationId = String(args.conversationId ?? args.conversation_id ?? "");
            const userContent = String(args.content ?? "");
            if (!conversationId) {
              throw new Error("missing required key conversationId");
            }

            const state = loadState();
            const timestamp = nowTs();
            const userMessageId = nextId("user");
            const assistantMessageId = nextId("assistant");
            const assistantContent = `Mock assistant reply: ${userContent}`;
            const half = Math.max(1, Math.floor(assistantContent.length / 2));
            const firstChunk = assistantContent.slice(0, half);
            const secondChunk = assistantContent.slice(half);

            state.messagesByConversation[conversationId] = [
              ...(state.messagesByConversation[conversationId] ?? []),
              {
                id: userMessageId,
                conversation_id: conversationId,
                role: "user",
                content: userContent,
                created_at: timestamp,
              },
              {
                id: assistantMessageId,
                conversation_id: conversationId,
                role: "assistant",
                content: "",
                created_at: timestamp + 1,
              },
            ];
            state.conversations = state.conversations.map((conversation) =>
              conversation.id === conversationId
                ? { ...conversation, updated_at: timestamp + 1 }
                : conversation
            );
            saveState(state);

            if (userContent.toLowerCase().includes("write file")) {
              const approvalId = nextId("approval");
              const approvalPayload = {
                path: "notes/todo.txt",
                content: "first line\nsecond line",
              };
              const nextState = loadState();
              nextState.pendingApprovals.push({
                approval_id: approvalId,
                conversation_id: conversationId,
                message_id: assistantMessageId,
                action_type: "write_file",
                payload: approvalPayload,
              });
              saveState(nextState);

              window.setTimeout(() => {
                emit("pending-approval", {
                  approval_id: approvalId,
                  conversation_id: conversationId,
                  message_id: assistantMessageId,
                  action_type: "write_file",
                  payload: approvalPayload,
                });
              }, 5);
              return assistantMessageId;
            }

            window.setTimeout(() => {
              emit("chat-delta", {
                conversation_id: conversationId,
                message_id: assistantMessageId,
                delta: firstChunk,
              });
            }, 5);
            window.setTimeout(() => {
              emit("chat-delta", {
                conversation_id: conversationId,
                message_id: assistantMessageId,
                delta: secondChunk,
              });
              emit("chat-done", {
                conversation_id: conversationId,
                message_id: assistantMessageId,
              });
              const nextState = loadState();
              const messages = nextState.messagesByConversation[conversationId] ?? [];
              nextState.messagesByConversation[conversationId] = messages.map((message) =>
                message.id === assistantMessageId ? { ...message, content: assistantContent } : message
              );
              saveState(nextState);
            }, 15);
            return assistantMessageId;
          }
          case "approve_action": {
            const approvalId = String(args.approvalId ?? args.approval_id ?? "");
            const state = loadState();
            const approval = state.pendingApprovals.find((item) => item.approval_id === approvalId);
            if (!approval) {
              throw new Error(`approval not found: ${approvalId}`);
            }
            state.pendingApprovals = state.pendingApprovals.filter((item) => item.approval_id !== approvalId);

            const approvedReply = `Approved and completed: ${approval.action_type} ${approval.payload.path}`;
            const messages = state.messagesByConversation[approval.conversation_id] ?? [];
            state.messagesByConversation[approval.conversation_id] = messages.map((message) =>
              message.id === approval.message_id ? { ...message, content: approvedReply } : message
            );
            saveState(state);

            window.setTimeout(() => {
              emit("approval-resolved", {
                approval_id: approval.approval_id,
                conversation_id: approval.conversation_id,
                message_id: approval.message_id,
                status: "approved",
              });
              emit("chat-delta", {
                conversation_id: approval.conversation_id,
                message_id: approval.message_id,
                delta: approvedReply,
              });
              emit("chat-done", {
                conversation_id: approval.conversation_id,
                message_id: approval.message_id,
              });
            }, 5);
            return null;
          }
          case "reject_action": {
            const approvalId = String(args.approvalId ?? args.approval_id ?? "");
            const state = loadState();
            const approval = state.pendingApprovals.find((item) => item.approval_id === approvalId);
            if (!approval) {
              throw new Error(`approval not found: ${approvalId}`);
            }
            state.pendingApprovals = state.pendingApprovals.filter((item) => item.approval_id !== approvalId);

            const rejectedReply = `User rejected action: ${approval.action_type} ${approval.payload.path}`;
            const messages = state.messagesByConversation[approval.conversation_id] ?? [];
            state.messagesByConversation[approval.conversation_id] = messages.map((message) =>
              message.id === approval.message_id ? { ...message, content: rejectedReply } : message
            );
            saveState(state);

            window.setTimeout(() => {
              emit("approval-resolved", {
                approval_id: approval.approval_id,
                conversation_id: approval.conversation_id,
                message_id: approval.message_id,
                status: "rejected",
              });
              emit("chat-delta", {
                conversation_id: approval.conversation_id,
                message_id: approval.message_id,
                delta: rejectedReply,
              });
              emit("chat-done", {
                conversation_id: approval.conversation_id,
                message_id: approval.message_id,
              });
            }, 5);
            return null;
          }
          default:
            throw new Error(`mock invoke not implemented for command: ${cmd}`);
        }
      };
    },
    { hasApiKey: options.hasApiKey }
  );
}
