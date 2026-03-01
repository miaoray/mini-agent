import { expect, test } from "@playwright/test";
import { installMockTauri } from "./helpers/mockTauri";

test("approval flow: accept resolves pending action", async ({ page }) => {
  await installMockTauri(page, { hasApiKey: true });
  await page.goto("/");

  await page.getByPlaceholder("Type a message...").fill("please write file for me");
  await page.getByRole("button", { name: "Send" }).click();

  await expect(page.getByText("Pending approval: write_file")).toBeVisible();
  await expect(page.getByText("Path: notes/todo.txt")).toBeVisible();

  await page.getByRole("button", { name: "Accept" }).click();

  await expect(page.getByText("Pending approval: write_file")).not.toBeVisible();
  await expect(page.getByText("Approved and completed: write_file notes/todo.txt")).toBeVisible();
});

test("approval flow: reject resolves pending action", async ({ page }) => {
  await installMockTauri(page, { hasApiKey: true });
  await page.goto("/");

  await page.getByPlaceholder("Type a message...").fill("please write file then reject");
  await page.getByRole("button", { name: "Send" }).click();

  await expect(page.getByText("Pending approval: write_file")).toBeVisible();
  await expect(page.getByText("Path: notes/todo.txt")).toBeVisible();

  await page.getByRole("button", { name: "Reject" }).click();

  await expect(page.getByText("Pending approval: write_file")).not.toBeVisible();
  await expect(page.getByText("User rejected action: write_file notes/todo.txt")).toBeVisible();
});
