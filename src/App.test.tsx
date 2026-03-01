import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import App from "./App";
import { useConversationStore } from "./stores/conversationStore";

const { invokeMock, listeners } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listeners: new Map<string, (event: { payload?: unknown }) => void>(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (eventName: string, callback: (event: { payload?: unknown }) => void) => {
    listeners.set(eventName, callback);
    return () => listeners.delete(eventName);
  }),
}));

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  useConversationStore.setState({
    currentConversationId: null,
    conversations: [],
    messagesByConversation: {},
    pendingApprovals: [],
    approvalBusy: {},
    activeConversationId: null,
    activeMessageId: null,
    isStreaming: false,
    error: null,
  });
});

afterEach(() => {
  cleanup();
});

function emit(eventName: string, payload: unknown) {
  const callback = listeners.get(eventName);
  if (callback) {
    callback({ payload });
  }
}

test("renders sidebar and chat view", async () => {
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "list_conversations") {
      return [];
    }
    if (command === "list_messages") {
      return [];
    }
    return "";
  });
  render(<App />);
  expect(screen.getByRole("main")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "New Chat" })).toBeInTheDocument();
  await waitFor(() => {
    expect(invokeMock).toHaveBeenCalledWith("list_conversations");
  });
});

test("hydrates messages for initial and selected conversation", async () => {
  invokeMock.mockImplementation(async (command: string, args?: { conversation_id?: string }) => {
    if (command === "list_conversations") {
      return [
        {
          id: "conv-1",
          title: "Chat 1",
          provider_id: "minimax",
          user_id: null,
          created_at: 1,
          updated_at: 1,
        },
        {
          id: "conv-2",
          title: "Chat 2",
          provider_id: "minimax",
          user_id: null,
          created_at: 2,
          updated_at: 2,
        },
      ];
    }
    if (command === "list_messages") {
      if (args?.conversation_id === "conv-1") {
        return [
          {
            id: "m-1",
            conversation_id: "conv-1",
            role: "assistant",
            content: "Hello from chat 1",
            created_at: 1,
          },
        ];
      }
      if (args?.conversation_id === "conv-2") {
        return [
          {
            id: "m-2",
            conversation_id: "conv-2",
            role: "assistant",
            content: "Welcome to chat 2",
            created_at: 2,
          },
        ];
      }
      return [];
    }
    return "";
  });

  render(<App />);

  await waitFor(() => {
    expect(invokeMock).toHaveBeenCalledWith("list_messages", { conversation_id: "conv-1" });
    expect(screen.getByText("Hello from chat 1")).toBeInTheDocument();
  });

  fireEvent.click(screen.getByRole("button", { name: "Chat 2" }));

  await waitFor(() => {
    expect(invokeMock).toHaveBeenCalledWith("list_messages", { conversation_id: "conv-2" });
    expect(screen.getByText("Welcome to chat 2")).toBeInTheDocument();
  });
});

test("sends a message and streams assistant response", async () => {
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "list_conversations") {
      return [
        {
          id: "conv-1",
          title: "Chat 1",
          provider_id: "minimax",
          user_id: null,
          created_at: 1,
          updated_at: 1,
        },
      ];
    }
    if (command === "list_messages") {
      return [];
    }
    if (command === "send_message") {
      return "assistant-1";
    }
    return "";
  });
  render(<App />);
  await waitFor(() => {
    expect(listeners.has("chat-delta")).toBe(true);
    expect(listeners.has("chat-done")).toBe(true);
    expect(listeners.has("chat-error")).toBe(true);
    expect(listeners.has("pending-approval")).toBe(true);
    expect(listeners.has("approval-resolved")).toBe(true);
  });

  fireEvent.change(screen.getByPlaceholderText("Type a message..."), {
    target: { value: "hello" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Send" }));

  await waitFor(() => {
    expect(invokeMock).toHaveBeenCalledWith("send_message", {
      conversation_id: "conv-1",
      content: "hello",
    });
  });

  emit("chat-delta", {
    conversation_id: "other-conversation",
    message_id: "other-message",
    delta: "ignored",
  });
  expect(screen.queryByText("ignored")).not.toBeInTheDocument();

  emit("chat-delta", {
    conversation_id: "conv-1",
    message_id: "assistant-1",
    delta: "accepted",
  });
  await waitFor(() => {
    expect(screen.getByText("accepted")).toBeInTheDocument();
    expect(screen.getByText("Streaming...")).toBeInTheDocument();
  });

  emit("chat-done", {
    conversation_id: "conv-1",
    message_id: "assistant-1",
  });
  await waitFor(() => {
    expect(screen.queryByText("Streaming...")).not.toBeInTheDocument();
  });
});

test("handles chat-error by stopping stream and showing message", async () => {
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "list_conversations") {
      return [
        {
          id: "conv-2",
          title: "Chat 2",
          provider_id: "minimax",
          user_id: null,
          created_at: 1,
          updated_at: 1,
        },
      ];
    }
    if (command === "list_messages") {
      return [];
    }
    if (command === "send_message") {
      return "assistant-2";
    }
    return "";
  });
  render(<App />);
  await waitFor(() => {
    expect(listeners.has("chat-delta")).toBe(true);
    expect(listeners.has("chat-done")).toBe(true);
    expect(listeners.has("chat-error")).toBe(true);
    expect(listeners.has("pending-approval")).toBe(true);
    expect(listeners.has("approval-resolved")).toBe(true);
  });

  fireEvent.change(screen.getByPlaceholderText("Type a message..."), {
    target: { value: "hello" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Send" }));

  await waitFor(() => {
    expect(invokeMock).toHaveBeenCalledWith("send_message", {
      conversation_id: "conv-2",
      content: "hello",
    });
  });

  emit("chat-delta", {
    conversation_id: "conv-2",
    message_id: "assistant-2",
    delta: "partial",
  });
  await waitFor(() => {
    expect(screen.getByText("Streaming...")).toBeInTheDocument();
  });

  emit("chat-error", {
    conversation_id: "conv-2",
    message_id: "assistant-2",
    message: "model failed",
  });

  await waitFor(() => {
    expect(screen.queryByText("Streaming...")).not.toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("Error: model failed");
  });
});

test("disables submit and blocks send while streaming", async () => {
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "list_conversations") {
      return [
        {
          id: "conv-stream",
          title: "Chat Stream",
          provider_id: "minimax",
          user_id: null,
          created_at: 1,
          updated_at: 1,
        },
      ];
    }
    if (command === "list_messages") {
      return [];
    }
    if (command === "send_message") {
      return "assistant-stream";
    }
    return "";
  });

  useConversationStore.setState({
    currentConversationId: "conv-stream",
    activeConversationId: "conv-stream",
    activeMessageId: "assistant-stream",
    isStreaming: true,
  });

  render(<App />);
  const sendButton = screen.getByRole("button", { name: "Send" });
  expect(sendButton).toBeDisabled();

  fireEvent.change(screen.getByPlaceholderText("Type a message..."), {
    target: { value: "should not send" },
  });
  fireEvent.submit(sendButton.closest("form") as HTMLFormElement);

  await waitFor(() => {
    expect(invokeMock).toHaveBeenCalledWith("list_conversations");
  });
  expect(invokeMock).not.toHaveBeenCalledWith("send_message", expect.anything());
});

test("renders pending approval card and calls approve command", async () => {
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "list_conversations") {
      return [
        {
          id: "conv-3",
          title: "Chat 3",
          provider_id: "minimax",
          user_id: null,
          created_at: 1,
          updated_at: 1,
        },
      ];
    }
    if (command === "list_messages") {
      return [];
    }
    if (command === "send_message") {
      return "assistant-3";
    }
    if (command === "approve_action") {
      return null;
    }
    return "";
  });
  render(<App />);
  await waitFor(() => {
    expect(listeners.has("pending-approval")).toBe(true);
    expect(listeners.has("approval-resolved")).toBe(true);
  });

  fireEvent.change(screen.getByPlaceholderText("Type a message..."), {
    target: { value: "make files" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Send" }));

  await waitFor(() => {
    expect(invokeMock).toHaveBeenCalledWith("send_message", {
      conversation_id: "conv-3",
      content: "make files",
    });
  });

  emit("pending-approval", {
    conversation_id: "conv-3",
    message_id: "assistant-3",
    approval_id: "approval-1",
    action_type: "write_file",
    payload: {
      path: "notes/todo.txt",
      content: "first line\nsecond line",
    },
  });

  await waitFor(() => {
    expect(screen.getByTestId("approval-card-approval-1")).toBeInTheDocument();
    expect(screen.getByText("Path: notes/todo.txt")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Accept" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reject" })).toBeInTheDocument();
  });

  fireEvent.click(screen.getByRole("button", { name: "Accept" }));
  await waitFor(() => {
    expect(invokeMock).toHaveBeenCalledWith("approve_action", {
      approval_id: "approval-1",
    });
  });

  emit("approval-resolved", {
    conversation_id: "conv-3",
    message_id: "assistant-3",
    approval_id: "approval-1",
    status: "approved",
  });

  await waitFor(() => {
    expect(screen.queryByTestId("approval-card-approval-1")).not.toBeInTheDocument();
  });
});
