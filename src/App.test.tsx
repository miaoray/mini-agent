import { render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { expect, test } from "vitest";
import App from "./App";

test("renders app heading", () => {
  render(<App />);
  expect(screen.getByText("Welcome to Tauri + React")).toBeInTheDocument();
});
