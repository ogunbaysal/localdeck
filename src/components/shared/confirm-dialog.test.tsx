import { describe, expect, mock, test } from "bun:test";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "@/test/render";
import { ConfirmDialog } from "./confirm-dialog";

function setup(open = true) {
  const onConfirm = mock(() => {});
  const onOpenChange = mock((_open: boolean) => {});
  const view = renderWithProviders(
    <ConfirmDialog
      open={open}
      onOpenChange={onOpenChange}
      title="Force stop LocalDeck?"
      description="This sends SIGKILL."
      confirmLabel="Force Stop"
      destructive
      onConfirm={onConfirm}
    />,
  );
  return { ...view, onConfirm, onOpenChange };
}

describe("ConfirmDialog", () => {
  test("nothing is rendered and nothing runs while closed", () => {
    const { onConfirm } = setup(false);
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(onConfirm).not.toHaveBeenCalled();
  });

  test("shows the question and runs onConfirm only on the explicit confirm click", async () => {
    const { user, onConfirm } = setup();
    expect(screen.getByRole("alertdialog", { name: "Force stop LocalDeck?" })).toBeInTheDocument();
    expect(screen.getByText("This sends SIGKILL.")).toBeInTheDocument();
    expect(onConfirm).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Force Stop" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  test("Cancel and Escape dismiss without confirming", async () => {
    const { user, onConfirm, onOpenChange } = setup();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onOpenChange).toHaveBeenLastCalledWith(false);
    await user.keyboard("{Escape}");
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
