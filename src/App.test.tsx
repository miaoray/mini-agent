import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import App from "./App";

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

test("renders app heading", () => {
  invokeMock.mockResolvedValue("unused");
  render(<App />);
  expect(screen.getByRole("main")).toBeInTheDocument();
  expect(screen.getByRole("heading", { level: 1 })).toBeInTheDocument();
});

test("filters stream events by active turn ids", async () => {
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "create_conversation") {
      return "conv-1";
    }
    if (command === "send_message") {
      return "msg-1";
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

  fireEvent.change(screen.getByPlaceholderText("Enter a name..."), {
    target: { value: "hello" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Greet" }));

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
  expect(screen.getByTestId("streamed-text")).toHaveTextContent("");

  emit("chat-delta", {
    conversation_id: "conv-1",
    message_id: "msg-1",
    delta: "accepted",
  });
  await waitFor(() => {
    expect(screen.getByTestId("streamed-text")).toHaveTextContent("accepted");
    expect(screen.getByText("Streaming...")).toBeInTheDocument();
  });

  emit("chat-done", {
    conversation_id: "conv-1",
    message_id: "msg-1",
  });
  await waitFor(() => {
    expect(screen.queryByText("Streaming...")).not.toBeInTheDocument();
  });
});

test("handles chat-error by stopping stream and showing message", async () => {
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "create_conversation") {
      return "conv-2";
    }
    if (command === "send_message") {
      return "msg-2";
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

  fireEvent.change(screen.getByPlaceholderText("Enter a name..."), {
    target: { value: "hello" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Greet" }));

  await waitFor(() => {
    expect(invokeMock).toHaveBeenCalledWith("send_message", {
      conversation_id: "conv-2",
      content: "hello",
    });
  });

  emit("chat-delta", {
    conversation_id: "conv-2",
    message_id: "msg-2",
    delta: "partial",
  });
  await waitFor(() => {
    expect(screen.getByText("Streaming...")).toBeInTheDocument();
  });

  emit("chat-error", {
    conversation_id: "conv-2",
    message_id: "msg-2",
    message: "model failed",
  });

  await waitFor(() => {
    expect(screen.queryByText("Streaming...")).not.toBeInTheDocument();
    expect(screen.getByTestId("streamed-text")).toHaveTextContent("Error: model failed");
  });
});

test("renders pending approval card and calls approve command", async () => {
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "create_conversation") {
      return "conv-3";
    }
    if (command === "send_message") {
      return "msg-3";
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

  fireEvent.change(screen.getByPlaceholderText("Enter a name..."), {
    target: { value: "make files" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Greet" }));

  await waitFor(() => {
    expect(invokeMock).toHaveBeenCalledWith("send_message", {
      conversation_id: "conv-3",
      content: "make files",
    });
  });

  emit("pending-approval", {
    conversation_id: "conv-3",
    message_id: "msg-3",
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
    message_id: "msg-3",
    approval_id: "approval-1",
    status: "approved",
  });

  await waitFor(() => {
    expect(screen.queryByTestId("approval-card-approval-1")).not.toBeInTheDocument();
  });
});
