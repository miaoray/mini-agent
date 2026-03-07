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
  customResponses: Record<string, { request: unknown; response: unknown }>;
};

      const STORAGE_KEY = "__mini_agent_e2e_mock_state__";
      let idCounter = 1;
      let callbackCounter = 1;
      const callbacks = new Map<number, (payload: unknown) => void>();
      const eventListeners = new Map<string, number[]>();

      function nowTs(): number {
        return Math.floor(Date.now() / 1000);
      }

      function generateTitleFromContent(content: string): string {
        const trimmed = content.trim();
        
        if (trimmed.length === 0) {
          return "New Chat";
        }
        
        // Find the end of the first sentence
        const sentenceEndMatch = trimmed.match(/[.?!]/);
        if (sentenceEndMatch && sentenceEndMatch.index !== undefined) {
          return trimmed.substring(0, sentenceEndMatch.index + 1);
        }
        
        // If no sentence ending is found, take up to 25 characters
        if (trimmed.length <= 25) {
          return trimmed;
        }
        
        // Try to find a word boundary
        const truncated = trimmed.substring(0, 25);
        const lastSpace = truncated.lastIndexOf(' ');
        if (lastSpace > 10) {
          return truncated.substring(0, lastSpace).trimEnd() + "...";
        }
        
        return truncated + "...";
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
            return { conversations: [], messagesByConversation: {}, pendingApprovals: [], customResponses: {} };
          }
          const parsed = JSON.parse(raw) as Partial<MockState>;
          return {
            conversations: parsed.conversations ?? [],
            messagesByConversation: parsed.messagesByConversation ?? {},
            pendingApprovals: parsed.pendingApprovals ?? [],
            customResponses: parsed.customResponses ?? {},
          };
        } catch {
          return { conversations: [], messagesByConversation: {}, pendingApprovals: [], customResponses: {} };
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
          case "__set_custom_response__": {
            const requestPattern = String(args.requestPattern ?? "");
            const responseJson = String(args.responseJson ?? "{}");
            const state = loadState();
            if (!state.customResponses) {
              state.customResponses = {};
            }
            state.customResponses[requestPattern] = {
              request: requestPattern,
              response: JSON.parse(responseJson),
            };
            saveState(state);
            return true;
          }
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
            const providedAssistantMessageId = String(args.assistantMessageId ?? args.assistant_message_id ?? "");
            const providedUserMessageId = String(args.userMessageId ?? args.user_message_id ?? "");
            if (!conversationId) {
              throw new Error("missing required key conversationId");
            }

            const state = loadState();
            const timestamp = nowTs();
            const userMessageId = providedUserMessageId || nextId("user");
            const assistantMessageId = providedAssistantMessageId || nextId("assistant");
            
            // Check if this is the first message in the conversation
            const existingMessages = state.messagesByConversation[conversationId] ?? [];
            const isFirstMessage = existingMessages.length === 0;

            let assistantContent = "";
            let thinkingContent = "";
            let useCustomResponse = false;
            let toolUseDetected = false;
            let toolName = "";

            const customResponses = state.customResponses ?? {};
            const keys = Object.keys(customResponses);
            for (const key of keys) {
              if (userContent.toLowerCase().includes(key.toLowerCase())) {
                const custom = customResponses[key];
                const resp = custom.response as {
                  content?: Array<{ thinking?: string; text?: string; type: string; name?: string; input?: object }>;
                };
                if (resp.content) {
                  for (const block of resp.content) {
                    if (block.type === "thinking") {
                      thinkingContent = block.thinking ?? "";
                    } else if (block.type === "text") {
                      assistantContent += block.text ?? "";
                    } else if (block.type === "tool_use") {
                      toolUseDetected = true;
                      toolName = block.name ?? "unknown_tool";
                    }
                  }
                  useCustomResponse = true;
                }
                break;
              }
            }

            if (!useCustomResponse) {
              assistantContent = `Mock assistant reply: ${userContent}`;
            }

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
            
            // Generate title if this is the first message
            let newTitle: string | null = null;
            if (isFirstMessage) {
              newTitle = generateTitleFromContent(userContent);
              state.conversations = state.conversations.map((conversation) =>
                conversation.id === conversationId
                  ? { ...conversation, title: newTitle!, updated_at: timestamp + 1 }
                  : conversation
              );
            } else {
              state.conversations = state.conversations.map((conversation) =>
                conversation.id === conversationId
                  ? { ...conversation, updated_at: timestamp + 1 }
                  : conversation
              );
            }
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

            if (thinkingContent) {
              window.setTimeout(() => {
                console.debug(
                  "[mockTauri][chat-thinking] EMIT conversation_id=",
                  conversationId,
                  "message_id=",
                  assistantMessageId,
                  "thinking_len=",
                  thinkingContent.length,
                  "preview=",
                  thinkingContent.slice(0, 80)
                );
                emit("chat-thinking", {
                  conversation_id: conversationId,
                  message_id: assistantMessageId,
                  thinking: thinkingContent,
                });
              }, 5);
            }

            // Emit title update event if this is the first message
            if (newTitle) {
              window.setTimeout(() => {
                console.debug(
                  "[mockTauri][conversation-title-updated] EMIT conversation_id=",
                  conversationId,
                  "title=",
                  newTitle
                );
                emit("conversation-title-updated", {
                  conversation_id: conversationId,
                  title: newTitle,
                });
              }, 5);
            }

            // Handle tool_use detection - simulate immediate execution for tools like get_time
            if (toolUseDetected) {
              const toolResult = JSON.stringify({
                iso: new Date().toISOString(),
                human_readable: new Date().toLocaleString(),
                unix_timestamp: Math.floor(Date.now() / 1000),
                timezone: "UTC",
                utc_offset: "+00"
              });
              
              window.setTimeout(() => {
                console.debug(
                  "[mockTauri][tool-exec] EMIT tool=", toolName, "result=", toolResult
                );
                // Emit a chat-done event with the tool execution result
                emit("chat-done", {
                  conversation_id: conversationId,
                  message_id: assistantMessageId,
                  content: `Called tool ${toolName}()\nTool result: ${toolResult}`,
                  hasThinking: !!thinkingContent,
                });
                const nextState = loadState();
                const messages = nextState.messagesByConversation[conversationId] ?? [];
                nextState.messagesByConversation[conversationId] = messages.map((message) =>
                  message.id === assistantMessageId ? { ...message, content: `Called tool ${toolName}()\nTool result: ${toolResult}` } : message
                );
                saveState(nextState);
              }, 20);
            } else {
              window.setTimeout(() => {
                emit("chat-done", {
                  conversation_id: conversationId,
                  message_id: assistantMessageId,
                  content: assistantContent,
                  hasThinking: !!thinkingContent,
                });
                const nextState = loadState();
                const messages = nextState.messagesByConversation[conversationId] ?? [];
                nextState.messagesByConversation[conversationId] = messages.map((message) =>
                  message.id === assistantMessageId ? { ...message, content: assistantContent } : message
                );
                saveState(nextState);
              }, 15);
            }
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
              emit("chat-done", {
                conversation_id: approval.conversation_id,
                message_id: approval.message_id,
                content: approvedReply,
                hasThinking: false,
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
              emit("chat-done", {
                conversation_id: approval.conversation_id,
                message_id: approval.message_id,
                content: rejectedReply,
                hasThinking: false,
              });
            }, 5);
            return null;
          }
          case "get_debug_mode":
            return false;
          case "set_debug_mode":
            return null;
          case "list_debug_logs":
            return [];
          case "clear_all_conversations": {
            const state = loadState();
            state.conversations = [];
            state.messagesByConversation = {};
            saveState(state);
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
