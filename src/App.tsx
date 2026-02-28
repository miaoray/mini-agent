import { useEffect, useRef, useState } from "react";
import reactLogo from "./assets/react.svg";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import ApprovalCard from "./components/ApprovalCard";
import "./App.css";

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
  status?: string;
};

type ApprovalCardState = {
  approvalId: string;
  actionType: string;
  path: string;
  content?: string;
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

function App() {
  const [greetMsg, setGreetMsg] = useState("");
  const [name, setName] = useState("");
  const [streamedText, setStreamedText] = useState("");
  const [isStreaming, setIsStreaming] = useState(false);
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [activeConversationId, setActiveConversationId] = useState<string | null>(null);
  const [activeMessageId, setActiveMessageId] = useState<string | null>(null);
  const [pendingApprovals, setPendingApprovals] = useState<ApprovalCardState[]>([]);
  const [approvalBusy, setApprovalBusy] = useState<Record<string, boolean>>({});
  const activeConversationIdRef = useRef<string | null>(null);
  const activeMessageIdRef = useRef<string | null>(null);

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
          if (
            payloadConversationId(payload) !== activeConversationIdRef.current ||
            payloadMessageId(payload) !== activeMessageIdRef.current
          ) {
            return;
          }
          setIsStreaming(true);
          setStreamedText((prev) => prev + (payload?.delta ?? ""));
        });
        unlistenDone = await listen<TurnEventPayload>("chat-done", (event) => {
          if (!mounted) {
            return;
          }
          const payload = event.payload;
          if (
            payloadConversationId(payload) !== activeConversationIdRef.current ||
            payloadMessageId(payload) !== activeMessageIdRef.current
          ) {
            return;
          }
          setIsStreaming(false);
          activeConversationIdRef.current = null;
          activeMessageIdRef.current = null;
          setActiveConversationId(null);
          setActiveMessageId(null);
        });
        unlistenError = await listen<ChatErrorPayload>("chat-error", (event) => {
          if (!mounted) {
            return;
          }
          const payload = event.payload;
          if (
            payloadConversationId(payload) !== activeConversationIdRef.current ||
            payloadMessageId(payload) !== activeMessageIdRef.current
          ) {
            return;
          }
          setIsStreaming(false);
          activeConversationIdRef.current = null;
          activeMessageIdRef.current = null;
          setActiveConversationId(null);
          setActiveMessageId(null);
          setStreamedText(`Error: ${payload?.message ?? "Unknown error"}`);
        });
        unlistenPendingApproval = await listen<PendingApprovalPayload>(
          "pending-approval",
          (event) => {
            if (!mounted) {
              return;
            }
            const payload = event.payload;
            const approvalId = payloadApprovalId(payload);
            if (!approvalId) {
              return;
            }
            const actionType = payloadActionType(payload);
            const path = payload?.payload?.path ?? "";
            const content = payload?.payload?.content;
            setPendingApprovals((previous) => {
              const next = previous.filter((item) => item.approvalId !== approvalId);
              next.push({
                approvalId,
                actionType,
                path,
                content,
              });
              return next;
            });
          },
        );
        unlistenApprovalResolved = await listen<ApprovalResolvedPayload>(
          "approval-resolved",
          (event) => {
            if (!mounted) {
              return;
            }
            const approvalId = payloadApprovalId(event.payload);
            if (!approvalId) {
              return;
            }
            setPendingApprovals((previous) =>
              previous.filter((item) => item.approvalId !== approvalId),
            );
            setApprovalBusy((previous) => {
              const next = { ...previous };
              delete next[approvalId];
              return next;
            });
          },
        );
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
  }, []);

  async function sendMessage(content: string) {
    activeConversationIdRef.current = null;
    activeMessageIdRef.current = null;
    setActiveConversationId(null);
    setActiveMessageId(null);
    const nextConversationId =
      conversationId ??
      (await invoke<string>("create_conversation").then((id) => {
        setConversationId(id);
        return id;
      }));
    const nextMessageId = await invoke<string>("send_message", {
      conversation_id: nextConversationId,
      content,
    });
    activeConversationIdRef.current = nextConversationId;
    activeMessageIdRef.current = nextMessageId;
    setActiveConversationId(nextConversationId);
    setActiveMessageId(nextMessageId);
    setIsStreaming(true);
    setGreetMsg("Message sent");
  }

  async function approvePending(approvalId: string) {
    setApprovalBusy((previous) => ({ ...previous, [approvalId]: true }));
    try {
      await invoke("approve_action", { approval_id: approvalId });
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setGreetMsg(`Error: ${message}`);
      setApprovalBusy((previous) => {
        const next = { ...previous };
        delete next[approvalId];
        return next;
      });
    }
  }

  async function rejectPending(approvalId: string) {
    setApprovalBusy((previous) => ({ ...previous, [approvalId]: true }));
    try {
      await invoke("reject_action", { approval_id: approvalId });
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setGreetMsg(`Error: ${message}`);
      setApprovalBusy((previous) => {
        const next = { ...previous };
        delete next[approvalId];
        return next;
      });
    }
  }

  return (
    <main className="container">
      <h1>Welcome to Tauri + React</h1>

      <div className="row">
        <a href="https://vite.dev" target="_blank">
          <img src="/vite.svg" className="logo vite" alt="Vite logo" />
        </a>
        <a href="https://tauri.app" target="_blank">
          <img src="/tauri.svg" className="logo tauri" alt="Tauri logo" />
        </a>
        <a href="https://react.dev" target="_blank">
          <img src={reactLogo} className="logo react" alt="React logo" />
        </a>
      </div>
      <p>Click on the Tauri, Vite, and React logos to learn more.</p>

      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          setStreamedText("");
          setIsStreaming(false);
          void sendMessage(name).catch((error: unknown) => {
            const message = error instanceof Error ? error.message : String(error);
            setGreetMsg(`Error: ${message}`);
          });
        }}
      >
        <input
          id="greet-input"
          onChange={(e) => setName(e.currentTarget.value)}
          placeholder="Enter a name..."
        />
        <button type="submit">Greet</button>
      </form>
      <p>{greetMsg}</p>
      {pendingApprovals.length > 0 ? (
        <section className="approval-list">
          {pendingApprovals.map((approval) => (
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
      <p data-testid="streamed-text">{streamedText}</p>
      {isStreaming ? <p>Streaming...</p> : null}
    </main>
  );
}

export default App;
