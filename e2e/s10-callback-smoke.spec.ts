import { expect, test } from "@playwright/test";
import { installMockTauri } from "./helpers/mockTauri";

test("callback id not found smoke test", async ({ page }) => {
  const consoleLogs: string[] = [];
  const consoleErrors: string[] = [];

  page.on("console", (msg) => {
    const text = msg.text();
    if (msg.type() === "error") {
      consoleErrors.push(text);
    } else {
      consoleLogs.push(text);
    }
  });

  await installMockTauri(page, { hasApiKey: true });
  await page.goto("/");

  // Send a simple message
  await page.getByPlaceholder("Type a message...").fill("你好");
  await page.getByRole("button", { name: "Send" }).click();

  // Wait for chat to complete
  await page.waitForTimeout(500);

  // Check for callback errors
  const callbackErrors = consoleErrors.filter(
    (log) => log.includes("callback") || log.includes("Could not find")
  );

  console.log("Console logs:", consoleLogs);
  console.log("Console errors:", consoleErrors);

  // Should not have callback errors
  expect(callbackErrors).toHaveLength(0);
});
