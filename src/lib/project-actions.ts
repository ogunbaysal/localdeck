import {
  Code,
  Copy,
  CornerDownRight,
  ExternalLink,
  Eye,
  EyeOff,
  FolderOpen,
  OctagonX,
  Pin,
  PinOff,
  Play,
  RefreshCw,
  RotateCw,
  Square,
  Terminal,
  Trash2,
  type LucideIcon,
} from "lucide-react";
import { announceStopReport, copyToClipboard, runAction } from "./actions";
import { cdCommand, commandLine } from "./shell-quote";
import {
  forceStopProject,
  forgetProject,
  ignoreProject,
  openProjectEditor,
  openProjectFolder,
  openProjectTerminal,
  openProjectUrl,
  refreshProject,
  restartProject,
  setProjectFavorite,
  startProject,
  stopProject,
} from "./tauri";
import type { EditorInfo, Project } from "@/types";

export type ProjectActionId =
  | "open-url"
  | "copy-url"
  | "open-folder"
  | "open-editor"
  | "open-terminal"
  | "copy-path"
  | "copy-cd"
  | "copy-start"
  | "start"
  | "restart"
  | `start-profile:${string}`
  | `restart-profile:${string}`
  | "stop"
  | "force-stop"
  | "favorite"
  | "ignore"
  | "refresh"
  | "forget";

/** Menus separate these groups with a divider. */
export type ProjectActionGroup = "open" | "lifecycle" | "manage";

/** Copy for the confirm dialog the UI must show *before* calling `run`. */
export interface ProjectActionConfirmation {
  title: string;
  description: string;
  confirmLabel: string;
}

export interface ProjectAction {
  id: ProjectActionId;
  /** Set on `start-profile:<id>` / `restart-profile:<id>`: the launch profile this action runs. */
  profileId?: string;
  group: ProjectActionGroup;
  label: string;
  icon: LucideIcon;
  destructive?: boolean;
  disabled?: boolean;
  /** Tooltip / palette hint explaining why the action is unavailable. */
  disabledReason?: string;
  /** When present, render a `ConfirmDialog` and only call `run` on confirm. */
  confirm?: ProjectActionConfirmation;
  /** Performs the backend call and reports the outcome via toasts. Never rejects. */
  run: () => Promise<void>;
}

export interface ProjectActionOptions {
  /** Editors from `listEditors()`, used to label and gate "Open in …". */
  editors?: readonly EditorInfo[];
  /** `settings.preferredEditor`; the project's own override takes precedence. */
  defaultEditorId?: string | null;
}

const NO_COMMAND = "No start command configured";
const NOT_RUNNING = "Not running";

/**
 * Single source of truth for per-project actions, consumed by the card overflow menu,
 * the detail header and the command palette. Force Stop is only listed while a graceful
 * stop is pending.
 */
export function getProjectActions(
  project: Project,
  options: ProjectActionOptions = {},
): ProjectAction[] {
  const { id, name } = project;
  const stopped = project.runtimeStatus === "stopped";
  const hasCommand = project.primaryCommand !== null;
  const url = project.primaryUrl?.url ?? null;

  const editorId = project.overrides.preferredEditor ?? options.defaultEditorId ?? null;
  const editor = editorId ? options.editors?.find((candidate) => candidate.id === editorId) : undefined;
  const editorUnavailable = editor !== undefined && !editor.available;

  const actions: ProjectAction[] = [
    {
      id: "open-url",
      group: "open",
      label: "Open URL",
      icon: ExternalLink,
      disabled: url === null,
      disabledReason: url === null ? "No URL detected yet" : undefined,
      run: async () => {
        await runAction("Open URL", () => openProjectUrl(id), { success: `Opened ${url}` });
      },
    },
    {
      id: "copy-url",
      group: "open",
      label: "Copy URL",
      icon: Copy,
      disabled: url === null,
      disabledReason: url === null ? "No URL detected yet" : undefined,
      run: async () => {
        if (url === null) return;
        await runAction("Copy URL", () => copyToClipboard(url), { success: "URL copied" });
      },
    },
    {
      id: "open-folder",
      group: "open",
      label: "Open Folder",
      icon: FolderOpen,
      run: async () => {
        await runAction("Open folder", () => openProjectFolder(id), {
          success: `Opened ${name} folder`,
        });
      },
    },
    {
      id: "open-editor",
      group: "open",
      label: editor ? `Open in ${editor.name}` : "Open in Editor",
      icon: Code,
      disabled: editorUnavailable,
      disabledReason: editorUnavailable ? `${editor.name} isn't installed` : undefined,
      run: async () => {
        await runAction("Open in editor", () => openProjectEditor(id), {
          success: editor ? `Opened ${name} in ${editor.name}` : `Opened ${name} in editor`,
        });
      },
    },
    {
      id: "open-terminal",
      group: "open",
      label: "Open Terminal",
      icon: Terminal,
      run: async () => {
        await runAction("Open terminal", () => openProjectTerminal(id), {
          success: `Opened terminal in ${name}`,
        });
      },
    },
    {
      id: "copy-path",
      group: "open",
      label: "Copy path",
      icon: Copy,
      run: async () => {
        await runAction("Copy path", () => copyToClipboard(project.rootPath), { success: "Path copied" });
      },
    },
    {
      id: "copy-cd",
      group: "open",
      label: "Copy cd command",
      icon: CornerDownRight,
      run: async () => {
        await runAction("Copy cd command", () => copyToClipboard(cdCommand(project.rootPath)), {
          success: "cd command copied",
        });
      },
    },
    {
      id: "copy-start",
      group: "open",
      label: "Copy start command",
      icon: Copy,
      disabled: !project.primaryCommand,
      disabledReason: project.primaryCommand ? undefined : NO_COMMAND,
      run: async () => {
        const command = project.primaryCommand;
        if (!command) return;
        await runAction(
          "Copy start command",
          () => copyToClipboard(commandLine(command.program, command.args)),
          { success: "Start command copied" },
        );
      },
    },
    {
      id: "start",
      group: "lifecycle",
      label: "Start",
      icon: Play,
      disabled: !(stopped && hasCommand),
      disabledReason: !stopped ? "Already running" : !hasCommand ? NO_COMMAND : undefined,
      run: async () => {
        await runAction(`Start ${name}`, () => startProject(id), { success: `Starting ${name}` });
      },
    },
    {
      id: "restart",
      group: "lifecycle",
      label: "Restart",
      icon: RotateCw,
      disabled: !(hasCommand && !stopped),
      disabledReason: stopped ? NOT_RUNNING : !hasCommand ? NO_COMMAND : undefined,
      run: async () => {
        await runAction(`Restart ${name}`, () => restartProject(id), {
          success: `Restarting ${name}`,
        });
      },
    },
    {
      id: "stop",
      group: "lifecycle",
      label: "Stop",
      icon: Square,
      disabled: stopped,
      disabledReason: stopped ? NOT_RUNNING : undefined,
      run: async () => {
        const report = await runAction(`Stop ${name}`, () => stopProject(id), { success: false });
        if (report) announceStopReport(name, report, "Stop requested");
      },
    },
  ];

  // Launch profiles: starting only makes sense while stopped, restarting only while live.
  for (const profile of project.launchProfiles) {
    if (stopped) {
      const empty = profile.commands.length === 0;
      actions.push({
        id: `start-profile:${profile.id}`,
        profileId: profile.id,
        group: "lifecycle",
        label: `Start \u201c${profile.name}\u201d`,
        icon: Play,
        disabled: empty,
        disabledReason: empty ? "This profile has no commands" : undefined,
        run: async () => {
          await runAction(`Start ${name} (${profile.name})`, () => startProject(id, profile.id), {
            success: `Starting ${name}: ${profile.name}`,
          });
        },
      });
    } else {
      actions.push({
        id: `restart-profile:${profile.id}`,
        profileId: profile.id,
        group: "lifecycle",
        label: `Restart \u201c${profile.name}\u201d`,
        icon: RotateCw,
        run: async () => {
          await runAction(`Restart ${name} (${profile.name})`, () => restartProject(id, profile.id), {
            success: `Restarting ${name}: ${profile.name}`,
          });
        },
      });
    }
  }

  if (project.stopPending) {
    actions.push({
      id: "force-stop",
      group: "lifecycle",
      label: "Force Stop",
      icon: OctagonX,
      destructive: true,
      confirm: {
        title: `Force stop ${name}?`,
        description:
          "This sends SIGKILL to every process of the project. They get no chance to clean up, so unsaved work or in-flight writes can be lost.",
        confirmLabel: "Force Stop",
      },
      run: async () => {
        const report = await runAction(`Force stop ${name}`, () => forceStopProject(id), {
          success: false,
        });
        if (report) announceStopReport(name, report, "Force stopped");
      },
    });
  }

  actions.push(
    {
      id: "favorite",
      group: "manage",
      label: project.favorite ? "Unpin" : "Pin",
      icon: project.favorite ? PinOff : Pin,
      run: async () => {
        await runAction(project.favorite ? "Unpin" : "Pin", () => setProjectFavorite(id, !project.favorite), {
          success: project.favorite ? `Unpinned ${name}` : `Pinned ${name}`,
        });
      },
    },
    {
      id: "ignore",
      group: "manage",
      label: project.ignored ? "Stop ignoring" : "Ignore",
      icon: project.ignored ? Eye : EyeOff,
      run: async () => {
        await runAction(project.ignored ? "Stop ignoring" : "Ignore", () => ignoreProject(id, !project.ignored), {
          success: project.ignored ? `${name} is visible again` : `Ignoring ${name}`,
        });
      },
    },
    {
      id: "refresh",
      group: "manage",
      label: "Refresh detection",
      icon: RefreshCw,
      run: async () => {
        await runAction("Refresh detection", () => refreshProject(id), {
          success: `Refreshed ${name}`,
        });
      },
    },
    {
      id: "forget",
      group: "manage",
      label: "Forget",
      icon: Trash2,
      destructive: true,
      disabled: !stopped,
      disabledReason: stopped ? undefined : "Stop the project first",
      confirm: {
        title: `Forget ${name}?`,
        description:
          "LocalDeck removes this project and its saved overrides. It can show up again if LocalDeck detects it running later.",
        confirmLabel: "Forget",
      },
      run: async () => {
        await runAction("Forget project", () => forgetProject(id), { success: `Forgot ${name}` });
      },
    },
  );

  return actions;
}
