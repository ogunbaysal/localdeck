import { create } from "zustand";
import { structuralShare } from "@/lib/structural-share";
import type { AppSnapshot, ErrorPayload, PortInfo, Project, ScannerStatus, Settings } from "@/types";

export type BootstrapStatus = "loading" | "ready" | "error";

export interface RuntimeState {
  /** Rust-owned projection of the latest scan. Never persisted client-side. */
  snapshot: AppSnapshot | null;
  settings: Settings | null;
  status: BootstrapStatus;
  /** Set when `status === "error"`. */
  error: ErrorPayload | null;

  /**
   * Replace the snapshot, reusing unchanged sub-trees. A snapshot older than the current
   * one (`updatedAt`) is ignored so a slow initial fetch can't overwrite a newer event.
   */
  setSnapshot: (snapshot: AppSnapshot) => void;
  setSettings: (settings: Settings) => void;
  setLoading: () => void;
  setReady: () => void;
  setError: (error: ErrorPayload) => void;
}

export const useRuntimeStore = create<RuntimeState>()((set) => ({
  snapshot: null,
  settings: null,
  status: "loading",
  error: null,

  setSnapshot: (snapshot) =>
    set((state) => {
      if (state.snapshot && snapshot.updatedAt < state.snapshot.updatedAt) return state;
      return { snapshot: structuralShare(state.snapshot, snapshot) };
    }),
  setSettings: (settings) => set((state) => ({ settings: structuralShare(state.settings, settings) })),
  setLoading: () => set({ status: "loading", error: null }),
  setReady: () => set({ status: "ready", error: null }),
  setError: (error) => set({ status: "error", error }),
}));

/* ── Selectors ─────────────────────────────────────────────────────────────
 * Selectors that derive arrays are memoised per source array (WeakMap), so they
 * return a stable reference until that part of the snapshot really changes. That
 * keeps `useRuntimeStore(selector)` free of render loops and avoids re-renders on
 * every 2 s tick. Per-id selectors are factories: `useRuntimeStore(selectProjectById(id))`.
 */

const EMPTY_PROJECTS: readonly Project[] = [];
const EMPTY_PORTS: readonly PortInfo[] = [];

export const selectProjects = (state: RuntimeState): readonly Project[] =>
  state.snapshot?.projects ?? EMPTY_PROJECTS;

export const selectPorts = (state: RuntimeState): readonly PortInfo[] =>
  state.snapshot?.ports ?? EMPTY_PORTS;

export const selectScanner = (state: RuntimeState): ScannerStatus | null =>
  state.snapshot?.scanner ?? null;

export const selectSettings = (state: RuntimeState): Settings | null => state.settings;

const visibleCache = new WeakMap<readonly Project[], readonly Project[]>();
const ignoredCache = new WeakMap<readonly Project[], readonly Project[]>();

/** Non-ignored projects: what the dashboard, search and palette show. */
export function selectVisibleProjects(state: RuntimeState): readonly Project[] {
  const projects = selectProjects(state);
  let visible = visibleCache.get(projects);
  if (!visible) {
    visible = projects.filter((project) => !project.ignored);
    visibleCache.set(projects, visible);
  }
  return visible;
}

/** Ignored projects: listed in Settings so they can be un-ignored. */
export function selectIgnoredProjects(state: RuntimeState): readonly Project[] {
  const projects = selectProjects(state);
  let ignored = ignoredCache.get(projects);
  if (!ignored) {
    ignored = projects.filter((project) => project.ignored);
    ignoredCache.set(projects, ignored);
  }
  return ignored;
}

/** Returns the project (ignored or not) or `undefined`. Stable while that project is unchanged. */
export const selectProjectById =
  (projectId: number | null | undefined) =>
  (state: RuntimeState): Project | undefined =>
    projectId == null ? undefined : selectProjects(state).find((project) => project.id === projectId);

const portsByProjectCache = new WeakMap<readonly PortInfo[], Map<number, PortInfo[]>>();

/** Port rows attributed to a project (`projectId` match, assigned or shared). */
export const selectPortsForProject =
  (projectId: number | null | undefined) =>
  (state: RuntimeState): readonly PortInfo[] => {
    if (projectId == null) return EMPTY_PORTS;
    const ports = selectPorts(state);
    let grouped = portsByProjectCache.get(ports);
    if (!grouped) {
      grouped = new Map();
      for (const port of ports) {
        if (port.projectId === null) continue;
        const bucket = grouped.get(port.projectId);
        if (bucket) bucket.push(port);
        else grouped.set(port.projectId, [port]);
      }
      portsByProjectCache.set(ports, grouped);
    }
    return grouped.get(projectId) ?? EMPTY_PORTS;
  };
