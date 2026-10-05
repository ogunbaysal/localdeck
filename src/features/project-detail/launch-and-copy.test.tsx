import { beforeAll, describe, expect, test } from "bun:test";
import { act, screen, waitFor, within } from "@testing-library/react";
import { Route, Routes } from "react-router";
import { backend } from "@/test/backend";
import {
  makeAttention,
  makeCommand,
  makePort,
  makeProcess,
  makeProject,
  makeRunningProject,
  makeSnapshot,
} from "@/test/fixtures";
import { renderWithProviders } from "@/test/render";
import type { LaunchProfile, PortInfo, Project, ProjectScripts, StartOption } from "@/types";
import { ProjectDetailPage } from "./ProjectDetailPage";

beforeAll(() => {
  const proto = Element.prototype as unknown as Record<string, unknown>;
  proto.hasPointerCapture ??= () => false;
  proto.setPointerCapture ??= () => {};
  proto.releasePointerCapture ??= () => {};
  proto.scrollIntoView ??= () => {};
});

const options: StartOption[] = [
  { id: "pkg:dev", source: "packageJson", name: "dev", label: "bun run dev", detail: "vite" },
  { id: "pkg:api", source: "packageJson", name: "api", label: "bun run api", detail: "node server.js" },
  { id: "make:worker", source: "makefile", name: "worker", label: "make worker", detail: null },
];
const scripts: ProjectScripts = { packageManager: "bun", packageManagerDetected: true, options, primaryOption: "pkg:dev" };

const webApi: LaunchProfile = {
  id: "p-web-api",
  name: "Web + API",
  commands: [
    makeCommand({ label: "bun run dev", script: "pkg:dev" }),
    makeCommand({ label: "bun run api", args: ["run", "api"], script: "pkg:api" }),
  ],
};
const worker: LaunchProfile = {
  id: "p-worker",
  name: "Worker",
  commands: [makeCommand({ program: "make", args: ["worker"], label: "make worker", script: "make:worker" })],
};

async function setup(project: Project, ports: PortInfo[] = [], others: Project[] = []) {
  backend.reply("list_editors", []);
  backend.reply("list_terminals", []);
  backend.reply("list_project_scripts", scripts);
  const view = renderWithProviders(
    <Routes>
      <Route path="projects/:projectId" element={<ProjectDetailPage />} />
    </Routes>,
    { route: `/projects/${project.id}`, snapshot: makeSnapshot({ projects: [project, ...others], ports }) },
  );
  await act(async () => {});
  return view;
}

const stoppedWithProfiles = (overrides: Partial<Project> = {}) =>
  makeProject({
    id: 1,
    name: "LocalDeck",
    primaryCommand: makeCommand({ script: "pkg:dev" }),
    launchProfiles: [webApi, worker],
    ...overrides,
  });

describe("launch profile start/restart menus", () => {
  test("the Start menu starts the primary command or a profile", async () => {
    backend.reply("start_project", undefined);
    const { user } = await setup(stoppedWithProfiles());

    await user.click(screen.getByRole("button", { name: "Start options" }));
    await user.click(screen.getByRole("menuitem", { name: /Start “Web \+ API”$/ }));
    await waitFor(() => expect(backend.callsTo("start_project")).toEqual([{ projectId: 1, profileId: "p-web-api", port: null }]));

    await user.click(screen.getByRole("button", { name: "Start options" }));
    await user.click(screen.getByRole("menuitem", { name: /Primary command/ }));
    await waitFor(() => expect(backend.callsTo("start_project")).toHaveLength(2));
    expect(backend.callsTo("start_project")[1]).toEqual({ projectId: 1, profileId: null, port: null });
  });

  test("the plain Start button still starts the primary command", async () => {
    backend.reply("start_project", undefined);
    const { user } = await setup(stoppedWithProfiles());
    await user.click(screen.getByRole("button", { name: "Start" }));
    await waitFor(() => expect(backend.callsTo("start_project")).toEqual([{ projectId: 1, profileId: null, port: null }]));
  });

  test("without profiles there is no split menu", async () => {
    await setup(makeProject({ id: 1, primaryCommand: makeCommand() }));
    expect(screen.queryByRole("button", { name: "Start options" })).not.toBeInTheDocument();
  });

  test("a running project restarts a chosen profile", async () => {
    backend.reply("restart_project", undefined);
    const { user } = await setup(makeRunningProject({ id: 1, name: "LocalDeck", launchProfiles: [webApi, worker] }));
    await user.click(screen.getByRole("button", { name: "Restart options" }));
    await user.click(screen.getByRole("menuitem", { name: /Restart “Worker”$/ }));
    await waitFor(() => expect(backend.callsTo("restart_project")).toEqual([{ projectId: 1, profileId: "p-worker", port: null }]));
  });

  test("a profile without commands can't be started", async () => {
    const { user } = await setup(stoppedWithProfiles({ launchProfiles: [{ id: "empty", name: "Empty", commands: [] }] }));
    await user.click(screen.getByRole("button", { name: "Start options" }));
    expect(screen.getByRole("menuitem", { name: /Start “Empty”$/ })).toHaveAttribute("aria-disabled", "true");
  });
});

describe("Start on a free port", () => {
  test("prefills a suggestion for the default port, warns about a held port and starts with the chosen one", async () => {
    backend.reply("start_project", undefined);
    backend.handle("suggest_free_port", (args) => (args?.preferred === 3000 ? 3001 : 3003));
    const holder = makeProject({ id: 2, name: "Other app" });
    const taken = makePort({
      port: 3002,
      ownership: "assigned",
      projectId: 2,
      processes: [{ pid: 777, name: "ruby", startedAt: 1 }],
    });
    const { user } = await setup(makeProject({ id: 1, name: "LocalDeck", primaryCommand: makeCommand() }), [taken], [holder]);

    await user.click(screen.getByRole("button", { name: "More actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Start on a free port…" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Start on a free port" }));

    const field = dialog.getByLabelText("Port");
    await waitFor(() => expect(field).toHaveValue("3001"));
    expect(backend.callsTo("suggest_free_port")).toEqual([{ preferred: 3000 }]);
    expect(dialog.queryByText(/already in use/)).not.toBeInTheDocument();

    await user.clear(field);
    await user.type(field, "3002");
    expect(await dialog.findByText(/Port 3002 is already in use by ruby \(PID 777\) of Other app/)).toBeInTheDocument();

    await user.click(dialog.getByRole("button", { name: "Find next free" }));
    await waitFor(() => expect(field).toHaveValue("3003"));
    expect(backend.callsTo("suggest_free_port")[1]).toEqual({ preferred: 3002 });
    expect(dialog.queryByText(/already in use/)).not.toBeInTheDocument();

    await user.click(dialog.getByRole("button", { name: "Start" }));
    await waitFor(() => expect(backend.callsTo("start_project")).toEqual([{ projectId: 1, profileId: null, port: 3003 }]));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  test("the preferred port is the project's own, and an invalid port can't be submitted", async () => {
    backend.reply("suggest_free_port", 5174);
    const { user } = await setup(makeProject({ id: 1, primaryCommand: makeCommand(), ports: [5173] }));
    await user.click(screen.getByRole("button", { name: "More actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Start on a free port…" }));
    const dialog = within(await screen.findByRole("dialog"));
    expect(backend.callsTo("suggest_free_port")).toEqual([{ preferred: 5173 }]);
    const field = dialog.getByLabelText("Port");
    await waitFor(() => expect(field).toHaveValue("5174"));
    await user.clear(field);
    await user.type(field, "70000");
    expect(dialog.getByRole("button", { name: "Start" })).toBeDisabled();
    expect(dialog.getByText("Enter a port between 1 and 65535.")).toBeInTheDocument();
  });

  test("a running project restarts a profile on the chosen port; its own port isn't a conflict", async () => {
    backend.reply("restart_project", undefined);
    backend.reply("suggest_free_port", 3000);
    const own = makePort({ port: 3000, ownership: "assigned", projectId: 1 });
    const { user } = await setup(
      makeRunningProject({
        id: 1,
        name: "LocalDeck",
        primaryCommand: makeCommand(),
        ports: [3000],
        launchProfiles: [webApi],
      }),
      [own],
    );
    await user.click(screen.getByRole("button", { name: "Restart options" }));
    await user.click(screen.getByRole("menuitem", { name: /Restart “Web \+ API” on a free port…/ }));
    const dialog = within(await screen.findByRole("dialog", { name: "Restart on a free port" }));
    await waitFor(() => expect(dialog.getByLabelText("Port")).toHaveValue("3000"));
    expect(dialog.queryByText(/already in use/)).not.toBeInTheDocument();
    await user.click(dialog.getByRole("button", { name: "Restart" }));
    await waitFor(() =>
      expect(backend.callsTo("restart_project")).toEqual([{ projectId: 1, profileId: "p-web-api", port: 3000 }]),
    );
  });
});

describe("auto-restart visibility", () => {
  test("the header says when auto-restart is on", async () => {
    const overrides = { displayName: null, primaryUrl: null, preferredEditor: null, preferredTerminal: null };
    await setup(makeProject({ id: 1, overrides: { ...overrides, autoRestart: true } }));
    expect(within(screen.getByRole("banner")).getByText("Auto-restart on")).toBeInTheDocument();
  });

  test("a gave-up attention item is shown and can be dismissed", async () => {
    backend.reply("dismiss_attention", undefined);
    const item = makeAttention({
      id: "auto-restart-gave-up",
      kind: "autoRestartGaveUp",
      message: "Auto-restart gave up after 5 attempts",
      dismissible: true,
    });
    const { user } = await setup(makeProject({ id: 1, attention: [item] }));
    expect(screen.getByText("Auto-restart gave up after 5 attempts")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    await waitFor(() => expect(backend.callsTo("dismiss_attention")).toEqual([{ projectId: 1, attentionId: "auto-restart-gave-up" }]));
  });
});

describe("copy helpers", () => {
  const spaced = "/home/dev/my projects/it's here";

  test("the project menu copies the path, a quoted cd command and the start command", async () => {
    const project = makeProject({
      id: 1,
      rootPath: spaced,
      primaryCommand: makeCommand({ program: "bun", args: ["run", "my script"], cwd: spaced }),
    });
    const { user } = await setup(project);
    const copied = async (name: string) => {
      await user.click(screen.getByRole("button", { name: "More actions" }));
      await user.click(screen.getByRole("menuitem", { name }));
      return navigator.clipboard.readText();
    };
    expect(await copied("Copy path")).toBe(spaced);
    expect(await copied("Copy cd command")).toBe(`cd '/home/dev/my projects/it'\\''s here'`);
    expect(await copied("Copy start command")).toBe("bun run 'my script'");
  });

  test("Copy start command is disabled without a primary command", async () => {
    const { user } = await setup(makeProject({ id: 1 }));
    await user.click(screen.getByRole("button", { name: "More actions" }));
    expect(screen.getByRole("menuitem", { name: /Copy start command/ })).toHaveAttribute("aria-disabled", "true");
  });

  test("the process menu copies the PID and the TERM / KILL commands without running anything", async () => {
    const proc = makeProcess({ pid: 4321, name: "vite", componentId: null });
    const { user } = await setup(makeRunningProject({ id: 1, processes: [proc], ports: [] }));
    await user.click(screen.getByRole("button", { name: "Processes (1)" }));
    const copied = async (name: string) => {
      await user.click(screen.getByRole("button", { name: "Actions for vite (PID 4321)" }));
      await user.click(screen.getByRole("menuitem", { name }));
      return navigator.clipboard.readText();
    };
    expect(await copied("Copy PID")).toBe("4321");
    expect(await copied("Copy kill command (SIGTERM)")).toBe("kill -TERM 4321");
    expect(await copied("Copy force-kill command (SIGKILL)")).toBe("kill -KILL 4321");
    expect(backend.callsTo("terminate_process")).toHaveLength(0);
    expect(backend.callsTo("force_kill_process")).toHaveLength(0);
  });
});

describe("launch profile editor", () => {
  const profilesSection = () => within(screen.getByRole("region", { name: "Launch profiles" }));

  test("lists profiles with their commands", async () => {
    await setup(stoppedWithProfiles());
    const section = profilesSection();
    expect(section.getByText("Web + API")).toBeInTheDocument();
    expect(section.getByText("bun run api")).toBeInTheDocument();
    expect(section.getByText("Worker")).toBeInTheDocument();
  });

  test("creating a profile sends every existing profile id plus the new one in selection order", async () => {
    backend.reply("set_launch_profiles", undefined);
    backend.reply("get_snapshot", makeSnapshot());
    const { user } = await setup(stoppedWithProfiles());
    await user.click(profilesSection().getByRole("button", { name: "New profile" }));
    const dialog = within(await screen.findByRole("dialog", { name: "New launch profile" }));

    expect(await dialog.findByRole("group", { name: "Package scripts" })).toBeInTheDocument();
    expect(dialog.getByRole("group", { name: "Makefile" })).toBeInTheDocument();

    await user.type(dialog.getByLabelText("Name"), "  Everything ");
    // Selection order, not list order: make worker first, then the API, then dev.
    await user.click(dialog.getByRole("checkbox", { name: /make worker/ }));
    await user.click(dialog.getByRole("checkbox", { name: /bun run api/ }));
    await user.click(dialog.getByRole("checkbox", { name: /bun run dev/ }));
    await user.click(dialog.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(backend.callsTo("set_launch_profiles")).toEqual([
        {
          projectId: 1,
          profiles: [
            { id: "p-web-api", name: "Web + API", options: ["pkg:dev", "pkg:api"] },
            { id: "p-worker", name: "Worker", options: ["make:worker"] },
            { name: "Everything", options: ["make:worker", "pkg:api", "pkg:dev"] },
          ],
        },
      ]),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  test("renaming keeps the id and the options; a duplicate name is refused locally", async () => {
    backend.reply("set_launch_profiles", undefined);
    backend.reply("get_snapshot", makeSnapshot());
    const { user } = await setup(stoppedWithProfiles());
    await user.click(profilesSection().getByRole("button", { name: "Edit Worker" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Edit launch profile" }));
    const name = dialog.getByLabelText("Name");

    await user.clear(name);
    await user.type(name, "web + api");
    await user.click(dialog.getByRole("button", { name: "Save" }));
    expect(await dialog.findByText("Another profile already uses this name.")).toBeInTheDocument();
    expect(backend.callsTo("set_launch_profiles")).toHaveLength(0);

    await user.clear(name);
    await user.type(name, "Queue worker");
    await user.click(dialog.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(backend.callsTo("set_launch_profiles")).toEqual([
        {
          projectId: 1,
          profiles: [
            { id: "p-web-api", name: "Web + API", options: ["pkg:dev", "pkg:api"] },
            { id: "p-worker", name: "Queue worker", options: ["make:worker"] },
          ],
        },
      ]),
    );
  });

  test("a profile needs a name and at least one command", async () => {
    const { user } = await setup(stoppedWithProfiles({ launchProfiles: [] }));
    await user.click(profilesSection().getByRole("button", { name: "New profile" }));
    const dialog = within(await screen.findByRole("dialog"));
    await dialog.findByRole("checkbox", { name: /bun run dev/ });
    await user.click(dialog.getByRole("button", { name: "Save" }));
    expect(dialog.getByText("Give the profile a name.")).toBeInTheDocument();
    expect(dialog.getByText("Pick at least one command.")).toBeInTheDocument();
    expect(backend.callsTo("set_launch_profiles")).toHaveLength(0);
  });

  test("deleting asks first and then sends the remaining profiles", async () => {
    backend.reply("set_launch_profiles", undefined);
    backend.reply("get_snapshot", makeSnapshot());
    const { user } = await setup(stoppedWithProfiles());
    await user.click(profilesSection().getByRole("button", { name: "Delete Web + API" }));
    expect(backend.callsTo("set_launch_profiles")).toHaveLength(0);
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete" }));
    await waitFor(() =>
      expect(backend.callsTo("set_launch_profiles")).toEqual([
        { projectId: 1, profiles: [{ id: "p-worker", name: "Worker", options: ["make:worker"] }] },
      ]),
    );
  });

  test("a backend rejection stays in the dialog", async () => {
    backend.fail("set_launch_profiles", { code: "INVALID_INPUT", message: "Unknown start option: pkg:gone" });
    const { user } = await setup(stoppedWithProfiles({ launchProfiles: [] }));
    await user.click(profilesSection().getByRole("button", { name: "New profile" }));
    const dialog = within(await screen.findByRole("dialog"));
    await user.type(dialog.getByLabelText("Name"), "X");
    await user.click(await dialog.findByRole("checkbox", { name: /bun run dev/ }));
    await user.click(dialog.getByRole("button", { name: "Save" }));
    expect(await dialog.findByText(/Unknown start option: pkg:gone/)).toBeInTheDocument();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });
});
