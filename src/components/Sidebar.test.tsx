import { render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { describe, expect, test, vi } from "vitest";
import Sidebar from "./Sidebar";

describe("Sidebar", () => {
  test("renders New Chat button", () => {
    render(
      <Sidebar
        conversations={[]}
        currentConversationId={null}
        onSelectConversation={vi.fn()}
        onNewChat={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: "New chat" })).toBeInTheDocument();
  });

  test("renders conversation list entries", () => {
    render(
      <Sidebar
        conversations={[
          {
            id: "conv-1",
            title: "First chat",
            provider_id: "minimax",
            user_id: null,
            created_at: 1,
            updated_at: 1,
          },
          {
            id: "conv-2",
            title: "Second chat",
            provider_id: "minimax",
            user_id: null,
            created_at: 2,
            updated_at: 2,
          },
        ]}
        currentConversationId="conv-1"
        onSelectConversation={vi.fn()}
        onNewChat={vi.fn()}
      />,
    );

    expect(screen.getByText("First chat")).toBeInTheDocument();
    expect(screen.getByText("Second chat")).toBeInTheDocument();
  });
});
