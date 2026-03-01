import { render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { expect, test } from "vitest";
import ConfigBanner from "./ConfigBanner";

test("shows warning when API key is missing", () => {
  render(<ConfigBanner hasApiKey={false} />);

  expect(screen.getByRole("alert")).toHaveTextContent("Missing `MINIMAX_API_KEY`.");
  expect(screen.getByText(/\.env\.example/)).toBeInTheDocument();
});

test("does not render warning when API key exists", () => {
  const { container } = render(<ConfigBanner hasApiKey />);

  expect(container).toBeEmptyDOMElement();
});
