import { create } from "zustand";
import type { StatusFilter } from "@/lib/search";

/** Global Ports screen quick filter. `assigned` = owned/shared by a project; `unassigned` = unassigned or ambiguous. */
export type PortsFilter = "all" | "http" | "assigned" | "unassigned";

/** Frontend-only state. Nothing here comes from (or goes back into) the runtime snapshot. */
export interface UiState {
  searchQuery: string;
  statusFilter: StatusFilter;
  paletteOpen: boolean;
  /** User toggle; the shell also collapses automatically on narrow windows. */
  sidebarCollapsed: boolean;
  /** Project ids whose raw-process diagnostics section is expanded. */
  expandedDiagnostics: Readonly<Record<number, true>>;
  portsSearch: string;
  portsFilter: PortsFilter;
  selectedPortId: string | null;

  setSearchQuery: (query: string) => void;
  setStatusFilter: (filter: StatusFilter) => void;
  setPaletteOpen: (open: boolean) => void;
  togglePalette: () => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
  setDiagnosticsExpanded: (projectId: number, expanded: boolean) => void;
  setPortsSearch: (query: string) => void;
  setPortsFilter: (filter: PortsFilter) => void;
  selectPort: (portId: string | null) => void;
}

export const useUiStore = create<UiState>()((set) => ({
  searchQuery: "",
  statusFilter: "all",
  paletteOpen: false,
  sidebarCollapsed: false,
  expandedDiagnostics: {},
  portsSearch: "",
  portsFilter: "all",
  selectedPortId: null,

  setSearchQuery: (searchQuery) => set({ searchQuery }),
  setStatusFilter: (statusFilter) => set({ statusFilter }),
  setPaletteOpen: (paletteOpen) => set({ paletteOpen }),
  togglePalette: () => set((state) => ({ paletteOpen: !state.paletteOpen })),
  setSidebarCollapsed: (sidebarCollapsed) => set({ sidebarCollapsed }),
  setDiagnosticsExpanded: (projectId, expanded) =>
    set((state) => {
      const next = { ...state.expandedDiagnostics };
      if (expanded) next[projectId] = true;
      else delete next[projectId];
      return { expandedDiagnostics: next };
    }),
  setPortsSearch: (portsSearch) => set({ portsSearch }),
  setPortsFilter: (portsFilter) => set({ portsFilter }),
  selectPort: (selectedPortId) => set({ selectedPortId }),
}));

/** Per-project selector: only re-renders when that project's flag flips. */
export const selectDiagnosticsExpanded =
  (projectId: number) =>
  (state: UiState): boolean =>
    state.expandedDiagnostics[projectId] === true;
