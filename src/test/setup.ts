import { vi } from "vitest";

/**
 * Global mock for @tauri-apps/api/core so components (e.g. DebugPanel) that use invoke
 * don't throw when Tauri runtime is unavailable in jsdom.
 * Individual test files (e.g. App.test.tsx) may override this with their own mock.
 */
vi.mock("@tauri-apps/api/core", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tauri-apps/api/core")>();
  const invokeMock = vi.fn().mockImplementation(async (cmd: string) => {
    if (cmd === "get_debug_mode") return false;
    return undefined;
  });
  return {
    ...actual,
    invoke: invokeMock,
  };
});
