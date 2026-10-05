import {
  ExternalLink,
  FolderPlus,
  History,
  LayoutGrid,
  Network,
  Settings as SettingsIcon,
  type LucideIcon,
} from "lucide-react";
import { hostPort, isOwned, portDigits } from "@/features/ports/port-model";
import { runAction } from "@/lib/actions";
import { formatPortList } from "@/lib/format";
import { getProjectActions, type ProjectActionConfirmation, type ProjectActionId } from "@/lib/project-actions";
import { matchProject } from "@/lib/search";
import { openPortUrl, pickAndAddProject } from "@/lib/tauri";
import type { EditorInfo, PortInfo, Project, RuntimeStatus } from "@/types";

export type PaletteGroup = "navigate" | "projects" | "actions" | "ports";

/** Prefix of the launch-profile actions `getProjectActions` emits; the palette lists them itself. */
const START_PROFILE_PREFIX = "start-profile:";

export const PALETTE_GROUP_HEADINGS: Record<PaletteGroup, string> = {
  navigate: "Navigate",
  projects: "Projects",
  actions: "Actions",
  ports: "Ports",
};

export const PALETTE_GROUP_ORDER: readonly PaletteGroup[] = ["navigate", "projects", "actions", "ports"];

/** What the palette needs from the app to carry out navigation-flavoured items. */
export interface PaletteContext {
  navigate: (to: string) => void;
  /** Open the Ports screen focused on `port` (search text + selection). */
  revealPort: (port: PortInfo) => void;
}

export interface PaletteItem {
  /** Unique cmdk value. */
  id: string;
  group: PaletteGroup;
  label: string;
  /** Muted trailing text (owning project, ports …). */
  meta?: string;
  icon?: LucideIcon;
  /** Set for project rows: renders a status dot instead of the icon. */
  status?: RuntimeStatus;
  /** Whose ports / root path / components / stack the query may also match. */
  project?: Project;
  /** For port entries: the port number (prefix-matched by `3000`, `:3000`, `localhost:3000`). */
  portNumber?: number;
  /** Extra lowercase text the query may match (process names …). */
  extra?: string;
  /** When set, show a `ConfirmDialog` and only call `run` on confirm. */
  confirm?: ProjectActionConfirmation;
  destructive?: boolean;
  run: () => void | Promise<void>;
}

export interface PaletteInput {
  projects: readonly Project[];
  ports: readonly PortInfo[];
  editors: readonly EditorInfo[];
  defaultEditorId: string | null;
}

const NAVIGATION: ReadonlyArray<{ to: string; label: string; icon: LucideIcon }> = [
  { to: "/", label: "Go to Projects", icon: LayoutGrid },
  { to: "/ports", label: "Go to Ports", icon: Network },
  { to: "/activity", label: "Open Activity", icon: History },
  { to: "/settings", label: "Go to Settings", icon: SettingsIcon },
];

/** Palette label per project action id. Pin, ignore and refresh live in the menus, not here. */
const ACTION_LABELS: Partial<Record<ProjectActionId, (name: string, editor: string) => string>> = {
  "open-url": (name) => `Open ${name}`,
  "copy-url": (name) => `Copy ${name} URL`,
  "open-folder": (name) => `Reveal ${name} folder`,
  "open-editor": (name, editor) => `Open ${name} in ${editor}`,
  "open-terminal": (name) => `Open terminal in ${name}`,
  start: (name) => `Start ${name}`,
  restart: (name) => `Restart ${name}`,
  stop: (name) => `Stop ${name}`,
  "force-stop": (name) => `Force stop ${name}`,
  forget: (name) => `Forget ${name}`,
};

/** Running/partial projects first, then by name. */
function sortProjects(projects: readonly Project[]): Project[] {
  const rank = (project: Project) => (project.runtimeStatus === "stopped" ? 1 : 0);
  return [...projects].sort(
    (a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
  );
}

/** Every palette entry, in display order (navigate → projects → actions → ports). */
export function buildPaletteItems(input: PaletteInput, context: PaletteContext): PaletteItem[] {
  const { projects, ports, editors, defaultEditorId } = input;
  const ordered = sortProjects(projects);
  const items: PaletteItem[] = [];

  for (const { to, label, icon } of NAVIGATION) {
    items.push({ id: `nav:${to}`, group: "navigate", label, icon, run: () => context.navigate(to) });
  }

  items.push({
    id: "add-project",
    group: "navigate",
    label: "Add project…",
    icon: FolderPlus,
    run: async () => {
      const projectId = await runAction("Add project", pickAndAddProject, { success: false });
      // `null` = the folder dialog was cancelled.
      if (projectId !== undefined && projectId !== null) context.navigate(`/projects/${projectId}`);
    },
  });

  for (const project of ordered) {
    items.push({
      id: `project:${project.id}`,
      group: "projects",
      label: project.name,
      meta: project.ports.length > 0 ? formatPortList(project.ports, 3) : undefined,
      status: project.runtimeStatus,
      project,
      run: () => context.navigate(`/projects/${project.id}`),
    });
  }

  for (const project of ordered) {
    for (const action of getProjectActions(project, { editors, defaultEditorId })) {
      if (action.id.startsWith(START_PROFILE_PREFIX)) {
        // One entry per launch profile: “Start “Dev stack”” with the project as its meta text.
        if (!action.disabled) {
          items.push({
            id: `action:${project.id}:${action.id}`,
            group: "actions",
            label: action.label,
            meta: project.name,
            icon: action.icon,
            project,
            run: action.run,
          });
        }
        continue;
      }
      const label = ACTION_LABELS[action.id];
      if (!label || action.disabled) continue;
      const url = project.primaryUrl?.url;
      items.push({
        id: `action:${project.id}:${action.id}`,
        group: "actions",
        label: label(project.name, action.label.replace(/^Open in /, "")),
        meta: (action.id === "open-url" || action.id === "copy-url") && url ? hostPort(url) : undefined,
        icon: action.icon,
        project,
        confirm: action.confirm,
        destructive: action.destructive,
        run: action.run,
      });
    }
  }

  const projectById = new Map(projects.map((project) => [project.id, project]));
  for (const port of [...ports].sort((a, b) => a.port - b.port || a.id.localeCompare(b.id))) {
    const owner = isOwned(port) && port.projectId !== null ? projectById.get(port.projectId) : undefined;
    const processNames = port.processes.flatMap((process) => (process.name ? [process.name] : []));
    const meta = owner?.name ?? (processNames.length > 0 ? processNames.join(", ") : undefined);
    const extra = [owner?.name, ...processNames].filter(Boolean).join(" ").toLowerCase();
    const common = { group: "ports", meta, portNumber: port.port, extra } as const;
    const { url } = port;
    if (port.isHttp && url) {
      items.push({
        ...common,
        id: `port-open:${port.id}`,
        label: `Open ${hostPort(url)}`,
        icon: ExternalLink,
        run: async () => {
          await runAction("Open URL", () => openPortUrl(port.id), { success: `Opened ${url}` });
        },
      });
    }
    items.push({
      ...common,
      id: `port-reveal:${port.id}`,
      label: `Reveal process on :${port.port}`,
      icon: Network,
      run: () => context.revealPort(port),
    });
  }
  return items;
}

/**
 * Every whitespace-separated token must match the item: its label/meta text, a port
 * (`3000`, `:3000`, `localhost:3000`) it stands for, or — for project-bound items — anything
 * `matchProject` understands (root path, stack, components, owned ports). Empty query keeps all.
 */
export function filterPaletteItems(
  items: readonly PaletteItem[],
  ports: readonly PortInfo[],
  query: string,
): PaletteItem[] {
  const tokens = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (tokens.length === 0) return [...items];
  return items.filter((item) => {
    const text = `${item.label} ${item.meta ?? ""} ${item.extra ?? ""}`.toLowerCase();
    return tokens.every((token) => {
      if (text.includes(token)) return true;
      if (item.portNumber !== undefined) {
        const digits = portDigits(token);
        if (digits !== null) return String(item.portNumber).startsWith(digits);
      }
      return item.project !== undefined && matchProject(item.project, ports, token);
    });
  });
}
