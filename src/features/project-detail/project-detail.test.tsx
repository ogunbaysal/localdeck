import { beforeAll, describe, expect, test } from "bun:test";
import { act, screen, waitFor, within } from "@testing-library/react";
import { Route, Routes } from "react-router";
import { backend } from "@/test/backend";
import {
  makeActivity,
  makeAttention,
  makeCommand,
  makeComponent,
  makePort,
  makeProcess,
  makeProject,
  makeRunningProject,
  makeSnapshot,
} from "@/test/fixtures";
import { renderWithProviders } from "@/test/render";
import { useRuntimeStore } from "@/stores/runtime-store";
import type { PortInfo, Project } from "@/types";
import { ProjectDetailPage } from "./ProjectDetailPage";

// Radix Select / DropdownMenu poke at pointer-capture and scrolling APIs happy-dom lacks.
beforeAll(() => {
  const proto = Element.prototype as unknown as Record<string, unknown>;
  proto.hasPointerCapture ??= () => false;
  proto.setPointerCapture ??= () => {};
  proto.releasePointerCapture ??= () => {};
  proto.scrollIntoView ??= () => {};
});

async function setup(
  project: Project,
  ports: PortInfo[] = [],
  route = `/projects/${project.id}`,
  toaster = false,
) {
  backend.reply("list_editors", []);
  const view = renderWithProviders(
    <Routes>
      <Route path="/" element={<p>Projects home</p>} />
      <Route path="projects/:projectId" element={<ProjectDetailPage />} />
    </Routes>,
    { route, snapshot: makeSnapshot({ projects: [project], ports }), toaster },
  );
  // Let the editor list resolve inside act() so nothing updates outside of it later.
  await act(async () => {});
  return view;
}

const web = makeComponent({ id: "web", key: "web", name: "Frontend", kind: "web", ports: [3000], rootPid: 100 });
const api = makeComponent({
  id: "api",
  key: "api",
  name: "API",
  kind: "api",
  detectedKind: "api",
  technology: "Express",
  rootPid: 200,
  ports: [4000],
  origin: "localDeckManaged",
  command: "node server.js",
});

const rootProc = makeProcess({ pid: 100, name: "bun", componentId: "web", depth: 0, args: ["bun", "run", "dev"] });
const childProc = makeProcess({ pid: 101, parentPid: 100, name: "vite", componentId: "web", depth: 1, args: ["node", "vite"] });
const apiProc = makeProcess({ pid: 200, name: "node", componentId: "api", depth: 0, args: ["node", "server.js"] });

function runningProject(overrides: Partial<Project> = {}): Project {
  return makeRunningProject({
    id: 1,
    name: "LocalDeck",
    components: [web, api],
    processes: [rootProc, childProc, apiProc],
    ports: [3000, 4000],
    ...overrides,
  });
}

const httpPort = makePort({
  id: "tcp:3000",
  port: 3000,
  ownership: "assigned",
  projectId: 1,
  componentId: "web",
  http: "confirmed",
  isHttp: true,
  url: "http://localhost:3000",
});
const tcpPort = makePort({
  id: "tcp:4000",
  port: 4000,
  ownership: "shared",
  projectId: 1,
  componentId: null,
  processes: [{ pid: 200, name: "node", startedAt: 1 }],
});

describe("rendering", () => {
  test("shows header, components and owned ports", async () => {
    await setup(runningProject(), [httpPort, tcpPort]);

    expect(screen.getByRole("heading", { level: 1, name: "LocalDeck" })).toBeInTheDocument();
    expect(within(screen.getByRole("banner")).getByText("Running")).toBeInTheDocument();
    expect(screen.getByText(/Git root/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "← Projects" })).toBeInTheDocument();

    const components = within(screen.getByRole("region", { name: "Components" }));
    expect(components.getByText("Frontend")).toBeInTheDocument();
    expect(components.getByText("Express")).toBeInTheDocument();
    expect(components.getByText("External")).toBeInTheDocument();
    expect(components.getByText("Managed by LocalDeck")).toBeInTheDocument();
    expect(components.getByText("node server.js")).toBeInTheDocument();

    const portsTable = within(screen.getByRole("region", { name: "Ports" }));
    const row3000 = within(portsTable.getByText(":3000").closest("tr")!);
    expect(row3000.getByText("HTTP")).toBeInTheDocument();
    expect(row3000.getByText("Frontend")).toBeInTheDocument();
    expect(row3000.getByText("http://localhost:3000")).toBeInTheDocument();
    const row4000 = within(portsTable.getByText(":4000").closest("tr")!);
    expect(row4000.getByText("TCP")).toBeInTheDocument();
    expect(row4000.getByText("Several components")).toBeInTheDocument();
  });

  test("never shows numeric confidence and lists recent activity newest first", async () => {
    await setup(
      runningProject({
        recentActivity: [
          makeActivity({ id: 2, message: "Port 3000 opened", createdAt: 2_000_000_000_000 }),
          makeActivity({ id: 1, message: "LocalDeck started", createdAt: 1_999_999_000_000 }),
        ],
      }),
    );
    expect(screen.queryByText(/confidence/i)).not.toBeInTheDocument();
    const items = within(screen.getByRole("region", { name: "Recent activity" })).getAllByRole("listitem");
    expect(items.map((item) => item.textContent)).toEqual([
      expect.stringContaining("Port 3000 opened"),
      expect.stringContaining("LocalDeck started"),
    ]);
  });

  test("an empty activity list says so", async () => {
    await setup(runningProject());
    expect(screen.getByText("No activity yet")).toBeInTheDocument();
  });

  test("a port chip opens its HTTP endpoint; non-HTTP chips are not buttons", async () => {
    backend.reply("open_port_url", undefined);
    const { user } = await setup(runningProject(), [httpPort, tcpPort]);
    const overview = within(screen.getByRole("region", { name: "Overview" }));
    expect(overview.queryByRole("button", { name: /4000/ })).not.toBeInTheDocument();
    await user.click(overview.getByRole("button", { name: "Open http://localhost:3000" }));
    expect(backend.callsTo("open_port_url")).toEqual([{ portId: "tcp:3000" }]);
  });

  test("not-found state for an unknown or malformed project id", async () => {
    const { unmount } = await setup(runningProject(), [], "/projects/99");
    expect(screen.getByRole("heading", { name: "Project not found" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "← Projects" })).toHaveAttribute("href", "/");
    unmount();
    await setup(runningProject(), [], "/projects/abc");
    expect(screen.getByRole("heading", { name: "Project not found" })).toBeInTheDocument();
  });
});

describe("component type", () => {
  test("changing the type calls set_component_kind with the component key", async () => {
    backend.reply("set_component_kind", undefined);
    const { user } = await setup(runningProject());
    await user.click(screen.getByRole("combobox", { name: "Change type of API" }));
    await user.click(await screen.findByRole("option", { name: "Worker" }));
    await waitFor(() =>
      expect(backend.callsTo("set_component_kind")).toEqual([{ projectId: 1, componentKey: "api", kind: "worker" }]),
    );
  });

  test("a corrected component is marked and can go back to auto-detection", async () => {
    backend.reply("set_component_kind", undefined);
    const corrected = makeComponent({ ...api, kind: "worker", detectedKind: "api", kindOverridden: true });
    const { user } = await setup(runningProject({ components: [web, corrected] }));
    expect(screen.getByText("corrected")).toBeInTheDocument();
    await user.click(screen.getByRole("combobox", { name: "Change type of API" }));
    await user.click(await screen.findByRole("option", { name: "Auto (detected: API)" }));
    await waitFor(() =>
      expect(backend.callsTo("set_component_kind")).toEqual([{ projectId: 1, componentKey: "api", kind: null }]),
    );
  });

  test("offers all twelve kinds plus auto", async () => {
    const { user } = await setup(runningProject());
    await user.click(screen.getByRole("combobox", { name: "Change type of Frontend" }));
    expect(await screen.findAllByRole("option")).toHaveLength(13);
  });

  test("expanding a component shows only its own process tree", async () => {
    const { user } = await setup(runningProject());
    await user.click(screen.getByRole("button", { name: "Show processes of Frontend" }));
    const tree = within(screen.getByRole("list", { name: "Processes of Frontend" }));
    expect(tree.getByText("bun")).toBeInTheDocument();
    expect(tree.getByText("vite")).toBeInTheDocument();
    expect(tree.queryByText("node")).not.toBeInTheDocument();
  });
});

describe("stopped project", () => {
  const stopped = makeProject({ id: 1, name: "LocalDeck", primaryCommand: makeCommand() });

  test("explains there are no running components and offers Start", async () => {
    backend.reply("start_project", undefined);
    const { user } = await setup(stopped);
    expect(screen.getByText("No running components")).toBeInTheDocument();
    expect(screen.getByText("No listening ports")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Restart" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Stop" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Start" }));
    expect(backend.callsTo("start_project")).toEqual([{ projectId: 1 }]);
  });

  test("Start is disabled with the reason when no command is configured", async () => {
    await setup(makeProject({ id: 1 }));
    const start = screen.getByRole("button", { name: "Start" });
    expect(start).toBeDisabled();
    expect(start).toHaveAccessibleDescription("No start command configured");
  });
});

describe("header actions", () => {
  test("running project offers Open, Restart and Stop but not Force Stop", async () => {
    backend.reply("stop_project", { signaled: [100], failed: [] });
    const { user } = await setup(runningProject());
    expect(screen.getByRole("button", { name: "Open" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Restart" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "Force Stop" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Start" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Stop" }));
    await waitFor(() => expect(backend.callsTo("stop_project")).toEqual([{ projectId: 1 }]));
  });

  test("Restart without a start command is disabled and says why", async () => {
    await setup(runningProject({ primaryCommand: null }));
    const restart = screen.getByRole("button", { name: "Restart" });
    expect(restart).toBeDisabled();
    expect(restart).toHaveAccessibleDescription("No start command configured");
  });

  test("Force Stop appears only while a stop is pending and needs confirmation", async () => {
    backend.reply("force_stop_project", { signaled: [100], failed: [] });
    const { user } = await setup(runningProject({ stopPending: true }));
    expect(screen.getByText("Stopping…")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Force Stop" }));
    const dialog = screen.getByRole("alertdialog");
    expect(within(dialog).getByText("Force stop LocalDeck?")).toBeInTheDocument();
    expect(backend.callsTo("force_stop_project")).toHaveLength(0);

    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(backend.callsTo("force_stop_project")).toHaveLength(0);

    await user.click(screen.getByRole("button", { name: "Force Stop" }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Force Stop" }));
    await waitFor(() => expect(backend.callsTo("force_stop_project")).toEqual([{ projectId: 1 }]));
  });

  test("overflow menu lists the remaining actions and Forget needs confirmation, then leaves the page", async () => {
    backend.reply("forget_project", undefined);
    backend.reply("get_snapshot", makeSnapshot({ projects: [], updatedAt: 2_000_000_000_000 }));
    const { user } = await setup(makeProject({ id: 1, name: "LocalDeck" }));
    await user.click(screen.getByRole("button", { name: "More actions" }));
    expect(screen.getByRole("menuitem", { name: "Open Terminal" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Project settings…" })).toBeInTheDocument();
    await user.click(screen.getByRole("menuitem", { name: "Forget" }));
    expect(backend.callsTo("forget_project")).toHaveLength(0);
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Forget" }));
    await waitFor(() => expect(backend.callsTo("forget_project")).toEqual([{ projectId: 1 }]));
    expect(await screen.findByText("Projects home")).toBeInTheDocument();
  });
});

describe("a project that disappears", () => {
  test("turns into the not-found state with a way back", async () => {
    await setup(runningProject());
    expect(screen.getByRole("heading", { level: 1, name: "LocalDeck" })).toBeInTheDocument();
    act(() => {
      useRuntimeStore.getState().setSnapshot(makeSnapshot({ projects: [], updatedAt: 2_000_000_000_000 }));
    });
    expect(screen.getByRole("heading", { name: "Project not found" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "← Projects" })).toBeInTheDocument();
  });
});

describe("attention", () => {
  test("dismissible items can be dismissed; others have no button", async () => {
    backend.reply("dismiss_attention", undefined);
    const { user } = await setup(
      runningProject({
        attention: [
          makeAttention({ id: "port-conflict:3000", message: "Port 3000 is also used elsewhere" }),
          makeAttention({
            id: "failed:1",
            kind: "actionFailed",
            message: "Start failed",
            dismissible: true,
          }),
        ],
      }),
    );
    expect(screen.getByText("Port 3000 is also used elsewhere")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Dismiss" })).toHaveLength(1);
    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(backend.callsTo("dismiss_attention")).toEqual([{ projectId: 1, attentionId: "failed:1" }]);
  });
});

describe("process diagnostics", () => {
  test("collapsed by default and expands on demand", async () => {
    const { user } = await setup(runningProject());
    const trigger = screen.getByRole("button", { name: "Processes (3)" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("table", { name: "" })).not.toBeInTheDocument();
    expect(screen.queryByRole("columnheader", { name: "PPID" })).not.toBeInTheDocument();

    await user.click(trigger);
    expect(screen.getByRole("columnheader", { name: "PPID" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Actions for vite (PID 101)" })).toBeInTheDocument();
  });

  test("Terminate is immediate and passes the start time", async () => {
    backend.reply("terminate_process", undefined);
    const { user } = await setup(runningProject());
    await user.click(screen.getByRole("button", { name: "Processes (3)" }));
    await user.click(screen.getByRole("button", { name: "Actions for vite (PID 101)" }));
    await user.click(screen.getByRole("menuitem", { name: "Terminate" }));
    await waitFor(() =>
      expect(backend.callsTo("terminate_process")).toEqual([{ pid: 101, startedAt: childProc.startedAt }]),
    );
  });

  test("Force Kill needs an explicit confirmation and explains the risk", async () => {
    backend.reply("force_kill_process", undefined);
    const { user } = await setup(runningProject());
    await user.click(screen.getByRole("button", { name: "Processes (3)" }));
    await user.click(screen.getByRole("button", { name: "Actions for vite (PID 101)" }));
    await user.click(screen.getByRole("menuitem", { name: "Force Kill…" }));

    const dialog = screen.getByRole("alertdialog");
    expect(within(dialog).getByText("Force kill vite (PID 101)?")).toBeInTheDocument();
    expect(within(dialog).getByText(/cannot catch or ignore it/)).toBeInTheDocument();
    expect(backend.callsTo("force_kill_process")).toHaveLength(0);

    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(backend.callsTo("force_kill_process")).toHaveLength(0);
    expect(backend.callsTo("terminate_process")).toHaveLength(0);

    await user.click(screen.getByRole("button", { name: "Actions for vite (PID 101)" }));
    await user.click(screen.getByRole("menuitem", { name: "Force Kill…" }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Force Kill" }));
    await waitFor(() =>
      expect(backend.callsTo("force_kill_process")).toEqual([{ pid: 101, startedAt: childProc.startedAt }]),
    );
  });

  test("Open working directory is disabled when the OS reported no cwd", async () => {
    const { user } = await setup(runningProject({ processes: [{ ...rootProc, cwd: null }, childProc, apiProc] }));
    await user.click(screen.getByRole("button", { name: "Processes (3)" }));
    await user.click(screen.getByRole("button", { name: "Actions for bun (PID 100)" }));
    expect(screen.getByRole("menuitem", { name: /Open working directory/ })).toHaveAttribute("aria-disabled", "true");
  });

  test("limited-access processes get an explanatory hint", async () => {
    const { user } = await setup(
      runningProject({ processes: [{ ...rootProc, accessLevel: "partial" }, childProc, apiProc] }),
    );
    await user.click(screen.getByRole("button", { name: "Processes (3)" }));
    expect(screen.getByText("Partial")).toBeInTheDocument();
    expect(screen.getByText(/limited access/)).toBeInTheDocument();
  });
});

describe("failures", () => {
  test("a structured backend error from an action shows a meaningful toast", async () => {
    backend.fail("terminate_process", {
      code: "PROCESS_PERMISSION_DENIED",
      message: "Operation not permitted for PID 101",
    });
    const { user } = await setup(runningProject(), [], "/projects/1", true);
    await user.click(screen.getByRole("button", { name: "Processes (3)" }));
    await user.click(screen.getByRole("button", { name: "Actions for vite (PID 101)" }));
    await user.click(screen.getByRole("menuitem", { name: "Terminate" }));
    expect(await screen.findByText("Permission denied")).toBeInTheDocument();
    expect(screen.getByText(/Operation not permitted for PID 101/)).toBeInTheDocument();
  });
});
