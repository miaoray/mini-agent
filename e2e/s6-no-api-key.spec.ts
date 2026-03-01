import { expect, test } from "@playwright/test";
import { installMockTauri } from "./helpers/mockTauri";

test("shows missing API key banner when backend config is unavailable", async ({ page }) => {
  await installMockTauri(page, { hasApiKey: false });
  await page.goto("/");

  await expect(page.getByRole("alert")).toContainText("Missing `MINIMAX_API_KEY`");
});
