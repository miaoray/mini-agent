import { expect, test } from "@playwright/test";
import { installMockTauri } from "./helpers/mockTauri";

// Mock response that simulates LLM calling get_time tool
const TIME_TOOL_RESPONSE = {
  id: "time-tool-test-123",
  type: "message",
  role: "assistant",
  model: "MiniMax-M2.5",
  content: [
    {
      thinking: "The user is asking about the current time. I should call the get_time tool to get accurate information.",
      signature: "test-signature",
      type: "thinking",
    },
    {
      text: "Let me check the current time for you.",
      type: "text",
    },
    {
      type: "tool_use",
      id: "toolu-time-123",
      name: "get_time",
      input: {},
    },
  ],
  usage: {
    input_tokens: 100,
    output_tokens: 50,
  },
  stop_reason: "tool_use",
  base_resp: {
    status_code: 0,
    status_msg: "",
  },
};

const DATE_TOOL_RESPONSE = {
  id: "date-tool-test-456",
  type: "message",
  role: "assistant",
  model: "MiniMax-M2.5",
  content: [
    {
      thinking: "The user wants to know today's date. Let me use the get_time tool.",
      signature: "test-signature-2",
      type: "thinking",
    },
    {
      type: "tool_use",
      id: "toolu-date-456",
      name: "get_time",
      input: {},
    },
  ],
  usage: {
    input_tokens: 80,
    output_tokens: 30,
  },
  stop_reason: "tool_use",
  base_resp: {
    status_code: 0,
    status_msg: "",
  },
};

test("LLM can use get_time tool to answer time-related questions", async ({ page }) => {
  const mockState = {
    conversations: [],
    messagesByConversation: {},
    pendingApprovals: [],
    customResponses: {
      "time": {
        request: "time",
        response: TIME_TOOL_RESPONSE,
      },
    },
  };

  await page.addInitScript((state) => {
    window.localStorage.setItem("__mini_agent_e2e_mock_state__", JSON.stringify(state));
  }, mockState);

  await installMockTauri(page, { hasApiKey: true });
  await page.goto("/");

  // User asks a time-related question
  await page.getByPlaceholder("Send a message...").fill("What time is it now?");
  await page.getByRole("button", { name: "Send" }).click();

  // Should see thinking panel first (from the thinking block in mock response)
  await expect(page.getByText(/calling tool|get_time/i)).toBeVisible({ timeout: 5000 });

  // The assistant should respond with tool invocation info
  await expect(page.getByText(/Called tool get_time/i)).toBeVisible({ timeout: 5000 });
});

test("LLM handles date-related questions with get_time tool", async ({ page }) => {
  const mockState = {
    conversations: [],
    messagesByConversation: {},
    pendingApprovals: [],
    customResponses: {
      "date": {
        request: "date",
        response: DATE_TOOL_RESPONSE,
      },
    },
  };

  await page.addInitScript((state) => {
    window.localStorage.setItem("__mini_agent_e2e_mock_state__", JSON.stringify(state));
  }, mockState);

  await installMockTauri(page, { hasApiKey: true });
  await page.goto("/");

  // User asks a date-related question
  await page.getByPlaceholder("Send a message...").fill("What's today's date?");
  await page.getByRole("button", { name: "Send" }).click();

  // Verify tool invocation result is displayed (look for the specific "Called tool" message)
  await expect(page.getByText(/Called tool get_time/i)).toBeVisible({ timeout: 5000 });
});
