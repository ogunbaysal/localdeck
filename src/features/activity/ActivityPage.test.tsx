import { afterEach, describe, expect, test } from "bun:test";
import { act, screen, waitFor, within } from "@testing-library/react";
import { toast } from "sonner";
import { Route, Routes } from "react-router";
import { useRuntimeStore } from "@/stores/runtime-store";
import { backend } from "@/test/backend";
import { makeActivity, makeProject, makeSnapshot } from "@/test/fixtures";
import { renderWithProviders } from "@/test/render";
import type { ActivityEvent } from "@/types";
import { ActivityPage } from "./ActivityPage";

afterEach(() => {
  toast.dismiss();
});

const day = (daysAgo: number, hour: number, minute = 0) => {
  const now = new Date();
  return new Date(now.getFullYear(), now.getMonth(), now.getDate() - daysAgo, hour, minute).getTime();
};

const alpha = makeProject({ id: 1, name: "Alpha" });
const beta = makeProject({ id: 2, name: "Beta" });
const snapshot = makeSnapshot({ projects: [alpha, beta], updatedAt: 1000 });

const EVENTS: ActivityEvent[] = [
  makeActivity({ id: 5, projectId: 1, kind: "managedExited", message: "Alpha api exited with code 1", createdAt: day(0, 9, 41) }),
  makeActivity({ id: 4, projectId: 2, kind: "portOpened", message: "Beta started listening on :4000", createdAt: day(0, 9, 30) }),
  makeActivity({ id: 3, projectId: 1, kind: "projectStarted", message: "Alpha started", createdAt: day(1, 18, 4) }),
  makeActivity({ id: 2, projectId: 99, kind: "projectStopped", message: "Ghost stopped", createdAt: day(1, 17, 0) }),
  makeActivity({ id: 1, projectId: 2, kind: "projectStarted", message: "Beta started", createdAt: day(5, 8, 0) }),
];

function setup(events: ActivityEvent[] = EVENTS) {
  backend.handle("list_activity", () => events);
  backend.reply("clear_activity", undefined);
  return renderWithProviders(
    <Routes>
      <Route path="/" element={<ActivityPage />} />
      <Route path="/projects/:projectId" element={<h1>Project page</h1>} />
    </Routes>,
    { snapshot, toaster: true },
  );
}

const rows = () => [...document.querySelectorAll("[data-slot=activity-row]")].map((row) => row.textContent ?? "");

describe("timeline", () => {
  test("lists events newest first, grouped by day, with time, kind, project link and message", async () => {
    setup();
    expect(await screen.findByText("Alpha api exited with code 1")).toBeInTheDocument();

    expect(backend.callsTo("list_activity")).toEqual([{ projectId: null, limit: null }]);
    const today = screen.getByRole("region", { name: "Today" });
    const yesterday = screen.getByRole("region", { name: "Yesterday" });
    expect(within(today).getAllByRole("listitem")).toHaveLength(2);
    expect(within(yesterday).getAllByRole("listitem")).toHaveLength(2);
    expect(screen.getAllByRole("heading", { level: 2 }).map((heading) => heading.textContent)).toEqual([
      "Today",
      "Yesterday",
      expect.stringMatching(/^[A-Z][a-z]{2} \d{1,2}(, \d{4})?$/),
    ]);

    const first = within(today).getAllByRole("listitem")[0];
    expect(first).toHaveTextContent("09:41");
    expect(first).toHaveTextContent("Crashed");
    expect(first).toHaveTextContent("Alpha api exited with code 1");
    expect(within(first).getByRole("link", { name: "Alpha" })).toHaveAttribute("href", "/projects/1");
  });

  test("project names link to the project; events of forgotten projects are muted and unlinked", async () => {
    const { user } = setup();
    const ghost = (await screen.findByText("Ghost stopped")).closest("li");
    expect(ghost).not.toBeNull();
    if (!ghost) return;
    expect(within(ghost).getByText("Unknown project")).toBeInTheDocument();
    expect(within(ghost).queryByRole("link")).not.toBeInTheDocument();

    await user.click(within(screen.getByRole("region", { name: "Today" })).getByRole("link", { name: "Alpha" }));
    expect(await screen.findByRole("heading", { name: "Project page" })).toBeInTheDocument();
  });

  test("shows a count and the empty state when nothing has been recorded", async () => {
    setup([]);
    expect(await screen.findByText("No activity yet")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Clear history" })).toBeDisabled();
  });

  test("a failing first load is a specific error with a working retry", async () => {
    backend.fail("list_activity", { code: "DATABASE_ERROR", message: "database is locked" });
    const { user } = renderWithProviders(<ActivityPage />, { snapshot });
    expect(await screen.findByText("Local storage error")).toBeInTheDocument();
    expect(screen.getByText("database is locked")).toBeInTheDocument();

    backend.handle("list_activity", () => EVENTS);
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Alpha started")).toBeInTheDocument();
    expect(screen.queryByText("Local storage error")).not.toBeInTheDocument();
  });
});

describe("filters", () => {
  test("kind groups narrow the list and All restores it", async () => {
    const { user } = setup();
    await screen.findByText("Alpha started");

    await user.click(screen.getByRole("radio", { name: "Problems" }));
    expect(rows()).toHaveLength(1);
    expect(screen.getByText("Alpha api exited with code 1")).toBeInTheDocument();

    await user.click(screen.getByRole("radio", { name: "Ports" }));
    expect(rows()).toHaveLength(1);
    expect(screen.getByText("Beta started listening on :4000")).toBeInTheDocument();

    await user.click(screen.getByRole("radio", { name: "Lifecycle" }));
    expect(rows()).toHaveLength(3);

    await user.click(screen.getByRole("radio", { name: "All" }));
    expect(rows()).toHaveLength(5);
  });

  test("the project select filters to one project and combines with the kind group", async () => {
    const { user } = setup();
    await screen.findByText("Alpha started");

    await user.click(screen.getByRole("combobox", { name: "Project" }));
    await user.click(await screen.findByRole("option", { name: "Beta" }));
    expect(rows().map((row) => row.includes("Beta"))).toEqual([true, true]);

    await user.click(screen.getByRole("radio", { name: "Problems" }));
    expect(screen.getByText("Nothing matches these filters")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Show everything" }));
    expect(rows()).toHaveLength(5);
  });
});

describe("clearing", () => {
  test("asks first; confirming clears the history and reloads", async () => {
    const { user } = setup();
    await screen.findByText("Alpha started");
    await user.click(screen.getByRole("button", { name: "Clear history" }));
    const dialog = screen.getByRole("alertdialog", { name: "Clear activity history?" });
    expect(backend.callsTo("clear_activity")).toHaveLength(0);

    backend.handle("list_activity", () => []);
    await user.click(within(dialog).getByRole("button", { name: "Clear history" }));
    await waitFor(() => expect(backend.callsTo("clear_activity")).toHaveLength(1));
    expect(await screen.findByText("No activity yet")).toBeInTheDocument();
  });

  test("cancelling keeps everything", async () => {
    const { user } = setup();
    await screen.findByText("Alpha started");
    await user.click(screen.getByRole("button", { name: "Clear history" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(backend.callsTo("clear_activity")).toHaveLength(0);
    expect(rows()).toHaveLength(5);
  });

  test("a failed clear is reported and the list stays", async () => {
    const { user } = setup();
    await screen.findByText("Alpha started");
    backend.fail("clear_activity", { code: "DATABASE_ERROR", message: "disk full" });
    await user.click(screen.getByRole("button", { name: "Clear history" }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Clear history" }));
    expect(await screen.findByText("Local storage error")).toBeInTheDocument();
    expect(rows()).toHaveLength(5);
  });
});

describe("live refresh", () => {
  test("a new snapshot refetches the history, throttled to once per window", async () => {
    let served: ActivityEvent[] = EVENTS;
    backend.handle("list_activity", () => served);
    renderWithProviders(<ActivityPage />, { snapshot });
    await screen.findByText("Alpha started");
    expect(backend.callsTo("list_activity")).toHaveLength(1);

    served = [makeActivity({ id: 6, projectId: 2, kind: "portClosed", message: "Beta stopped listening on :4000", createdAt: day(0, 10, 0) }), ...EVENTS];
    // Two ticks inside the window: neither fetches immediately, together they cause one trailing fetch.
    act(() => {
      useRuntimeStore.getState().setSnapshot({ ...snapshot, updatedAt: 2000 });
      useRuntimeStore.getState().setSnapshot({ ...snapshot, updatedAt: 3000 });
    });
    expect(backend.callsTo("list_activity")).toHaveLength(1);

    expect(await screen.findByText("Beta stopped listening on :4000", {}, { timeout: 3500 })).toBeInTheDocument();
    expect(backend.callsTo("list_activity")).toHaveLength(2);
  }, 8000);

  test("a failed refresh keeps the last good list and says so", async () => {
    let fail = false;
    backend.handle("list_activity", () => {
      if (fail) throw { code: "DATABASE_ERROR", message: "database is locked" };
      return EVENTS;
    });
    renderWithProviders(<ActivityPage />, { snapshot });
    await screen.findByText("Alpha started");

    fail = true;
    act(() => useRuntimeStore.getState().setSnapshot({ ...snapshot, updatedAt: 5000 }));
    expect(await screen.findByText(/Couldn't refresh activity: database is locked/, {}, { timeout: 3500 })).toBeInTheDocument();
    expect(rows()).toHaveLength(5);
  }, 8000);
});
