import { expect, test } from "@playwright/test";
import { installMockTauri } from "./helpers/mockTauri";

const CUSTOM_RESPONSE_KEY = "你好";

const MINIMAX_RESPONSE_WITH_THINKING = {
  id: "05f3b07fd1756473879f9ac51c6a1c4f",
  type: "message",
  role: "assistant",
  model: "MiniMax-M2.5",
  content: [
    {
      thinking: "用户用中文说\"你好\"，这是一个简单的问候。我应该用中文回复，友好地回应并询问有什么可以帮助的。",
      signature: "be0a30e6d6021ceba39b96d5dea1616eba5697d1cdf6fa4cc6111a9819decf45",
      type: "thinking",
    },
    {
      text: "你好！很高兴见到你。有什么我可以帮助你的吗？无论是编程问题，信息查询、文件操作还是其他任务，我都很乐意为你提供帮助。",
      type: "text",
    },
  ],
  usage: {
    input_tokens: 445,
    output_tokens: 59,
  },
  stop_reason: "end_turn",
  base_resp: {
    status_code: 0,
    status_msg: "",
  },
};

const mockState = {
  conversations: [],
  messagesByConversation: {},
  pendingApprovals: [],
  customResponses: {
    [CUSTOM_RESPONSE_KEY]: {
      request: CUSTOM_RESPONSE_KEY,
      response: MINIMAX_RESPONSE_WITH_THINKING,
    },
  },
};

/**
 * Test case: Simulate MiniMax response with thinking and text content blocks
 * This reproduces the exact response format from the real API
 */
test("MiniMax response with thinking and text content blocks", async ({ page }) => {
  await page.addInitScript((state) => {
    window.localStorage.setItem("__mini_agent_e2e_mock_state__", JSON.stringify(state));
  }, mockState);

  await installMockTauri(page, { hasApiKey: true });
  await page.goto("/");

  await page.getByPlaceholder("Send a message...").fill(CUSTOM_RESPONSE_KEY);
  await page.getByRole("button", { name: "Send" }).click();

  // Wait for response
  await page.waitForTimeout(500);

  // Check if thinking is displayed
  const thinkingText = "用户用中文说";
  const hasThinking = await page.getByText(thinkingText).count();
  console.log(`[test] Thinking text "${thinkingText}" count: ${hasThinking}`);

  // Check if the final response text is displayed
  const responseText = "你好！很高兴见到你";
  const hasResponse = await page.getByText(responseText).count();
  console.log(`[test] Response text "${responseText}" count: ${hasResponse}`);

  // This is the actual assertion - the text content should be visible
  await expect(page.getByText(responseText)).toBeVisible();
});
