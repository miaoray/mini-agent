import { useEffect, useRef, useState } from "react";
import reactLogo from "./assets/react.svg";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import "./App.css";

type TurnEventPayload = {
  conversation_id?: string;
  conversationId?: string;
  message_id?: string;
  messageId?: string;
};

type ChatDeltaPayload = TurnEventPayload & { delta?: string };
type ChatErrorPayload = TurnEventPayload & { message?: string };

function payloadConversationId(payload: TurnEventPayload | null | undefined) {
  return payload?.conversationId ?? payload?.conversation_id ?? "";
}

function payloadMessageId(payload: TurnEventPayload | null | undefined) {
  return payload?.messageId ?? payload?.message_id ?? "";
}

function App() {
  const [greetMsg, setGreetMsg] = useState("");
  const [name, setName] = useState("");
  const [streamedText, setStreamedText] = useState("");
  const [isStreaming, setIsStreaming] = useState(false);
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [activeConversationId, setActiveConversationId] = useState<string | null>(null);
  const [activeMessageId, setActiveMessageId] = useState<string | null>(null);
  const activeConversationIdRef = useRef<string | null>(null);
  const activeMessageIdRef = useRef<string | null>(null);

  useEffect(() => {
    let unlistenDelta: UnlistenFn | null = null;
    let unlistenDone: UnlistenFn | null = null;
    let unlistenError: UnlistenFn | null = null;
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
      <p data-testid="streamed-text">{streamedText}</p>
      {isStreaming ? <p>Streaming...</p> : null}
    </main>
  );
}

export default App;
