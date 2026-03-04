import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useConversationStore } from "./stores/conversationStore";
import { hydrateConversationMessages } from "./lib/conversationHydrate";

type TurnEventPayload = {
  conversation_id?: string;
  conversationId?: string;
  message_id?: string;
  messageId?: string;
};

type ChatErrorPayload = TurnEventPayload & { message?: string };
type PendingApprovalPayload = TurnEventPayload & {
  approval_id?: string;
  approvalId?: string;
  action_type?: string;
  actionType?: string;
  payload?: {
    path?: string;
    content?: string;
  };
};
type ApprovalResolvedPayload = TurnEventPayload & {
  approval_id?: string;
  approvalId?: string;
};

function payloadConversationId(payload: TurnEventPayload | null | undefined) {
  return payload?.conversationId ?? payload?.conversation_id ?? "";
}

function payloadMessageId(payload: TurnEventPayload | null | undefined) {
  return payload?.messageId ?? payload?.message_id ?? "";
}

function payloadApprovalId(
  payload: PendingApprovalPayload | ApprovalResolvedPayload | null | undefined
) {
  return payload?.approvalId ?? payload?.approval_id ?? "";
}

function payloadActionType(payload: PendingApprovalPayload | null | undefined) {
  return payload?.actionType ?? payload?.action_type ?? "";
}

const unlisteners: UnlistenFn[] = [];

/**
 * 注册 Tauri 事件监听器，与 React 生命周期解耦。
 * 在 App 挂载时调用一次，应用运行期间监听器保持稳定，避免 "Couldn't find callback id"。
 */
export async function setupTauriListeners(): Promise<void> {
  try {
    unlisteners.push(
      await listen<TurnEventPayload & { thinking?: string }>("chat-thinking", (event) => {
        const payload = event.payload;
        const thinking = (payload as { thinking?: string }).thinking ?? "";
        console.debug(
          "[chat-thinking] RECV conversationId=",
          payloadConversationId(payload),
          "messageId=",
          payloadMessageId(payload),
          "thinking_len=",
          thinking.length,
          "preview=",
          thinking.slice(0, 80)
        );
        useConversationStore.getState().setActiveThinking(thinking);
      })
    );

    unlisteners.push(
      await listen<TurnEventPayload & { content?: string }>("chat-done", (event) => {
        const payload = event.payload;
        const nextConversationId = payloadConversationId(payload);
        const nextMessageId = payloadMessageId(payload);
        const content = payload?.content ?? "";
        const thinking = useConversationStore.getState().activeThinking ?? undefined;
        const store = useConversationStore.getState();
        store.upsertMessage({
          id: nextMessageId,
          conversationId: nextConversationId,
          role: "assistant",
          content,
          thinking: thinking || undefined,
        });
        store.setActiveThinking(null);
        store.setWaiting(false);
        void hydrateConversationMessages(nextConversationId);
      })
    );

    unlisteners.push(
      await listen<ChatErrorPayload>("chat-error", (event) => {
        const payload = event.payload;
        const nextConversationId = payloadConversationId(payload);
        const nextMessageId = payloadMessageId(payload);
        const message = payload?.message ?? "Unknown error";
        const store = useConversationStore.getState();
        store.upsertMessage({
          id: nextMessageId,
          conversationId: nextConversationId,
          role: "assistant",
          content: `Error: ${message}`,
        });
        store.setError(`Error: ${message}`);
        store.setActiveThinking(null);
        store.setWaiting(false);
      })
    );

    unlisteners.push(
      await listen<PendingApprovalPayload>("pending-approval", (event) => {
        const payload = event.payload;
        const approvalId = payloadApprovalId(payload);
        if (!approvalId) return;
        useConversationStore.getState().upsertPendingApproval({
          approvalId,
          conversationId: payloadConversationId(payload),
          messageId: payloadMessageId(payload),
          actionType: payloadActionType(payload),
          path: payload?.payload?.path ?? "",
          content: payload?.payload?.content,
        });
      })
    );

    unlisteners.push(
      await listen<ApprovalResolvedPayload>("approval-resolved", (event) => {
        const approvalId = payloadApprovalId(event.payload);
        if (!approvalId) return;
        useConversationStore.getState().resolveApproval(approvalId);
      })
    );
  } catch {
    // Ignore in non-Tauri environments (e.g. browser tests, e2e).
  }
}

/**
 * 注销所有 Tauri 事件监听器。在 App 卸载时调用。
 */
export function teardownTauriListeners(): void {
  for (const fn of unlisteners) {
    fn();
  }
  unlisteners.length = 0;
}
