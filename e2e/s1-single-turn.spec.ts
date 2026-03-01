import { expect, test } from "@playwright/test";
import { installMockTauri } from "./helpers/mockTauri";

test("single-turn chat flow", async ({ page }) => {
  await installMockTauri(page, { hasApiKey: true });
  await page.goto("/");

  await page.getByPlaceholder("Type a message...").fill("hello mini-agent");
  await page.getByRole("button", { name: "Send" }).click();

  await expect(page.getByText("Mock assistant reply: hello mini-agent")).toBeVisible();
});
