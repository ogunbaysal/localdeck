import { describe, expect, test } from "bun:test";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { backend } from "@/test/backend";
import { makeProject, makeScanner, makeSettings, makeSnapshot } from "@/test/fixtures";
import { useRuntimeStore } from "@/stores/runtime-store";
import { useUiStore } from "@/stores/ui-store";
import type { AppSnapshot, Settings } from "@/types";
import { Providers } from "./providers";
import { AppRoutes } from "./router";

function backendReturns(snapshot: AppSnapshot = makeSnapshot(), settings: Settings = makeSettings()) {
  backend.reply("get_snapshot", snapshot);
  backend.reply("get_settings", settings);
}

function renderApp(route = "/") {
  const user = userEvent.setup();
  const view = render(
    <MemoryRouter initialEntries={[route]}>
      <Providers>
        <AppRoutes />
      </Providers>
    </MemoryRouter>,
  );
  return { user, ...view };
}

const alpha = makeProject({ id: 1, name: "Alpha", rootPath: "/home/dev/alpha" });

describe("bootstrap states", () => {
  test("shows a loading skeleton, then the routed page once snapshot and settings arrive", async () => {
    backendReturns(makeSnapshot({ projects: [alpha] }));
    renderApp();
    expect(screen.getByRole("status", { name: "Loading projects" })).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: /^Projects/ })).toBeInTheDocument();
    expect(screen.queryByRole("status", { name: "Loading projects" })).not.toBeInTheDocument();
    expect(useRuntimeStore.getState().status).toBe("ready");
  });

  test("a failed load is a fatal, specific error with a working Retry", async () => {
    backend.fail("get_snapshot", { code: "DATABASE_ERROR", message: "Database error: database is locked" });
    backend.reply("get_settings", makeSettings());
    const { user } = renderApp();

    expect(await screen.findByRole("heading", { name: "Local storage error" })).toBeInTheDocument();
    expect(screen.getByText("Database error: database is locked")).toBeInTheDocument();
    expect(useRuntimeStore.getState()).toMatchObject({ status: "error", error: { code: "DATABASE_ERROR" } });
    expect(screen.queryByRole("heading", { name: /^Projects/ })).not.toBeInTheDocument();

    backendReturns();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("heading", { name: /^Projects/ })).toBeInTheDocument();
    expect(useRuntimeStore.getState()).toMatchObject({ status: "ready", error: null });
  });

  test("runtime://updated events replace the snapshot and unmounting removes every listener", async () => {
    backendReturns(makeSnapshot({ projects: [alpha], updatedAt: 1000 }));
    const { unmount } = renderApp();
    await screen.findByRole("heading", { name: /^Projects/ });

    act(() => {
      backend.emit("runtime://updated", makeSnapshot({ projects: [alpha, makeProject({ id: 2, name: "Beta" })], updatedAt: 3000 }));
    });
    expect(useRuntimeStore.getState().snapshot?.projects).toHaveLength(2);

    unmount();
    expect(backend.listenerCount("runtime://updated")).toBe(0);
    expect(backend.listenerCount("ui://open-project")).toBe(0);
  });
});

describe("routing", () => {
  test("navigation is exactly Projects, Ports, Settings: no Processes page", async () => {
    backendReturns();
    const { user } = renderApp();
    await screen.findByRole("heading", { name: /^Projects/ });

    const nav = screen.getByRole("navigation", { name: "Primary" });
    expect(within(nav).getAllByRole("link").map((link) => link.textContent?.replace(/\d+$/, ""))).toEqual([
      "Projects",
      "Ports",
      "Settings",
    ]);
    expect(within(nav).getByRole("link", { name: /^Projects/ })).toHaveAttribute("aria-current", "page");

    await user.click(within(nav).getByRole("link", { name: /^Ports/ }));
    expect(await screen.findByRole("heading", { name: /^Ports/ })).toBeInTheDocument();
    expect(within(nav).getByRole("link", { name: /^Ports/ })).toHaveAttribute("aria-current", "page");

    await user.click(within(nav).getByRole("link", { name: "Settings" }));
    expect(await screen.findByRole("heading", { name: "Settings" })).toBeInTheDocument();

    await user.click(within(nav).getByRole("link", { name: /^Projects/ }));
    expect(await screen.findByRole("heading", { name: /^Projects/ })).toBeInTheDocument();
  });

  test("/projects/:id renders that project and keeps Projects highlighted; unknown ids say so", async () => {
    backendReturns(makeSnapshot({ projects: [alpha] }));
    const { unmount } = renderApp("/projects/1");
    expect(await screen.findByRole("heading", { name: "Alpha" })).toBeInTheDocument();
    expect(within(screen.getByRole("navigation", { name: "Primary" })).getByRole("link", { name: /^Projects/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
    unmount();

    backendReturns(makeSnapshot({ projects: [alpha] }));
    renderApp("/projects/99");
    expect(await screen.findByRole("heading", { name: "Project not found" })).toBeInTheDocument();
  });

  test("unknown paths fall back to Projects", async () => {
    backendReturns();
    renderApp("/processes");
    expect(await screen.findByRole("heading", { name: /^Projects/ })).toBeInTheDocument();
  });

  test("a tray click (ui://open-project) navigates to that project", async () => {
    backendReturns(makeSnapshot({ projects: [alpha] }));
    renderApp();
    await screen.findByRole("heading", { name: /^Projects/ });

    act(() => backend.emit("ui://open-project", 1));
    expect(await screen.findByRole("heading", { name: "Alpha" })).toBeInTheDocument();
  });
});

describe("command palette shortcut", () => {
  test("Cmd+K opens the palette on macOS; Ctrl+K does not", async () => {
    backendReturns(makeSnapshot({ scanner: makeScanner({ platform: "macos" }) }));
    renderApp();
    await screen.findByRole("heading", { name: /^Projects/ });

    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect(useUiStore.getState().paletteOpen).toBe(false);

    fireEvent.keyDown(window, { key: "k", metaKey: true });
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    expect(screen.getByText("Go to Ports")).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "k", metaKey: true });
    expect(useUiStore.getState().paletteOpen).toBe(false);
  });

  test("Ctrl+K opens it elsewhere, and the top bar hint matches the platform", async () => {
    backendReturns(makeSnapshot({ scanner: makeScanner({ platform: "linux" }) }));
    renderApp();
    await screen.findByRole("heading", { name: /^Projects/ });

    expect(screen.getByRole("button", { name: /search projects, ports and actions/i })).toHaveTextContent("Ctrl+K");
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect(useUiStore.getState().paletteOpen).toBe(true);
  });

  test("the top bar search trigger opens the palette", async () => {
    backendReturns();
    const { user } = renderApp();
    await screen.findByRole("heading", { name: /^Projects/ });
    await user.click(screen.getByRole("button", { name: /search projects, ports and actions/i }));
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
  });
});

describe("platform and limited access", () => {
  test("the scanner's platform is mirrored onto <html data-platform>", async () => {
    backendReturns(makeSnapshot({ scanner: makeScanner({ platform: "linux" }) }));
    renderApp();
    await screen.findByRole("heading", { name: /^Projects/ });
    expect(document.documentElement.dataset.platform).toBe("linux");
  });

  test("a healthy scanner shows Live and no banner", async () => {
    backendReturns();
    renderApp();
    await screen.findByRole("heading", { name: /^Projects/ });
    expect(screen.queryByText("Limited access", { selector: "[data-slot=alert-title]" })).not.toBeInTheDocument();
    expect(document.querySelector("[data-slot=scanner-indicator]")).toHaveAttribute("data-limited", "false");
  });

  test("unreadable sockets surface a calm banner and an indicator, without breaking the page", async () => {
    backendReturns(
      makeSnapshot({
        scanner: makeScanner({
          socketsAvailable: false,
          socketError: { code: "PORT_SCAN_FAILED", message: "Socket discovery failed: permission denied" },
        }),
      }),
    );
    renderApp();
    expect(await screen.findByRole("heading", { name: /^Projects/ })).toBeInTheDocument();
    const banner = document.querySelector("[data-slot=limited-access-banner]");
    expect(banner).toHaveTextContent("Listening ports unavailable");
    expect(banner).toHaveTextContent("Socket discovery failed: permission denied");
    expect(document.querySelector("[data-slot=scanner-indicator]")).toHaveAttribute("data-limited", "true");
  });
});
