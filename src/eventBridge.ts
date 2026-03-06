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
/** 防止 Strict Mode double-invoke 导致重复注册；新 setup 时递增，旧 setup 的 push 会跳过 */
let listenerGeneration = 0;

/**
 * 注册 Tauri 事件监听器，与 React 生命周期解耦。
 * 在 App 挂载时调用一次，应用运行期间监听器保持稳定，避免 "Couldn't find callback id"。
 * 注意：React Strict Mode 会 double-invoke useEffect，需在 setup 前 teardown 避免重复注册。
 */
export async function setupTauriListeners(): Promise<void> {
  teardownTauriListeners();
  const myGen = ++listenerGeneration;
  try {
    const unlisten1 = await listen<TurnEventPayload & { thinking?: string }>("chat-thinking", (event) => {
        const payload = event.payload;
        const thinking = (payload as { thinking?: string }).thinking ?? "";
        const convId = payloadConversationId(payload);
        const store = useConversationStore.getState();
        const existing = store.activeThinking;
        if (existing && store.activeThinkingConversationId === convId) {
          if (thinking.startsWith(existing)) {
            store.setActiveThinking(thinking, convId);
          } else {
            store.appendActiveThinking(thinking, convId);
          }
        } else {
          store.setActiveThinking(thinking, convId);
        }
      });
    if (myGen !== listenerGeneration) {
      unlisten1();
      return;
    }
    unlisteners.push(unlisten1);

    const unlisten2 = await listen<TurnEventPayload & { content?: string; hasThinking?: boolean }>("chat-done", (event) => {
        const payload = event.payload;
        const nextConversationId = payloadConversationId(payload);
        const nextMessageId = payloadMessageId(payload);
        const content = payload?.content ?? "";
        const hasThinking = payload?.hasThinking ?? false;
        const store = useConversationStore.getState();
        const thinking = hasThinking ? (store.activeThinking ?? undefined) : undefined;
        store.setWaiting(false);
        if (thinking) {
          // Defer upsert until thinking typewriter completes; keep activeThinking for effect
          store.setPendingChatDone({
            conversationId: nextConversationId,
            messageId: nextMessageId,
            content,
            thinking,
          });
        } else {
          store.upsertMessage({
            id: nextMessageId,
            conversationId: nextConversationId,
            role: "assistant",
            content,
          });
          void hydrateConversationMessages(nextConversationId);
        }
      });
    if (myGen !== listenerGeneration) {
      unlisten1();
      unlisten2();
      return;
    }
    unlisteners.push(unlisten2);

    const unlisten3 = await listen<ChatErrorPayload>("chat-error", (event) => {
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
        store.setPendingChatDone(null);
        store.setWaiting(false);
      });
    if (myGen !== listenerGeneration) {
      unlisten1();
      unlisten2();
      unlisten3();
      return;
    }
    unlisteners.push(unlisten3);

    const unlisten4 = await listen<PendingApprovalPayload>("pending-approval", (event) => {
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
      });
    if (myGen !== listenerGeneration) {
      unlisten1();
      unlisten2();
      unlisten3();
      unlisten4();
      return;
    }
    unlisteners.push(unlisten4);

    const unlisten5 = await listen<ApprovalResolvedPayload>("approval-resolved", (event) => {
        const approvalId = payloadApprovalId(event.payload);
        if (!approvalId) return;
        useConversationStore.getState().resolveApproval(approvalId);
      });
    if (myGen !== listenerGeneration) {
      unlisten1();
      unlisten2();
      unlisten3();
      unlisten4();
      unlisten5();
      return;
    }
    unlisteners.push(unlisten5);
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
