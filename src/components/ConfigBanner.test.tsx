import { render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { expect, test } from "vitest";
import ConfigBanner from "./ConfigBanner";

test("shows warning when API key is missing", () => {
  render(<ConfigBanner hasApiKey={false} />);

  expect(screen.getByRole("alert")).toHaveTextContent(/MINIMAX_API_KEY/);
  expect(screen.getByText("Get API Key")).toBeInTheDocument();
});

test("does not render warning when API key exists", () => {
  const { container } = render(<ConfigBanner hasApiKey />);

  expect(container).toBeEmptyDOMElement();
});
