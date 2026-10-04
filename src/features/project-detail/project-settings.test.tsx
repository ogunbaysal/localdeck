import { beforeAll, describe, expect, test } from "bun:test";
import { act, screen, waitFor, within } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";
import { Route, Routes } from "react-router";
import { backend } from "@/test/backend";
import { makeCommand, makeProject, makeRunningProject, makeSnapshot } from "@/test/fixtures";
import { renderWithProviders } from "@/test/render";
import type { EditorInfo, Project, ProjectScripts } from "@/types";
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

const scripts: ProjectScripts = {
  packageManager: "bun",
  packageManagerDetected: true,
  scripts: [
    { name: "dev", command: "vite --port 5173" },
    { name: "build", command: "tsc && vite build" },
  ],
  primaryScript: null,
};

async function setup(project: Project, projectScripts: ProjectScripts = scripts) {
  backend.reply("list_editors", editors);
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
  test("is prefilled from the overrides and saves all three fields", async () => {
    backend.reply("set_project_override", undefined);
    const project = makeRunningProject({
      id: 7,
      name: "Renamed",
      detectedName: "localdeck",
      overrides: { displayName: "Renamed", primaryUrl: null, preferredEditor: null },
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
          overrides: { displayName: "My app", primaryUrl: "localhost:4000", preferredEditor: "zed" },
        },
      ]),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  test("blank fields clear the overrides (null)", async () => {
    backend.reply("set_project_override", undefined);
    const project = makeRunningProject({
      id: 7,
      overrides: { displayName: "Old", primaryUrl: "http://localhost:1", preferredEditor: "zed" },
    });
    const { user } = await setup(project);
    const dialog = within(await openSettings(user));
    await user.clear(dialog.getByLabelText("Display name"));
    await user.clear(dialog.getByLabelText("Primary URL override"));
    await user.click(dialog.getByRole("combobox", { name: "Preferred editor" }));
    await user.click(await screen.findByRole("option", { name: "Use global default" }));
    await user.click(dialog.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(backend.callsTo("set_project_override")).toEqual([
        { projectId: 7, overrides: { displayName: null, primaryUrl: null, preferredEditor: null } },
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
  test("choosing a script and clearing it call set_primary_command", async () => {
    backend.reply("set_primary_command", undefined);
    const project = makeRunningProject({ id: 7, primaryCommand: makeCommand({ label: "bun run dev", script: "dev" }) });
    const { user } = await setup(project, { ...scripts, primaryScript: "dev" });
    const dialog = within(await openSettings(user));

    expect(await dialog.findByRole("radio", { name: /dev/ })).toBeChecked();
    expect(dialog.getByText("bun run dev")).toBeInTheDocument();
    expect(dialog.getByText("tsc && vite build")).toBeInTheDocument();

    // The backend reports the new primary script on the reload that follows the change.
    backend.reply("list_project_scripts", { ...scripts, primaryScript: "build" });
    await user.click(dialog.getByRole("radio", { name: /build/ }));
    await waitFor(() => expect(backend.callsTo("set_primary_command")).toEqual([{ projectId: 7, script: "build" }]));
    await waitFor(() => expect(backend.callsTo("list_project_scripts")).toHaveLength(2));
    expect(await dialog.findByRole("radio", { name: /build/ })).toBeChecked();
    await user.click(dialog.getByRole("button", { name: "Clear" }));
    await waitFor(() =>
      expect(backend.callsTo("set_primary_command")).toEqual([
        { projectId: 7, script: "build" },
        { projectId: 7, script: null },
      ]),
    );
  });

  test("says when the package manager was only assumed", async () => {
    const { user } = await setup(makeProject({ id: 7 }), { ...scripts, packageManagerDetected: false });
    const dialog = within(await openSettings(user));
    expect(await dialog.findByText(/assumed/)).toBeInTheDocument();
  });

  test("explains that v0.1 reads package.json scripts when there are none", async () => {
    const { user } = await setup(makeProject({ id: 7 }), { ...scripts, scripts: [] });
    const dialog = within(await openSettings(user));
    expect(await dialog.findByText(/reads start commands from package\.json scripts/)).toBeInTheDocument();
    expect(dialog.queryByRole("radio")).not.toBeInTheDocument();
  });

  test("a failed script listing shows the backend error with a retry", async () => {
    backend.reply("list_editors", editors);
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
