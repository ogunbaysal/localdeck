/**
 * The only door to the Rust backend: typed wrappers over Tauri `invoke`/`listen`.
 * Command names are the snake_case forms of the function names. Every rejection
 * is normalised into an `ErrorPayload` (see `./errors`).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { toErrorPayload } from "./errors";
import type {
  AppSnapshot,
  ComponentKind,
  EditorInfo,
  ProjectOverrides,
  ProjectScripts,
  Settings,
  StopReport,
} from "@/types";

export { describeError, isErrorPayload, toErrorPayload } from "./errors";
export type { UnlistenFn };

export const RUNTIME_UPDATED_EVENT = "runtime://updated";
export const OPEN_PROJECT_EVENT = "ui://open-project";

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toErrorPayload(error);
  }
}

/* ── State ─────────────────────────────────────────────────────────────── */

export const getSnapshot = (): Promise<AppSnapshot> => call("get_snapshot");

export const getSettings = (): Promise<Settings> => call("get_settings");

/** Full replace; the backend validates and syncs autostart. */
export const updateSettings = (settings: Settings): Promise<Settings> =>
  call("update_settings", { settings });

export const listEditors = (): Promise<EditorInfo[]> => call("list_editors");

/* ── Project metadata ──────────────────────────────────────────────────── */

/** Replaces displayName/primaryUrl/preferredEditor; `null`/`""` clears a field. */
export const setProjectOverride = (
  projectId: number,
  overrides: ProjectOverrides,
): Promise<void> => call("set_project_override", { projectId, overrides });

/** `null` resets the component to its detected kind. */
export const setComponentKind = (
  projectId: number,
  componentKey: string,
  kind: ComponentKind | null,
): Promise<void> => call("set_component_kind", { projectId, componentKey, kind });

export const setProjectFavorite = (projectId: number, favorite: boolean): Promise<void> =>
  call("set_project_favorite", { projectId, favorite });

export const ignoreProject = (projectId: number, ignored: boolean): Promise<void> =>
  call("ignore_project", { projectId, ignored });

/** Stopped projects only. */
export const forgetProject = (projectId: number): Promise<void> =>
  call("forget_project", { projectId });

/** Returns the removed count (never touches ignored or favorite projects). */
export const forgetStoppedProjects = (): Promise<number> => call("forget_stopped_projects");

/** Re-runs stack detection. */
export const refreshProject = (projectId: number): Promise<void> =>
  call("refresh_project", { projectId });

export const listProjectScripts = (projectId: number): Promise<ProjectScripts> =>
  call("list_project_scripts", { projectId });

/** Pick a package.json script as the primary command; `null` clears it. */
export const setPrimaryCommand = (projectId: number, script: string | null): Promise<void> =>
  call("set_primary_command", { projectId, script });

/* ── Lifecycle ─────────────────────────────────────────────────────────── */

export const startProject = (projectId: number): Promise<void> =>
  call("start_project", { projectId });

export const restartProject = (projectId: number): Promise<void> =>
  call("restart_project", { projectId });

/** Graceful only (SIGTERM). */
export const stopProject = (projectId: number): Promise<StopReport> =>
  call("stop_project", { projectId });

/** Explicit SIGKILL. Always confirm with the user first. */
export const forceStopProject = (projectId: number): Promise<StopReport> =>
  call("force_stop_project", { projectId });

/** Pass `ProcessInfo.startedAt` for stale-PID protection. */
export const terminateProcess = (pid: number, startedAt: number | null): Promise<void> =>
  call("terminate_process", { pid, startedAt });

/** Pass `ProcessInfo.startedAt` for stale-PID protection. */
export const forceKillProcess = (pid: number, startedAt: number | null): Promise<void> =>
  call("force_kill_process", { pid, startedAt });

/* ── Open / reveal ─────────────────────────────────────────────────────── */

export const openProjectUrl = (projectId: number): Promise<void> =>
  call("open_project_url", { projectId });

export const openPortUrl = (portId: string): Promise<void> =>
  call("open_port_url", { portId });

export const openProjectFolder = (projectId: number): Promise<void> =>
  call("open_project_folder", { projectId });

export const openProjectEditor = (projectId: number): Promise<void> =>
  call("open_project_editor", { projectId });

export const openProjectTerminal = (projectId: number): Promise<void> =>
  call("open_project_terminal", { projectId });

export const openProcessCwd = (pid: number): Promise<void> =>
  call("open_process_cwd", { pid });

export const dismissAttention = (projectId: number, attentionId: string): Promise<void> =>
  call("dismiss_attention", { projectId, attentionId });

/* ── Events (Rust → React) ─────────────────────────────────────────────── */

async function subscribe<T>(
  event: string,
  accept: (payload: unknown) => payload is T,
  callback: (payload: T) => void,
): Promise<UnlistenFn> {
  try {
    return await listen<unknown>(event, ({ payload }) => {
      if (accept(payload)) callback(payload);
    });
  } catch (error) {
    throw toErrorPayload(error);
  }
}

const isSnapshot = (payload: unknown): payload is AppSnapshot =>
  typeof payload === "object" &&
  payload !== null &&
  Array.isArray((payload as { projects?: unknown }).projects) &&
  Array.isArray((payload as { ports?: unknown }).ports);

const isProjectId = (payload: unknown): payload is number =>
  typeof payload === "number" && Number.isInteger(payload);

/** Full snapshot roughly every 2 s. Resolves to the unlisten function. */
export const listenRuntimeUpdates = (
  callback: (snapshot: AppSnapshot) => void,
): Promise<UnlistenFn> => subscribe(RUNTIME_UPDATED_EVENT, isSnapshot, callback);

/** Emitted when a tray project entry is clicked. Resolves to the unlisten function. */
export const listenOpenProject = (callback: (projectId: number) => void): Promise<UnlistenFn> =>
  subscribe(OPEN_PROJECT_EVENT, isProjectId, callback);
