import { afterEach, describe, expect, test } from "bun:test";
import { act, screen, waitFor, within } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";
import { useLocation } from "react-router";
import { toast } from "sonner";
import { useUiStore } from "@/stores/ui-store";
import { backend } from "@/test/backend";
import {
  makeComponent,
  makePort,
  makeProject,
  makeRunningProject,
  makeSnapshot,
  makeSettings,
} from "@/test/fixtures";
import { renderWithProviders } from "@/test/render";
import type { AppSnapshot, EditorInfo, Settings } from "@/types";
import { CommandPalette } from "./CommandPalette";

// Sonner keeps toasts in module state and replays them to the next <Toaster>.
afterEach(() => {
  toast.dismiss();
});

const EDITORS: EditorInfo[] = [
  { id: "vscode", name: "VS Code", available: true },
  { id: "cursor", name: "Cursor", available: true },
];

const anilarim = makeRunningProject({
  id: 1,
  name: "Anılarım",
  rootPath: "/home/dev/anilarim",
  stack: [{ id: "next", label: "Next.js" }],
  components: [makeComponent({ id: "web", name: "Web", technology: "Next.js", ports: [3000] })],
  ports: [3000, 4000],
  primaryUrl: { url: "http://localhost:3000", source: "webComponent" },
});
const puq = makeRunningProject({
  id: 2,
  name: "PUQ",
  rootPath: "/home/dev/puq",
  components: [makeComponent({ id: "app", name: "App", technology: "Vite", ports: [5173] })],
  ports: [5173],
  primaryUrl: { url: "http://localhost:5173", source: "webComponent" },
});
const healthnode = makeProject({
  id: 3,
  name: "HealthNode",
  rootPath: "/home/dev/healthnode",
  primaryCommand: anilarim.primaryCommand,
});
const secret = makeProject({ id: 4, name: "Secret", ignored: true });

const ports = [
  makePort({
    port: 3000,
    ownership: "assigned",
    projectId: 1,
    componentId: "web",
    http: "confirmed",
    isHttp: true,
    url: "http://localhost:3000",
    processes: [{ pid: 1234, name: "next", startedAt: null }],
  }),
  makePort({
    port: 5173,
    ownership: "assigned",
    projectId: 2,
    http: "confirmed",
    isHttp: true,
    url: "http://localhost:5173",
    processes: [{ pid: 5173, name: "vite", startedAt: null }],
  }),
  makePort({ port: 5432, processes: [{ pid: 900, name: "postgres", startedAt: null }] }),
  makePort({ port: 4000, ownership: "assigned", projectId: 1, processes: [{ pid: 1235, name: "bun", startedAt: null }] }),
];

const baseSnapshot = makeSnapshot({ projects: [healthnode, puq, anilarim, secret], ports });

function LocationProbe() {
  const location = useLocation();
  return <output aria-label="location">{location.pathname}</output>;
}

async function setup({
  snapshot = baseSnapshot,
  settings = makeSettings({ preferredEditor: "cursor" }),
  open = true,
}: { snapshot?: AppSnapshot; settings?: Settings; open?: boolean } = {}) {
  backend.reply("list_editors", EDITORS);
  if (open) useUiStore.getState().setPaletteOpen(true);
  const view = renderWithProviders(
    <>
      <CommandPalette />
      <LocationProbe />
    </>,
    { snapshot, settings, toaster: true },
  );
  // Let the editor list resolve inside act() so its state update doesn't leak out of the test.
  await act(async () => {});
  return view;
}

/** A string matches the accessible name exactly; items with muted trailing text (project, port) need a regex. */
const named = (name: string | RegExp) =>
  typeof name === "string" ? new RegExp(`^${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`) : name;
const option = (name: string | RegExp) => screen.getByRole("option", { name: named(name) });
const queryOption = (name: string | RegExp) => screen.queryByRole("option", { name: named(name) });
const labels = () => screen.queryAllByRole("option").map((o) => o.textContent ?? "");

async function search(user: UserEvent, text: string) {
  await user.type(screen.getByRole("combobox", { name: "Search projects, ports and actions" }), text);
}

describe("opening and closing", () => {
  test("renders nothing until the store opens it", async () => {
    await setup({ open: false });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(backend.callsTo("list_editors")).toHaveLength(0);
  });

  test("opens from the store, shows the shortcut hint and loads the editors once", async () => {
    await setup();
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    expect(screen.getByText("⌘K")).toBeInTheDocument();
    expect(option("Go to Ports")).toBeInTheDocument();
    await waitFor(() => expect(backend.callsTo("list_editors")).toHaveLength(1));
  });

  test("Escape closes it", async () => {
    const { user } = await setup();
    await screen.findByRole("dialog");
    await user.keyboard("{Escape}");
    expect(useUiStore.getState().paletteOpen).toBe(false);
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  test("the search starts empty again after closing", async () => {
    const { user } = await setup();
    await search(user, "puq");
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    act(() => useUiStore.getState().setPaletteOpen(true));
    expect(await screen.findByRole("combobox")).toHaveValue("");
  });
});

describe("navigation and projects", () => {
  test("Navigate group goes to the screen and closes the palette", async () => {
    const { user } = await setup();
    await user.click(option("Go to Settings"));
    expect(screen.getByLabelText("location")).toHaveTextContent("/settings");
    expect(useUiStore.getState().paletteOpen).toBe(false);
  });

  test("jumps to a project; ignored projects are not offered; running projects come first", async () => {
    const { user } = await setup();
    const projects = within(screen.getByRole("group", { name: "Projects" }))
      .getAllByRole("option")
      .map((o) => o.textContent);
    expect(projects).toEqual(["Anılarım :3000 · :4000", "PUQ :5173", "HealthNode"]);
    expect(screen.queryByText("Secret")).not.toBeInTheDocument();
    expect(queryOption(/Secret/)).not.toBeInTheDocument();

    await user.click(within(screen.getByRole("group", { name: "Projects" })).getByRole("option", { name: /PUQ/ }));
    expect(screen.getByLabelText("location")).toHaveTextContent("/projects/2");
  });
});

describe("project actions", () => {
  test("running projects expose open / restart / stop / editor / folder / terminal / copy", async () => {
    await setup();
    expect(option("Restart Anılarım")).toBeInTheDocument();
    expect(option("Stop PUQ")).toBeInTheDocument();
    expect(option(/^Open Anılarım localhost:3000$/)).toHaveTextContent("localhost:3000");
    expect(option(/^Copy Anılarım URL localhost:3000$/)).toBeInTheDocument();
    expect(option("Reveal Anılarım folder")).toBeInTheDocument();
    expect(option("Open terminal in Anılarım")).toBeInTheDocument();
    // The editor name arrives with listEditors(): the settings default is Cursor.
    expect(await screen.findByRole("option", { name: /^Open Anılarım in Cursor/ })).toBeInTheDocument();
  });

  test("only actions that apply are offered: stopped projects can start, not stop or restart", async () => {
    await setup();
    expect(option("Start HealthNode")).toBeInTheDocument();
    expect(queryOption("Stop HealthNode")).not.toBeInTheDocument();
    expect(queryOption("Restart HealthNode")).not.toBeInTheDocument();
    expect(queryOption("Open HealthNode")).not.toBeInTheDocument(); // no URL yet
    expect(queryOption("Start Anılarım")).not.toBeInTheDocument();
    expect(queryOption("Force stop Anılarım")).not.toBeInTheDocument();
    expect(option("Forget HealthNode")).toBeInTheDocument();
  });

  test("running projects' actions are listed before stopped ones", async () => {
    await setup();
    const actions = within(screen.getByRole("group", { name: "Actions" }))
      .getAllByRole("option")
      .map((o) => o.textContent ?? "");
    const lastRunning = actions.map((text) => /Anılarım|PUQ/.test(text)).lastIndexOf(true);
    const firstStopped = actions.findIndex((text) => /HealthNode/.test(text));
    expect(lastRunning).toBeLessThan(firstStopped);
  });

  test("running an action calls its backend command and closes the palette", async () => {
    backend.reply("restart_project", undefined);
    const { user } = await setup();
    await user.click(option("Restart Anılarım"));
    await waitFor(() => expect(backend.callsTo("restart_project")).toEqual([{ projectId: 1 }]));
    expect(useUiStore.getState().paletteOpen).toBe(false);
    expect(await screen.findByText("Restarting Anılarım")).toBeInTheDocument();
  });

  test("Enter runs the highlighted match", async () => {
    backend.reply("open_project_folder", undefined);
    const { user } = await setup();
    await search(user, "reveal puq");
    await user.keyboard("{Enter}");
    await waitFor(() => expect(backend.callsTo("open_project_folder")).toEqual([{ projectId: 2 }]));
  });

  test("open in editor and open terminal hit their commands", async () => {
    backend.reply("open_project_editor", undefined);
    backend.reply("open_project_terminal", undefined);
    const { user } = await setup();
    await user.click(await screen.findByRole("option", { name: /^Open Anılarım in Cursor/ }));
    await waitFor(() => expect(backend.callsTo("open_project_editor")).toEqual([{ projectId: 1 }]));
    act(() => useUiStore.getState().setPaletteOpen(true));
    await user.click(await screen.findByRole("option", { name: "Open terminal in PUQ" }));
    await waitFor(() => expect(backend.callsTo("open_project_terminal")).toEqual([{ projectId: 2 }]));
  });

  test("a backend failure is reported as a toast", async () => {
    backend.fail("stop_project", { code: "PROCESS_PERMISSION_DENIED", message: "Operation not permitted" });
    const { user } = await setup();
    await user.click(option("Stop PUQ"));
    expect(await screen.findByText("Permission denied")).toBeInTheDocument();
  });
});

describe("Force Stop is confirmation-gated", () => {
  const stopping = makeRunningProject({ id: 9, name: "Stubborn", stopPending: true });
  const snapshot = makeSnapshot({ projects: [stopping] });

  test("selecting it closes the palette and asks first; nothing is killed until confirmed", async () => {
    backend.reply("force_stop_project", { signaled: [1], failed: [] });
    const { user } = await setup({ snapshot });
    await user.click(option("Force stop Stubborn"));

    expect(useUiStore.getState().paletteOpen).toBe(false);
    const dialog = await screen.findByRole("alertdialog", { name: "Force stop Stubborn?" });
    expect(backend.callsTo("force_stop_project")).toHaveLength(0);

    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(backend.callsTo("force_stop_project")).toHaveLength(0);

    act(() => useUiStore.getState().setPaletteOpen(true));
    await user.click(await screen.findByRole("option", { name: "Force stop Stubborn" }));
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Force Stop" }));
    await waitFor(() => expect(backend.callsTo("force_stop_project")).toEqual([{ projectId: 9 }]));
  });

  test("Forget is gated the same way", async () => {
    backend.reply("forget_project", undefined);
    const { user } = await setup();
    await user.click(option("Forget HealthNode"));
    const dialog = await screen.findByRole("alertdialog", { name: "Forget HealthNode?" });
    expect(backend.callsTo("forget_project")).toHaveLength(0);
    await user.click(within(dialog).getByRole("button", { name: "Forget" }));
    await waitFor(() => expect(backend.callsTo("forget_project")).toEqual([{ projectId: 3 }]));
  });
});

describe("search", () => {
  test("by project name narrows projects and their actions", async () => {
    const { user } = await setup();
    await search(user, "anıl");
    expect(labels().every((text) => /Anılarım/.test(text))).toBe(true);
    expect(option("Restart Anılarım")).toBeInTheDocument();
    expect(queryOption(/PUQ/)).not.toBeInTheDocument();
  });

  test("by root path", async () => {
    const { user } = await setup();
    await search(user, "/home/dev/healthnode");
    expect(labels()).toContain("Start HealthNode");
    expect(queryOption(/Anılarım/)).not.toBeInTheDocument();
  });

  test("by component and stack text", async () => {
    const { user } = await setup();
    await search(user, "next.js");
    expect(within(screen.getByRole("group", { name: "Projects" })).getAllByRole("option")).toHaveLength(1);
    expect(option("Restart Anılarım")).toBeInTheDocument();
    expect(queryOption(/PUQ/)).not.toBeInTheDocument();
  });

  test.each(["3000", ":3000", "localhost:3000"])("by port: %s finds the project, its actions and the port entries", async (text) => {
    const { user } = await setup();
    await search(user, text);
    expect(option("Restart Anılarım")).toBeInTheDocument();
    expect(option(/^Open localhost:3000 Anılarım$/)).toBeInTheDocument();
    expect(option(/^Reveal process on :3000 /)).toBeInTheDocument();
    expect(queryOption(/PUQ/)).not.toBeInTheDocument();
    expect(queryOption(/^Reveal process on :5173/)).not.toBeInTheDocument();
  });

  test("verbs and names combine: 'stop puq' → only Stop PUQ", async () => {
    const { user } = await setup();
    await search(user, "stop puq");
    expect(labels()).toEqual(["Stop PUQ"]);
  });

  test("unassigned ports are found by process name", async () => {
    const { user } = await setup();
    await search(user, "postgres");
    expect(labels()).toEqual(["Reveal process on :5432 postgres"]);
  });

  test("shows empty-result text", async () => {
    const { user } = await setup();
    await search(user, "zzzzqq");
    expect(screen.getByText("No matching projects, ports or actions.")).toBeInTheDocument();
    expect(screen.queryAllByRole("option")).toHaveLength(0);
  });
});

describe("port entries", () => {
  test("Open localhost:3000 opens that endpoint through open_port_url and closes", async () => {
    backend.reply("open_port_url", undefined);
    const { user } = await setup();
    await user.click(option(/^Open localhost:3000 Anılarım$/));
    await waitFor(() => expect(backend.callsTo("open_port_url")).toEqual([{ portId: "tcp:3000" }]));
    expect(useUiStore.getState().paletteOpen).toBe(false);
  });

  test("only HTTP endpoints get an Open entry", async () => {
    await setup();
    expect(queryOption(/Open localhost:5432/)).not.toBeInTheDocument();
    expect(queryOption(/Open localhost:4000/)).not.toBeInTheDocument();
    expect(option(/^Reveal process on :5432 postgres$/)).toBeInTheDocument();
  });

  test("Reveal process navigates to Ports with the search set and the row selected", async () => {
    useUiStore.getState().setPortsFilter("http");
    const { user } = await setup();
    await user.click(option(/^Reveal process on :5173 /));
    expect(screen.getByLabelText("location")).toHaveTextContent("/ports");
    expect(useUiStore.getState()).toMatchObject({
      portsSearch: ":5173",
      portsFilter: "all",
      selectedPortId: "tcp:5173",
      paletteOpen: false,
    });
  });
});
