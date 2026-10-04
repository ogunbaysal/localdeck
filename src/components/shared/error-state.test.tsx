import { describe, expect, mock, test } from "bun:test";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "@/test/render";
import { ErrorState } from "./error-state";

describe("ErrorState", () => {
  test("renders a code-specific title, the backend message, a hint and the context", () => {
    renderWithProviders(
      <ErrorState
        error={{
          code: "PROCESS_PERMISSION_DENIED",
          message: "Not permitted to signal process 321",
          context: { pid: "321" },
        }}
      />,
    );
    const alert = screen.getByRole("alert");
    expect(alert).toHaveAttribute("data-code", "PROCESS_PERMISSION_DENIED");
    expect(screen.getByRole("heading", { name: "Permission denied" })).toBeInTheDocument();
    expect(screen.getByText("Not permitted to signal process 321")).toBeInTheDocument();
    expect(screen.getByText(/only signal processes owned by your user account/)).toBeInTheDocument();
    expect(screen.getByText("pid")).toBeInTheDocument();
    expect(screen.getByText("321")).toBeInTheDocument();
  });

  test("unreachable-backend failures say so and don't print the internal origin tag", () => {
    renderWithProviders(<ErrorState error="Command get_snapshot not found" />);
    expect(screen.getByRole("heading", { name: "Couldn't reach the LocalDeck backend" })).toBeInTheDocument();
    expect(screen.getByText("Command get_snapshot not found")).toBeInTheDocument();
    expect(screen.queryByText("origin")).not.toBeInTheDocument();
  });

  test("offers a retry only when a handler is given, and calls it", async () => {
    const onRetry = mock(() => {});
    const { user, rerender } = renderWithProviders(
      <ErrorState error={{ code: "DATABASE_ERROR", message: "Database error: locked" }} onRetry={onRetry} retryLabel="Retry" />,
    );
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(onRetry).toHaveBeenCalledTimes(1);

    rerender(<ErrorState error={{ code: "DATABASE_ERROR", message: "Database error: locked" }} />);
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
  });
});
