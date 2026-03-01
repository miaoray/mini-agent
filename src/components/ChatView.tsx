import { FormEvent, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import ApprovalCard from "./ApprovalCard";
import MessageList from "./MessageList";
import { useConversationStore, type Conversation } from "../stores/conversationStore";

type TurnEventPayload = {
  conversation_id?: string;
  conversationId?: string;
  message_id?: string;
  messageId?: string;
};

type ChatDeltaPayload = TurnEventPayload & { delta?: string };
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

function payloadApprovalId(payload: PendingApprovalPayload | ApprovalResolvedPayload | null | undefined) {
  return payload?.approvalId ?? payload?.approval_id ?? "";
}

function payloadActionType(payload: PendingApprovalPayload | null | undefined) {
  return payload?.actionType ?? payload?.action_type ?? "";
}

export default function ChatView() {
  const [input, setInput] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const {
    currentConversationId,
    messagesByConversation,
    pendingApprovals,
    approvalBusy,
    activeMessageId,
    isStreaming,
    error,
    setCurrentConversation,
    setConversations,
    upsertMessage,
    appendDelta,
    setStreaming,
    clearStreaming,
    setError,
    upsertPendingApproval,
    resolveApproval,
    setApprovalBusy,
  } = useConversationStore((state) => state);

  const visibleMessages = currentConversationId ? (messagesByConversation[currentConversationId] ?? []) : [];
  const visibleApprovals = currentConversationId
    ? pendingApprovals.filter((item) => item.conversationId === currentConversationId)
    : pendingApprovals;

  useEffect(() => {
    let unlistenDelta: UnlistenFn | null = null;
    let unlistenDone: UnlistenFn | null = null;
    let unlistenError: UnlistenFn | null = null;
    let unlistenPendingApproval: UnlistenFn | null = null;
    let unlistenApprovalResolved: UnlistenFn | null = null;
    let mounted = true;

    const setupListeners = async () => {
      try {
        unlistenDelta = await listen<ChatDeltaPayload>("chat-delta", (event) => {
          if (!mounted) {
            return;
          }
          const payload = event.payload;
          const nextConversationId = payloadConversationId(payload);
          const nextMessageId = payloadMessageId(payload);
          const state = useConversationStore.getState();
          if (
            nextConversationId !== state.activeConversationId ||
            nextMessageId !== state.activeMessageId
          ) {
            return;
          }
          appendDelta(nextConversationId, nextMessageId, payload?.delta ?? "");
          setStreaming(nextConversationId, nextMessageId, true);
        });

        unlistenDone = await listen<TurnEventPayload>("chat-done", (event) => {
          if (!mounted) {
            return;
          }
          const payload = event.payload;
          const nextConversationId = payloadConversationId(payload);
          const nextMessageId = payloadMessageId(payload);
          const state = useConversationStore.getState();
          if (
            nextConversationId !== state.activeConversationId ||
            nextMessageId !== state.activeMessageId
          ) {
            return;
          }
          clearStreaming();
        });

        unlistenError = await listen<ChatErrorPayload>("chat-error", (event) => {
          if (!mounted) {
            return;
          }
          const payload = event.payload;
          const nextConversationId = payloadConversationId(payload);
          const nextMessageId = payloadMessageId(payload);
          const state = useConversationStore.getState();
          if (
            nextConversationId !== state.activeConversationId ||
            nextMessageId !== state.activeMessageId
          ) {
            return;
          }
          const message = payload?.message ?? "Unknown error";
          upsertMessage({
            id: nextMessageId,
            conversationId: nextConversationId,
            role: "assistant",
            content: `Error: ${message}`,
          });
          setError(`Error: ${message}`);
          clearStreaming();
        });

        unlistenPendingApproval = await listen<PendingApprovalPayload>("pending-approval", (event) => {
          if (!mounted) {
            return;
          }
          const payload = event.payload;
          const approvalId = payloadApprovalId(payload);
          if (!approvalId) {
            return;
          }
          upsertPendingApproval({
            approvalId,
            conversationId: payloadConversationId(payload),
            messageId: payloadMessageId(payload),
            actionType: payloadActionType(payload),
            path: payload?.payload?.path ?? "",
            content: payload?.payload?.content,
          });
        });

        unlistenApprovalResolved = await listen<ApprovalResolvedPayload>("approval-resolved", (event) => {
          if (!mounted) {
            return;
          }
          const approvalId = payloadApprovalId(event.payload);
          if (!approvalId) {
            return;
          }
          resolveApproval(approvalId);
        });
      } catch {
        // Ignore listener setup in non-Tauri environments (e.g. browser tests).
      }
    };

    void setupListeners();

    return () => {
      mounted = false;
      if (unlistenDelta) {
        unlistenDelta();
      }
      if (unlistenDone) {
        unlistenDone();
      }
      if (unlistenError) {
        unlistenError();
      }
      if (unlistenPendingApproval) {
        unlistenPendingApproval();
      }
      if (unlistenApprovalResolved) {
        unlistenApprovalResolved();
      }
    };
  }, [
    appendDelta,
    clearStreaming,
    resolveApproval,
    setError,
    setStreaming,
    upsertMessage,
    upsertPendingApproval,
  ]);

  async function refreshConversations() {
    const conversations = await invoke<Conversation[]>("list_conversations");
    setConversations(conversations);
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (isStreaming || isSubmitting) {
      return;
    }
    const content = input.trim();
    if (!content) {
      return;
    }
    setError(null);

    let conversationId = currentConversationId;
    if (!conversationId) {
      conversationId = await invoke<string>("create_conversation");
      setCurrentConversation(conversationId);
      await refreshConversations();
    }

    upsertMessage({
      id: `user-${Date.now()}`,
      conversationId,
      role: "user",
      content,
    });

    let assistantMessageId = "";
    setIsSubmitting(true);
    try {
      assistantMessageId = await invoke<string>("send_message", {
        conversation_id: conversationId,
        content,
      });
    } finally {
      setIsSubmitting(false);
    }
    upsertMessage({
      id: assistantMessageId,
      conversationId,
      role: "assistant",
      content: "",
    });
    setStreaming(conversationId, assistantMessageId, true);
    setInput("");
  }

  async function approvePending(approvalId: string) {
    setApprovalBusy(approvalId, true);
    try {
      await invoke("approve_action", { approval_id: approvalId });
    } catch (caughtError: unknown) {
      const message = caughtError instanceof Error ? caughtError.message : String(caughtError);
      setError(`Error: ${message}`);
      setApprovalBusy(approvalId, false);
    }
  }

  async function rejectPending(approvalId: string) {
    setApprovalBusy(approvalId, true);
    try {
      await invoke("reject_action", { approval_id: approvalId });
    } catch (caughtError: unknown) {
      const message = caughtError instanceof Error ? caughtError.message : String(caughtError);
      setError(`Error: ${message}`);
      setApprovalBusy(approvalId, false);
    }
  }

  return (
    <section className="chat-view">
      {visibleApprovals.length > 0 ? (
        <section className="approval-list">
          {visibleApprovals.map((approval) => (
            <ApprovalCard
              key={approval.approvalId}
              approvalId={approval.approvalId}
              actionType={approval.actionType}
              path={approval.path}
              content={approval.content}
              busy={approvalBusy[approval.approvalId] === true}
              onApprove={approvePending}
              onReject={rejectPending}
            />
          ))}
        </section>
      ) : null}

      <MessageList
        messages={visibleMessages}
        streamingMessageId={activeMessageId}
        isStreaming={isStreaming}
      />

      {error ? (
        <p role="alert" className="chat-error">
          {error}
        </p>
      ) : null}

      <form className="chat-input-row" onSubmit={handleSubmit}>
        <input
          value={input}
          onChange={(event) => setInput(event.currentTarget.value)}
          placeholder="Type a message..."
        />
        <button type="submit" disabled={isStreaming || isSubmitting}>
          Send
        </button>
      </form>
    </section>
  );
}
