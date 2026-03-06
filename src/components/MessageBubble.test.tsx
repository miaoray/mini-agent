import { render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { describe, expect, test } from "vitest";
import MessageBubble from "./MessageBubble";

describe("MessageBubble", () => {
  test("renders user message content", () => {
    render(<MessageBubble role="user" content="Hello from user" />);
    expect(screen.getByText("Hello from user")).toBeInTheDocument();
  });

  test("renders assistant message content", () => {
    render(<MessageBubble role="assistant" content="Hello from assistant" />);
    expect(screen.getByText("Hello from assistant")).toBeInTheDocument();
  });

  test("renders assistant final content as markdown", () => {
    render(
      <MessageBubble
        role="assistant"
        content="**Bold** and *italic*"
        variant="final"
      />
    );
    expect(screen.getByText("Bold")).toBeInTheDocument();
    expect(screen.getByText("italic")).toBeInTheDocument();
  });

  test("renders markdown tables", () => {
    const tableMarkdown = `| A | B |
| --- | --- |
| 1 | 2 |`;
    render(
      <MessageBubble role="assistant" content={tableMarkdown} variant="final" />
    );
    expect(screen.getByText("A")).toBeInTheDocument();
    expect(screen.getByText("B")).toBeInTheDocument();
    expect(screen.getByText("1")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
  });

  test("renders tool-process as plain text", () => {
    render(
      <MessageBubble
        role="assistant"
        content="**not bold**"
        variant="tool-process"
      />
    );
    expect(screen.getByText("**not bold**")).toBeInTheDocument();
  });
});
