import { beforeAll, describe, expect, test } from "bun:test";
import { screen, waitFor, within } from "@testing-library/react";
import { Route, Routes } from "react-router";
import { backend } from "@/test/backend";
import { makeCommand, makeProject, makeRunningProject, makeSnapshot } from "@/test/fixtures";
import { renderWithProviders } from "@/test/render";
import type { Project } from "@/types";
import { ProjectsPage } from "./ProjectsPage";

beforeAll(() => {
  const proto = Element.prototype as unknown as Record<string, unknown>;
  proto.hasPointerCapture ??= () => false;
  proto.setPointerCapture ??= () => {};
  proto.releasePointerCapture ??= () => {};
  proto.scrollIntoView ??= () => {};
});

function renderRoutes(projects: Project[]) {
  backend.reply("list_editors", []);
  return renderWithProviders(
    <Routes>
      <Route path="/" element={<ProjectsPage />} />
      <Route path="projects/:projectId" element={<p>Detail of project</p>} />
    </Routes>,
    { snapshot: makeSnapshot({ projects }), toaster: true },
  );
}

describe("Add project", () => {
  test("the header button opens the folder picker and navigates to the new project", async () => {
    backend.reply("pick_and_add_project", 42);
    const { user } = renderRoutes([makeRunningProject({ id: 1, name: "Alpha" })]);
    expect(screen.getAllByRole("button", { name: "Add project" })).toHaveLength(1);
    await user.click(screen.getByRole("button", { name: "Add project" }));
    expect(await screen.findByText("Detail of project")).toBeInTheDocument();
    expect(backend.callsTo("pick_and_add_project")).toEqual([undefined]);
  });

  test("cancelling the picker stays on the dashboard without an error", async () => {
    backend.reply("pick_and_add_project", null);
    const { user } = renderRoutes([makeRunningProject({ id: 1, name: "Alpha" })]);
    await user.click(screen.getByRole("button", { name: "Add project" }));
    await waitFor(() => expect(backend.callsTo("pick_and_add_project")).toHaveLength(1));
    expect(screen.queryByText("Detail of project")).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1, name: /Projects/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add project" })).toBeEnabled();
    expect(screen.queryByText(/Add project:/)).not.toBeInTheDocument();
  });

  test("the empty state offers it too, and a failure is reported", async () => {
    backend.fail("pick_and_add_project", { code: "PATH_NOT_FOUND", message: "The folder no longer exists" });
    const { user } = renderRoutes([]);
    const buttons = screen.getAllByRole("button", { name: "Add project" });
    expect(buttons).toHaveLength(2);
    await user.click(buttons[1]);
    expect(await screen.findByText(/The folder no longer exists/)).toBeInTheDocument();
    expect(screen.queryByText("Detail of project")).not.toBeInTheDocument();
  });
});

describe("card menu", () => {
  test("a stopped project's menu starts a launch profile; running ones offer restart", async () => {
    backend.reply("start_project", undefined);
    backend.reply("restart_project", undefined);
    const profile = {
      id: "p1",
      name: "Web + API",
      commands: [makeCommand({ script: "pkg:dev" })],
    };
    const { user } = renderRoutes([
      makeProject({ id: 2, name: "Bravo", primaryCommand: makeCommand(), launchProfiles: [profile] }),
      makeRunningProject({ id: 3, name: "Gamma", launchProfiles: [profile] }),
    ]);

    await user.click(screen.getByRole("button", { name: "More actions for Bravo" }));
    expect(screen.queryByRole("menuitem", { name: /Restart “Web/ })).not.toBeInTheDocument();
    await user.click(screen.getByRole("menuitem", { name: /Start “Web \+ API”/ }));
    await waitFor(() => expect(backend.callsTo("start_project")).toEqual([{ projectId: 2, profileId: "p1", port: null }]));

    await user.click(within(screen.getByRole("article", { name: "Gamma" })).getByRole("button", { name: "More actions for Gamma" }));
    await user.click(screen.getByRole("menuitem", { name: /Restart “Web \+ API”/ }));
    await waitFor(() => expect(backend.callsTo("restart_project")).toEqual([{ projectId: 3, profileId: "p1", port: null }]));
  });
});
