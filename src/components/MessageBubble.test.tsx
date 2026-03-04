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

});
