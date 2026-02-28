import { useEffect, useState } from "react";
import reactLogo from "./assets/react.svg";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import "./App.css";

function App() {
  const [greetMsg, setGreetMsg] = useState("");
  const [name, setName] = useState("");
  const [streamedText, setStreamedText] = useState("");
  const [isStreaming, setIsStreaming] = useState(false);

  useEffect(() => {
    let unlistenDelta: UnlistenFn | null = null;
    let unlistenDone: UnlistenFn | null = null;
    let mounted = true;

    const setupListeners = async () => {
      try {
        unlistenDelta = await listen<{ delta: string }>("chat-delta", (event) => {
          if (!mounted) {
            return;
          }
          setIsStreaming(true);
          setStreamedText((prev) => prev + (event.payload?.delta ?? ""));
        });
        unlistenDone = await listen("chat-done", () => {
          if (!mounted) {
            return;
          }
          setIsStreaming(false);
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
    };
  }, []);

  async function greet() {
    // Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
    setGreetMsg(await invoke("greet", { name }));
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
          greet();
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
