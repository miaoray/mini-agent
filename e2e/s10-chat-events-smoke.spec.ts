import { expect, test } from "@playwright/test";
import { installMockTauri } from "./helpers/mockTauri";

/**
 * Smoke test for chat events flow
 * Tests that chat-done event is properly handled
 */
test("chat events flow - delta and done events", async ({ page }) => {
  await installMockTauri(page, { hasApiKey: true });
  await page.goto("/");

  await page.getByPlaceholder("Send a message...").fill("hello");
  await page.getByRole("button", { name: "Send" }).click();

  await expect(page.getByText("Mock assistant reply: hello")).toBeVisible();
});

/**
 * Test debug panel is hidden by default
 */
test("debug panel is hidden by default", async ({ page }) => {
  await installMockTauri(page, { hasApiKey: true });
  await page.goto("/");

  // Debug panel is hidden by default, only shown when VITE_SHOW_DEBUG=true
  const debugToggle = page.getByLabel("Debug Mode");
  await expect(debugToggle).not.toBeVisible();
});

/**
 * Test that multiple messages work correctly
 */
test("multiple messages in sequence", async ({ page }) => {
  await installMockTauri(page, { hasApiKey: true });
  await page.goto("/");

  await page.getByPlaceholder("Send a message...").fill("first message");
  await page.getByRole("button", { name: "Send" }).click();
  await expect(page.getByText("Mock assistant reply: first message")).toBeVisible();

  await page.getByPlaceholder("Send a message...").fill("second message");
  await page.getByRole("button", { name: "Send" }).click();
  await expect(page.getByText("Mock assistant reply: second message")).toBeVisible();
});
