import { afterEach, describe, expect, test } from "bun:test";
import { screen, waitFor, within } from "@testing-library/react";
import { toast } from "sonner";
import { useRuntimeStore } from "@/stores/runtime-store";
import { backend } from "@/test/backend";
import { makeProject, makeScanner, makeSettings, makeSnapshot } from "@/test/fixtures";
import { renderWithProviders } from "@/test/render";
import type { AppSnapshot, EditorInfo, Settings } from "@/types";
import { SettingsPage } from "./SettingsPage";

// Sonner keeps toasts in module state and replays them to the next <Toaster>.
afterEach(() => {
  toast.dismiss();
});

const EDITORS: EditorInfo[] = [
  { id: "vscode", name: "VS Code", available: true },
  { id: "cursor", name: "Cursor", available: true },
  { id: "zed", name: "Zed", available: false },
];

/** Backend that echoes `update_settings` like the real one (validated full replace). */
function backendSavesSettings() {
  backend.handle("update_settings", (args) => args?.settings);
}

/** Renders the page and waits for the editor list so its state update doesn't leak out of the test. */
async function setup({
  settings = makeSettings(),
  snapshot = makeSnapshot(),
  editors = EDITORS,
}: { settings?: Settings; snapshot?: AppSnapshot; editors?: EditorInfo[] } = {}) {
  backend.reply("list_editors", editors);
  const view = renderWithProviders(<SettingsPage />, { settings, snapshot, toaster: true });
  await screen.findByText(/Used by “Open in editor”|No supported editor|Couldn't list editors/);
  return view;
}

const savedSettings = () => useRuntimeStore.getState().settings;

describe("appearance and startup", () => {
  test("changing the theme sends the full settings object and stores what the backend returns", async () => {
    backendSavesSettings();
    const { user } = await setup({
      settings: makeSettings({ launchAtLogin: true, preferredEditor: "cursor", forgetAfterDays: 30 }),
    });
    expect(screen.getByRole("radio", { name: "System" })).toHaveAttribute("aria-checked", "true");

    await user.click(screen.getByRole("radio", { name: "Dark" }));

    await waitFor(() => expect(savedSettings()?.theme).toBe("dark"));
    expect(backend.callsTo("update_settings")).toEqual([
      { settings: { theme: "dark", launchAtLogin: true, preferredEditor: "cursor", forgetAfterDays: 30 } },
    ]);
    expect(screen.getByRole("radio", { name: "Dark" })).toHaveAttribute("aria-checked", "true");
  });

  test("the store follows the backend's version, not the request", async () => {
    // e.g. autostart couldn't be enabled, so the backend stored `false`.
    backend.handle("update_settings", (args) => ({ ...(args?.settings as Settings), launchAtLogin: false }));
    const { user } = await setup();
    await user.click(screen.getByRole("switch", { name: "Launch at login" }));
    await waitFor(() => expect(backend.callsTo("update_settings")).toHaveLength(1));
    await waitFor(() => expect(screen.getByRole("switch", { name: "Launch at login" })).not.toBeDisabled());
    expect(screen.getByRole("switch", { name: "Launch at login" })).not.toBeChecked();
    expect(savedSettings()?.launchAtLogin).toBe(false);
  });

  test("launch at login toggles through update_settings", async () => {
    backendSavesSettings();
    const { user } = await setup();
    const toggle = screen.getByRole("switch", { name: "Launch at login" });
    expect(toggle).not.toBeChecked();
    await user.click(toggle);
    await waitFor(() => expect(savedSettings()?.launchAtLogin).toBe(true));
    expect(backend.callsTo("update_settings")).toEqual([{ settings: makeSettings({ launchAtLogin: true }) }]);
    expect(screen.getByRole("switch", { name: "Launch at login" })).toBeChecked();
    expect(await screen.findByText("LocalDeck will launch at login")).toBeInTheDocument();
  });

  test("a rejected save reverts the control and explains why", async () => {
    backend.fail("update_settings", {
      code: "UNSUPPORTED_OPERATION",
      message: "Couldn't register the login item",
    });
    const { user } = await setup();
    await user.click(screen.getByRole("switch", { name: "Launch at login" }));

    expect(await screen.findByText("Not supported")).toBeInTheDocument();
    expect(screen.getByText(/Couldn't register the login item/)).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("switch", { name: "Launch at login" })).not.toBeDisabled());
    expect(screen.getByRole("switch", { name: "Launch at login" })).not.toBeChecked();
    expect(savedSettings()?.launchAtLogin).toBe(false);
  });

  test("a failed theme change leaves the previous theme selected", async () => {
    backend.fail("update_settings", { code: "DATABASE_ERROR", message: "database is locked" });
    const { user } = await setup({ settings: makeSettings({ theme: "light" }) });
    await user.click(screen.getByRole("radio", { name: "Dark" }));
    expect(await screen.findByText("Local storage error")).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Light" })).toHaveAttribute("aria-checked", "true");
    expect(savedSettings()?.theme).toBe("light");
  });
});

describe("preferred editor", () => {
  test("lists detected editors, marks missing ones and saves the choice", async () => {
    backendSavesSettings();
    const { user } = await setup();
    const trigger = screen.getByRole("combobox", { name: "Preferred editor" });
    await waitFor(() => expect(backend.callsTo("list_editors")).toHaveLength(1));
    expect(trigger).toHaveTextContent("Auto-detect");

    await user.click(trigger);
    const zed = await screen.findByRole("option", { name: /Zed/ });
    expect(zed).toHaveTextContent("not installed");
    expect(zed).toHaveAttribute("aria-disabled", "true");
    await user.click(screen.getByRole("option", { name: "Cursor" }));

    await waitFor(() => expect(savedSettings()?.preferredEditor).toBe("cursor"));
    expect(backend.callsTo("update_settings")).toEqual([
      { settings: makeSettings({ preferredEditor: "cursor" }) },
    ]);
  });

  test("Auto-detect clears the preference (null)", async () => {
    backendSavesSettings();
    const { user } = await setup({ settings: makeSettings({ preferredEditor: "cursor" }) });
    await user.click(screen.getByRole("combobox", { name: "Preferred editor" }));
    await user.click(await screen.findByRole("option", { name: "Auto-detect" }));
    await waitFor(() => expect(savedSettings()?.preferredEditor).toBeNull());
  });

  test("says so when no editor is installed", async () => {
    await setup({ editors: EDITORS.map((editor) => ({ ...editor, available: false })) });
    expect(await screen.findByText(/No supported editor was found/)).toBeInTheDocument();
  });

  test("a failing editor list is reported without breaking the page", async () => {
    backend.fail("list_editors", { code: "UNSUPPORTED_OPERATION", message: "catalogue unreadable" });
    renderWithProviders(<SettingsPage />, { settings: makeSettings(), snapshot: makeSnapshot() });
    expect(await screen.findByText(/Couldn't list editors: catalogue unreadable/)).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Preferred editor" })).toBeInTheDocument();
  });
});

describe("remembered projects", () => {
  test("retention maps to forgetAfterDays, and 'Keep forever' to null", async () => {
    backendSavesSettings();
    const { user } = await setup();
    const trigger = screen.getByRole("combobox", { name: "Forget stopped projects after" });
    expect(trigger).toHaveTextContent("Keep forever");

    await user.click(trigger);
    await user.click(await screen.findByRole("option", { name: "30 days" }));
    await waitFor(() => expect(savedSettings()?.forgetAfterDays).toBe(30));
    expect(backend.callsTo("update_settings")[0]).toEqual({ settings: makeSettings({ forgetAfterDays: 30 }) });

    await waitFor(() => expect(trigger).not.toBeDisabled());
    await user.click(trigger);
    await user.click(await screen.findByRole("option", { name: "Keep forever" }));
    await waitFor(() => expect(savedSettings()?.forgetAfterDays).toBeNull());
  });

  const projects = [
    makeProject({ id: 1, name: "stopped-a" }),
    makeProject({ id: 2, name: "stopped-b" }),
    makeProject({ id: 3, name: "pinned", favorite: true }),
    makeProject({ id: 4, name: "hidden", ignored: true }),
    makeProject({ id: 5, name: "live", runtimeStatus: "running" }),
  ];

  test("forgetting asks first, shows how many projects are affected and reports the removed count", async () => {
    backend.reply("forget_stopped_projects", 2);
    const { user } = await setup({ snapshot: makeSnapshot({ projects }) });

    await user.click(screen.getByRole("button", { name: "Forget stopped projects" }));
    const dialog = screen.getByRole("alertdialog", { name: "Forget 2 stopped projects?" });
    expect(backend.callsTo("forget_stopped_projects")).toHaveLength(0);

    await user.click(within(dialog).getByRole("button", { name: "Forget projects" }));
    await waitFor(() => expect(backend.callsTo("forget_stopped_projects")).toHaveLength(1));
    expect(await screen.findByText("Forgot 2 stopped projects")).toBeInTheDocument();
  });

  test("cancelling does not forget anything", async () => {
    const { user } = await setup({ snapshot: makeSnapshot({ projects }) });
    await user.click(screen.getByRole("button", { name: "Forget stopped projects" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(backend.callsTo("forget_stopped_projects")).toHaveLength(0);
  });

  test("is disabled when nothing qualifies (running, pinned and ignored projects don't count)", async () => {
    await setup({ snapshot: makeSnapshot({ projects: projects.slice(2) }) });
    expect(screen.getByRole("button", { name: "Forget stopped projects" })).toBeDisabled();
    expect(screen.getByText("No stopped projects to forget.")).toBeInTheDocument();
  });

  test("a backend failure is shown and no success toast appears", async () => {
    backend.fail("forget_stopped_projects", { code: "DATABASE_ERROR", message: "disk full" });
    const { user } = await setup({ snapshot: makeSnapshot({ projects }) });
    await user.click(screen.getByRole("button", { name: "Forget stopped projects" }));
    await user.click(screen.getByRole("button", { name: "Forget projects" }));
    expect(await screen.findByText("Local storage error")).toBeInTheDocument();
    expect(screen.queryByText(/^Forgot/)).not.toBeInTheDocument();
  });
});

describe("ignored projects", () => {
  const ignored = makeProject({ id: 7, name: "Old Blog", rootPath: "/home/dev/old-blog", ignored: true });

  test("lists name and path, and Stop ignoring un-ignores that project", async () => {
    backend.reply("ignore_project", undefined);
    const { user } = await setup({
      snapshot: makeSnapshot({ projects: [ignored, makeProject({ id: 8, name: "Visible" })] }),
    });
    expect(screen.getByText("Old Blog")).toBeInTheDocument();
    expect(screen.getByText("/home/dev/old-blog")).toBeInTheDocument();
    expect(screen.queryByText("Visible")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Stop ignoring Old Blog" }));
    await waitFor(() =>
      expect(backend.callsTo("ignore_project")).toEqual([{ projectId: 7, ignored: false }]),
    );
  });

  test("empty text when nothing is ignored", async () => {
    await setup();
    expect(screen.getByText("No ignored projects.")).toBeInTheDocument();
  });
});

describe("system access", () => {
  test("reports full access plainly", async () => {
    await setup({ snapshot: makeSnapshot({ scanner: makeScanner({ platform: "linux" }) }) });
    const section = screen.getByRole("region", { name: "System access" });
    expect(section).toHaveTextContent("100 full · 0 partial · 0 unavailable of 100");
    expect(section).toHaveTextContent("Readable");
    expect(section).toHaveTextContent("Saved locally");
    expect(section).toHaveTextContent("Linux");
    expect(section).toHaveTextContent("every 2 s · last scan took 12 ms");
  });

  test("surfaces limited access: partial processes, skipped sockets, unreadable ports, persistence errors", async () => {
    await setup({
      snapshot: makeSnapshot({
        scanner: makeScanner({
          processAccess: { total: 120, full: 80, partial: 30, unavailable: 10 },
          socketsAvailable: false,
          socketError: { code: "PORT_SCAN_FAILED", message: "netstat2 failed" },
          socketsSkipped: 3,
          persistenceError: { code: "DATABASE_ERROR", message: "database is locked" },
        }),
      }),
    });
    const section = screen.getByRole("region", { name: "System access" });
    expect(section).toHaveTextContent("80 full · 30 partial · 10 unavailable of 120");
    expect(section).toHaveTextContent("Unavailable — netstat2 failed");
    expect(section).toHaveTextContent("3 — shown on Ports without an owner");
    expect(section).toHaveTextContent("Can't be saved — database is locked");
    expect(section).toHaveTextContent("attributed to a project on a guess");
  });
});

describe("about", () => {
  test("states the version and the local-only promise", async () => {
    await setup();
    const about = screen.getByRole("region", { name: "About" });
    expect(about).toHaveTextContent("LocalDeck v0.1");
    expect(about).toHaveTextContent("no telemetry");
  });
});
