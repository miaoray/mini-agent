import { expect, test } from "@playwright/test";
import { installMockTauri } from "./helpers/mockTauri";

test("auto-generate conversation title from first message", async ({ page }) => {
  await installMockTauri(page, { hasApiKey: true });
  await page.goto("/");

  // Create a new conversation
  await page.getByRole("button", { name: "New chat" }).click();

  // Verify the initial title is "New Chat"
  await expect(page.locator(".conversation-item-title").getByText("New Chat")).toBeVisible();

  // Send the first message with a question
  const testMessage = "What is the weather like today? I want to go outside.";
  await page.getByPlaceholder("Send a message...").fill(testMessage);
  await page.getByRole("button", { name: "Send" }).click();

  // Wait for the assistant reply
  await expect(page.getByText("Mock assistant reply:")).toBeVisible();

  // Verify the title has been updated to the first sentence
  await expect(page.locator(".conversation-item-title").getByText("What is the weather like today?")).toBeVisible();
  
  // Verify "New Chat" is no longer visible in the sidebar
  await expect(page.locator(".conversation-item-title").getByText("New Chat")).not.toBeVisible();
});

test("auto-generate title with short message", async ({ page }) => {
  await installMockTauri(page, { hasApiKey: true });
  await page.goto("/");

  // Create a new conversation
  await page.getByRole("button", { name: "New chat" }).click();

  // Send a short message without punctuation
  const testMessage = "Hello world";
  await page.getByPlaceholder("Send a message...").fill(testMessage);
  await page.getByRole("button", { name: "Send" }).click();

  // Wait for the assistant reply
  await expect(page.getByText("Mock assistant reply:")).toBeVisible();

  // Verify the title is the full message (since it's short)
  await expect(page.locator(".conversation-item-title").getByText("Hello world")).toBeVisible();
});

test("auto-generate title with long message", async ({ page }) => {
  await installMockTauri(page, { hasApiKey: true });
  await page.goto("/");

  // Create a new conversation
  await page.getByRole("button", { name: "New chat" }).click();

  // Send a long message without punctuation
  const testMessage = "This is a very long message that should be truncated to a reasonable length for the conversation title";
  await page.getByPlaceholder("Send a message...").fill(testMessage);
  await page.getByRole("button", { name: "Send" }).click();

  // Wait for the assistant reply
  await expect(page.getByText("Mock assistant reply:")).toBeVisible();

  // Verify the title is truncated with "..."
  await expect(page.locator(".conversation-item-title").getByText("This is a very long...")).toBeVisible();
});

test("second message does not update title", async ({ page }) => {
  await installMockTauri(page, { hasApiKey: true });
  await page.goto("/");

  // Create a new conversation
  await page.getByRole("button", { name: "New chat" }).click();

  // Send the first message
  const firstMessage = "First message.";
  await page.getByPlaceholder("Send a message...").fill(firstMessage);
  await page.getByRole("button", { name: "Send" }).click();

  // Wait for the assistant reply
  await expect(page.getByText("Mock assistant reply:")).toBeVisible();

  // Verify the title is set to the first message
  await expect(page.locator(".conversation-item-title").getByText("First message.")).toBeVisible();

  // Send a second message
  const secondMessage = "This is a second message with different content.";
  await page.getByPlaceholder("Send a message...").fill(secondMessage);
  await page.getByRole("button", { name: "Send" }).click();

  // Wait for the assistant reply
  await expect(page.getByText("Mock assistant reply: " + secondMessage)).toBeVisible();

  // Verify the title is still the first message
  await expect(page.locator(".conversation-item-title").getByText("First message.")).toBeVisible();
  
  // Verify the second message is not the title
  await expect(page.locator(".conversation-item-title").getByText("This is a second message")).not.toBeVisible();
});
