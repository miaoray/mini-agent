import { expect, test } from "@playwright/test";
import { installMockTauri } from "./helpers/mockTauri";

test("conversation persistence after reload", async ({ page }) => {
  await installMockTauri(page, { hasApiKey: true });
  await page.goto("/");

  await page.getByPlaceholder("Send a message...").fill("persist this message");
  await page.getByRole("button", { name: "Send" }).click();
  await expect(page.getByText("Mock assistant reply: persist this message")).toBeVisible();

  await page.reload();

  await expect(page.getByRole("button", { name: "New chat" })).toBeVisible();
  await expect(page.getByText("Mock assistant reply: persist this message")).toBeVisible();
});
