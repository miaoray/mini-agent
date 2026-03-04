import { FormEvent, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import ApprovalCard from "./ApprovalCard";
import MessageList from "./MessageList";
import { useConversationStore, type Conversation } from "../stores/conversationStore";

export default function ChatView() {
  const [input, setInput] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const {
    currentConversationId,
    messagesByConversation,
    pendingApprovals,
    approvalBusy,
    isStreaming,
    activeThinking,
    error,
    setCurrentConversation,
    setConversations,
    upsertMessage,
    setWaiting,
    setActiveThinking,
    setError,
    setApprovalBusy,
  } = useConversationStore((state) => state);

  const visibleMessages = currentConversationId ? (messagesByConversation[currentConversationId] ?? []) : [];
  const visibleApprovals = currentConversationId
    ? pendingApprovals.filter((item) => item.conversationId === currentConversationId)
    : pendingApprovals;

  async function refreshConversations() {
    const conversations = await invoke<Conversation[]>("list_conversations");
    setConversations(conversations);
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const content = input.trim();
    if (!content) {
      return;
    }
    if (isStreaming || isSubmitting) {
      return;
    }
    setIsSubmitting(true);
    setError(null);
    setActiveThinking(null);

    try {
      let conversationId = currentConversationId;
      if (!conversationId) {
        conversationId = await invoke<string>("create_conversation");
        setCurrentConversation(conversationId);
        await refreshConversations();
      }

      const userMessageId = `user-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
      const assistantMessageId = `assistant-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;

      upsertMessage({
        id: userMessageId,
        conversationId,
        role: "user",
        content,
      });

      upsertMessage({
        id: assistantMessageId,
        conversationId,
        role: "assistant",
        content: "",
      });

      setWaiting(true);

      await invoke<string>("send_message", {
        conversationId,
        content,
        assistantMessageId,
        userMessageId,
      });

      setInput("");
    } catch (caughtError: unknown) {
      const message = caughtError instanceof Error ? caughtError.message : String(caughtError);
      setError(`Error: ${message}`);
    } finally {
      setIsSubmitting(false);
    }
  }

  async function approvePending(approvalId: string) {
    setApprovalBusy(approvalId, true);
    try {
      await invoke("approve_action", { approvalId });
    } catch (caughtError: unknown) {
      const message = caughtError instanceof Error ? caughtError.message : String(caughtError);
      setError(`Error: ${message}`);
      setApprovalBusy(approvalId, false);
    }
  }

  async function rejectPending(approvalId: string) {
    setApprovalBusy(approvalId, true);
    try {
      await invoke("reject_action", { approvalId });
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

      {activeThinking ? (
        <section className="thinking-panel" data-testid="thinking-panel">
          <p className="thinking-label">Thinking</p>
          <pre className="thinking-content">{activeThinking}</pre>
        </section>
      ) : null}

      <MessageList messages={visibleMessages} />

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
