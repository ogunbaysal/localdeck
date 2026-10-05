import { beforeAll, describe, expect, test } from "bun:test";
import { act, screen, waitFor, within } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";
import { Route, Routes } from "react-router";
import { backend } from "@/test/backend";
import { makeCommand, makeProject, makeRunningProject, makeSnapshot } from "@/test/fixtures";
import { renderWithProviders } from "@/test/render";
import type { EditorInfo, Project, ProjectScripts, StartOption, TerminalInfo } from "@/types";
import { ProjectDetailPage } from "./ProjectDetailPage";
import { looseUrlProblem } from "./project-settings-dialog";

beforeAll(() => {
  const proto = Element.prototype as unknown as Record<string, unknown>;
  proto.hasPointerCapture ??= () => false;
  proto.setPointerCapture ??= () => {};
  proto.releasePointerCapture ??= () => {};
  proto.scrollIntoView ??= () => {};
});

const editors: EditorInfo[] = [
  { id: "zed", name: "Zed", available: true },
  { id: "vscode", name: "VS Code", available: false },
];

const terminals: TerminalInfo[] = [
  { id: "terminal", name: "Terminal", available: true },
  { id: "warp", name: "Warp", available: false },
];

const options: StartOption[] = [
  { id: "pkg:dev", source: "packageJson", name: "dev", label: "bun run dev", detail: "vite --port 5173" },
  { id: "pkg:build", source: "packageJson", name: "build", label: "bun run build", detail: "tsc && vite build" },
  { id: "make:serve", source: "makefile", name: "serve", label: "make serve", detail: null },
  { id: "compose:up", source: "compose", name: "up", label: "docker compose up", detail: null },
];

const scripts: ProjectScripts = {
  packageManager: "bun",
  packageManagerDetected: true,
  options,
  primaryOption: null,
};

async function setup(project: Project, projectScripts: ProjectScripts = scripts) {
  backend.reply("list_editors", editors);
  backend.reply("list_terminals", terminals);
  backend.reply("list_project_scripts", projectScripts);
  const view = renderWithProviders(
    <Routes>
      <Route path="projects/:projectId" element={<ProjectDetailPage />} />
    </Routes>,
    { route: `/projects/${project.id}`, snapshot: makeSnapshot({ projects: [project] }) },
  );
  await act(async () => {});
  return view;
}

async function openSettings(user: UserEvent) {
  await user.click(screen.getByRole("button", { name: "More actions" }));
  await user.click(screen.getByRole("menuitem", { name: "Project settings…" }));
  return screen.findByRole("dialog", { name: "Project settings" });
}

describe("looseUrlProblem", () => {
  test("accepts blanks, http(s) URLs and host:port; rejects the obviously wrong", () => {
    for (const ok of ["", "  ", "http://localhost:3000", "https://app.test/x?y=1", "localhost:3000", "127.0.0.1:8080/app"]) {
      expect(looseUrlProblem(ok)).toBeNull();
    }
    expect(looseUrlProblem("ftp://host")).not.toBeNull();
    expect(looseUrlProblem("http://")).not.toBeNull();
    expect(looseUrlProblem("http://local host")).not.toBeNull();
    expect(looseUrlProblem("not a url")).not.toBeNull();
  });
});

describe("project settings dialog", () => {
  test("is prefilled from the overrides and saves every field", async () => {
    backend.reply("set_project_override", undefined);
    const project = makeRunningProject({
      id: 7,
      name: "Renamed",
      detectedName: "localdeck",
      overrides: { displayName: "Renamed", primaryUrl: null, preferredEditor: null, preferredTerminal: null, autoRestart: false },
    });
    const { user } = await setup(project);
    const dialog = within(await openSettings(user));

    const name = dialog.getByLabelText("Display name");
    expect(name).toHaveValue("Renamed");
    expect(name).toHaveAttribute("placeholder", "localdeck");

    await user.clear(name);
    await user.type(name, "My app");
    await user.type(dialog.getByLabelText("Primary URL override"), "localhost:4000");
    await user.click(dialog.getByRole("combobox", { name: "Preferred editor" }));
    expect(await screen.findByRole("option", { name: /VS Code.*not installed/ })).toBeInTheDocument();
    await user.click(screen.getByRole("option", { name: "Zed" }));
    await user.click(dialog.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(backend.callsTo("set_project_override")).toEqual([
        {
          projectId: 7,
          overrides: {
            displayName: "My app",
            primaryUrl: "localhost:4000",
            preferredEditor: "zed",
            preferredTerminal: null,
            autoRestart: false,
          },
        },
      ]),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  test("blank fields clear the overrides (null)", async () => {
    backend.reply("set_project_override", undefined);
    const project = makeRunningProject({
      id: 7,
      overrides: {
        displayName: "Old",
        primaryUrl: "http://localhost:1",
        preferredEditor: "zed",
        preferredTerminal: "terminal",
        autoRestart: true,
      },
    });
    const { user } = await setup(project);
    const dialog = within(await openSettings(user));
    await user.clear(dialog.getByLabelText("Display name"));
    await user.clear(dialog.getByLabelText("Primary URL override"));
    await user.click(dialog.getByRole("combobox", { name: "Preferred editor" }));
    await user.click(await screen.findByRole("option", { name: "Use global default" }));
    await user.click(dialog.getByRole("combobox", { name: "Terminal" }));
    await user.click(await screen.findByRole("option", { name: "Use global default" }));
    await user.click(dialog.getByRole("switch", { name: "Auto-restart" }));
    await user.click(dialog.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(backend.callsTo("set_project_override")).toEqual([
        {
          projectId: 7,
          overrides: {
            displayName: null,
            primaryUrl: null,
            preferredEditor: null,
            preferredTerminal: null,
            autoRestart: false,
          },
        },
      ]),
    );
  });

  test("the terminal and auto-restart choices are saved in the same override object", async () => {
    backend.reply("set_project_override", undefined);
    const { user } = await setup(makeRunningProject({ id: 7 }));
    const dialog = within(await openSettings(user));

    await user.click(dialog.getByRole("combobox", { name: "Terminal" }));
    expect(await screen.findByRole("option", { name: /Warp.*not installed/ })).toBeInTheDocument();
    await user.click(screen.getByRole("option", { name: "Terminal" }));
    const restart = dialog.getByRole("switch", { name: "Auto-restart" });
    expect(restart).not.toBeChecked();
    expect(dialog.getByText(/up to 5 attempts with increasing delay/)).toBeInTheDocument();
    await user.click(restart);
    await user.click(dialog.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(backend.callsTo("set_project_override")).toEqual([
        {
          projectId: 7,
          overrides: {
            displayName: null,
            primaryUrl: null,
            preferredEditor: null,
            preferredTerminal: "terminal",
            autoRestart: true,
          },
        },
      ]),
    );
  });

  test("an obviously invalid URL is rejected locally without calling the backend", async () => {
    backend.reply("set_project_override", undefined);
    const { user } = await setup(makeRunningProject({ id: 7 }));
    const dialog = within(await openSettings(user));
    await user.type(dialog.getByLabelText("Primary URL override"), "ftp://nope");
    await user.click(dialog.getByRole("button", { name: "Save" }));
    expect(dialog.getByText("Use an http:// or https:// URL.")).toBeInTheDocument();
    expect(backend.callsTo("set_project_override")).toHaveLength(0);
  });

  test("a structured backend rejection is shown inside the dialog, which stays open", async () => {
    backend.fail("set_project_override", {
      code: "INVALID_INPUT",
      message: "Only http and https URLs with a host can be opened: http://:99",
    });
    const { user } = await setup(makeRunningProject({ id: 7 }));
    const dialog = within(await openSettings(user));
    await user.type(dialog.getByLabelText("Primary URL override"), "http://:99");
    await user.click(dialog.getByRole("button", { name: "Save" }));
    expect(await dialog.findByText("Invalid input")).toBeInTheDocument();
    expect(dialog.getByText(/Only http and https URLs with a host/)).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Project settings" })).toBeInTheDocument();
  });

  test("Cancel closes without saving", async () => {
    const { user } = await setup(makeRunningProject({ id: 7 }));
    const dialog = within(await openSettings(user));
    await user.type(dialog.getByLabelText("Display name"), "x");
    await user.click(dialog.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(backend.callsTo("set_project_override")).toHaveLength(0);
  });

  test("Refresh detection re-runs detection for the project", async () => {
    backend.reply("refresh_project", undefined);
    const { user } = await setup(makeRunningProject({ id: 7 }));
    const dialog = within(await openSettings(user));
    await user.click(dialog.getByRole("button", { name: "Refresh detection" }));
    await waitFor(() => expect(backend.callsTo("refresh_project")).toEqual([{ projectId: 7 }]));
  });
});

describe("primary start command", () => {
  test("lists options grouped by source; choosing and clearing call set_primary_command with option ids", async () => {
    backend.reply("set_primary_command", undefined);
    const project = makeRunningProject({
      id: 7,
      primaryCommand: makeCommand({ label: "bun run dev", script: "pkg:dev" }),
    });
    const { user } = await setup(project, { ...scripts, primaryOption: "pkg:dev" });
    const dialog = within(await openSettings(user));

    expect(await dialog.findByRole("radio", { name: /bun run dev/ })).toBeChecked();
    for (const heading of ["Package scripts", "Makefile", "Docker Compose"]) {
      expect(dialog.getByRole("radiogroup", { name: heading })).toBeInTheDocument();
    }
    expect(dialog.queryByRole("radiogroup", { name: "Cargo" })).not.toBeInTheDocument();
    expect(within(dialog.getByRole("radiogroup", { name: "Makefile" })).getByText("make serve")).toBeInTheDocument();
    expect(dialog.getByText("tsc && vite build")).toBeInTheDocument();

    // The backend reports the new primary option on the reload that follows the change.
    backend.reply("list_project_scripts", { ...scripts, primaryOption: "make:serve" });
    await user.click(dialog.getByRole("radio", { name: /make serve/ }));
    await waitFor(() => expect(backend.callsTo("set_primary_command")).toEqual([{ projectId: 7, option: "make:serve" }]));
    await waitFor(() => expect(backend.callsTo("list_project_scripts")).toHaveLength(2));
    expect(await dialog.findByRole("radio", { name: /make serve/ })).toBeChecked();
    await user.click(dialog.getByRole("button", { name: "Clear" }));
    await waitFor(() =>
      expect(backend.callsTo("set_primary_command")).toEqual([
        { projectId: 7, option: "make:serve" },
        { projectId: 7, option: null },
      ]),
    );
  });

  test("says when the package manager was only assumed", async () => {
    const { user } = await setup(makeProject({ id: 7 }), { ...scripts, packageManagerDetected: false });
    const dialog = within(await openSettings(user));
    expect(await dialog.findByText(/assumed/)).toBeInTheDocument();
  });

  test("explains which files are read when there are no options", async () => {
    const { user } = await setup(makeProject({ id: 7 }), { ...scripts, options: [] });
    const dialog = within(await openSettings(user));
    const hint = await dialog.findByText(/LocalDeck reads package\.json scripts, Makefile targets/);
    expect(hint).toHaveTextContent(/justfile.*composer\.json.*Cargo\.toml.*docker-compose.*pyproject\.toml/);
    expect(dialog.queryByRole("radio")).not.toBeInTheDocument();
  });

  test("a failed script listing shows the backend error with a retry", async () => {
    backend.reply("list_editors", editors);
    backend.reply("list_terminals", terminals);
    backend.fail("list_project_scripts", { code: "PATH_NOT_FOUND", message: "package.json is missing" });
    const project = makeProject({ id: 7 });
    const { user } = renderWithProviders(
      <Routes>
        <Route path="projects/:projectId" element={<ProjectDetailPage />} />
      </Routes>,
      { route: "/projects/7", snapshot: makeSnapshot({ projects: [project] }) },
    );
    await act(async () => {});
    const dialog = within(await openSettings(user));
    expect(await dialog.findByText("Path not found")).toBeInTheDocument();
    expect(dialog.getByText("package.json is missing")).toBeInTheDocument();
    expect(dialog.getByRole("button", { name: "Try again" })).toBeInTheDocument();
  });

  test("the overview's Choose… link opens the settings when no command is set", async () => {
    const { user } = await setup(makeProject({ id: 7 }));
    const overview = within(screen.getByRole("region", { name: "Overview" }));
    expect(overview.getByText("Not set")).toBeInTheDocument();
    await user.click(overview.getByRole("button", { name: "Choose…" }));
    expect(await screen.findByRole("dialog", { name: "Project settings" })).toBeInTheDocument();
  });
});
