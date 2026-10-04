import { afterEach, describe, expect, spyOn, test } from "bun:test";
import { toast } from "sonner";
import { backend } from "@/test/backend";
import { makeCommand, makeProject, makeRunningProject } from "@/test/fixtures";
import type { Project } from "@/types";
import { getProjectActions, type ProjectAction, type ProjectActionId } from "./project-actions";

function actionsById(project: Project, options?: Parameters<typeof getProjectActions>[1]) {
  const byId = new Map<ProjectActionId, ProjectAction>();
  for (const action of getProjectActions(project, options)) byId.set(action.id, action);
  return byId;
}

function disabledIds(project: Project, options?: Parameters<typeof getProjectActions>[1]): ProjectActionId[] {
  return getProjectActions(project, options)
    .filter((action) => action.disabled)
    .map((action) => action.id);
}

describe("getProjectActions enablement", () => {
  test("a stopped project with a start command can only start (and be forgotten)", () => {
    const project = makeProject({ runtimeStatus: "stopped", primaryCommand: makeCommand() });
    expect(disabledIds(project)).toEqual(["open-url", "copy-url", "restart", "stop"]);
    expect(actionsById(project).has("force-stop")).toBe(false);
  });

  test("a running project can restart and stop but not start or be forgotten", () => {
    const project = makeRunningProject();
    expect(disabledIds(project)).toEqual(["start", "forget"]);
    const actions = actionsById(project);
    expect(actions.get("start")?.disabledReason).toBe("Already running");
    expect(actions.get("forget")?.disabledReason).toBe("Stop the project first");
  });

  test("partial behaves like running", () => {
    expect(disabledIds(makeRunningProject({ runtimeStatus: "partial" }))).toEqual(["start", "forget"]);
  });

  test("without a primary command, start and restart explain why they're disabled", () => {
    const stopped = actionsById(makeProject({ primaryCommand: null }));
    expect(stopped.get("start")).toMatchObject({ disabled: true, disabledReason: "No start command configured" });
    const running = actionsById(makeRunningProject({ primaryCommand: null }));
    expect(running.get("restart")).toMatchObject({ disabled: true, disabledReason: "No start command configured" });
    expect(running.get("stop")?.disabled).toBeFalsy();
  });

  test("open/copy URL need a primary URL", () => {
    const without = actionsById(makeRunningProject({ primaryUrl: null }));
    expect(without.get("open-url")).toMatchObject({ disabled: true, disabledReason: "No URL detected yet" });
    expect(without.get("copy-url")?.disabled).toBe(true);
    const withUrl = actionsById(makeRunningProject());
    expect(withUrl.get("open-url")?.disabled).toBeFalsy();
    expect(withUrl.get("copy-url")?.disabled).toBeFalsy();
  });

  test("folder, terminal and refresh are always available", () => {
    const actions = actionsById(makeProject());
    for (const id of ["open-folder", "open-terminal", "refresh"] as const) {
      expect(actions.get(id)?.disabled).toBeFalsy();
    }
  });
});

describe("Force Stop", () => {
  test("is only offered while a graceful stop is pending, as a confirmed destructive action", () => {
    expect(actionsById(makeRunningProject()).has("force-stop")).toBe(false);
    const pending = actionsById(makeRunningProject({ stopPending: true })).get("force-stop");
    expect(pending).toBeDefined();
    expect(pending?.destructive).toBe(true);
    expect(pending?.disabled).toBeFalsy();
    expect(pending?.confirm?.confirmLabel).toBe("Force Stop");
  });
});

describe("labels and toggles", () => {
  test("pin / ignore labels flip with the current state", () => {
    expect(actionsById(makeProject({ favorite: false })).get("favorite")?.label).toBe("Pin");
    expect(actionsById(makeProject({ favorite: true })).get("favorite")?.label).toBe("Unpin");
    expect(actionsById(makeProject({ ignored: false })).get("ignore")?.label).toBe("Ignore");
    expect(actionsById(makeProject({ ignored: true })).get("ignore")?.label).toBe("Stop ignoring");
  });

  test("forget is destructive and requires confirmation", () => {
    const forget = actionsById(makeProject()).get("forget");
    expect(forget?.destructive).toBe(true);
    expect(forget?.confirm).toBeDefined();
  });

  test("the editor label names the project's editor, falling back to the default", () => {
    const editors = [
      { id: "vscode", name: "VS Code", available: true },
      { id: "cursor", name: "Cursor", available: true },
      { id: "zed", name: "Zed", available: false },
    ];
    const project = makeProject();
    expect(actionsById(project).get("open-editor")?.label).toBe("Open in Editor");
    expect(actionsById(project, { editors, defaultEditorId: "vscode" }).get("open-editor")?.label).toBe("Open in VS Code");
    const override = makeProject({ overrides: { displayName: null, primaryUrl: null, preferredEditor: "cursor" } });
    expect(actionsById(override, { editors, defaultEditorId: "vscode" }).get("open-editor")?.label).toBe("Open in Cursor");
  });

  test("an editor that is known to be missing disables the action with a reason", () => {
    const editors = [{ id: "zed", name: "Zed", available: false }];
    const action = actionsById(makeProject(), { editors, defaultEditorId: "zed" }).get("open-editor");
    expect(action).toMatchObject({ disabled: true, disabledReason: "Zed isn't installed" });
  });
});

describe("run()", () => {
  afterEach(() => {
    spyOn(toast, "error").mockRestore();
    spyOn(toast, "success").mockRestore();
  });

  test("a backend failure becomes a code-specific toast and run() still resolves", async () => {
    const error = spyOn(toast, "error").mockImplementation(() => "id");
    backend.fail("open_project_folder", {
      code: "PATH_NOT_FOUND",
      message: "Path does not exist: /home/dev/projects/gone",
    });
    const open = actionsById(makeProject({ id: 7 })).get("open-folder");
    await open?.run();
    expect(error).toHaveBeenCalledTimes(1);
    const [title, options] = error.mock.calls[0] as [string, { description: string }];
    expect(title).toBe("Path not found");
    expect(options.description).toContain("Path does not exist: /home/dev/projects/gone");
    expect(backend.callsTo("open_project_folder")).toEqual([{ projectId: 7 }]);
  });

  test("stop only signals: a partially failed report is surfaced as a warning, not success", async () => {
    const success = spyOn(toast, "success").mockImplementation(() => "id");
    const warning = spyOn(toast, "warning").mockImplementation(() => "id");
    backend.reply("stop_project", {
      signaled: [10],
      failed: [{ pid: 11, error: { code: "PROCESS_PERMISSION_DENIED", message: "Not permitted to signal process 11" } }],
    });
    await actionsById(makeRunningProject({ id: 2 })).get("stop")?.run();
    expect(success).not.toHaveBeenCalled();
    expect(warning).toHaveBeenCalledTimes(1);
    warning.mockRestore();
  });
});
