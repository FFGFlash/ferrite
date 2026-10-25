import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ErrorBoundary } from "./ErrorBoundary";

let broken = true;

function Fragile() {
  if (broken) {
    throw new Error("measure failed");
  }
  return <p>working</p>;
}

describe("ErrorBoundary", () => {
  afterEach(() => {
    broken = true;
    vi.restoreAllMocks();
  });

  it("contains a rendering error and recovers on Try Again", async () => {
    // React reports caught errors to the console.
    vi.spyOn(console, "error").mockImplementation(() => {});
    const user = userEvent.setup();
    render(
      <div>
        <ErrorBoundary name="The editor">
          <Fragile />
        </ErrorBoundary>
        <p>still here</p>
      </div>,
    );
    expect(screen.getByRole("alert")).toHaveTextContent(
      "The editor stopped working because of an internal error.",
    );
    expect(screen.getByText("measure failed")).toBeInTheDocument();
    expect(screen.getByText("still here")).toBeInTheDocument();

    broken = false;
    await user.click(screen.getByRole("button", { name: "Try Again" }));
    expect(screen.getByText("working")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("clears the error when its reset key changes", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const { rerender } = render(
      <ErrorBoundary name="The editor" resetKey={1}>
        <Fragile />
      </ErrorBoundary>,
    );
    expect(screen.getByRole("alert")).toBeInTheDocument();
    broken = false;
    rerender(
      <ErrorBoundary name="The editor" resetKey={2}>
        <Fragile />
      </ErrorBoundary>,
    );
    expect(screen.getByText("working")).toBeInTheDocument();
  });
});
