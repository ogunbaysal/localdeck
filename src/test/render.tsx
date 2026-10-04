import { render } from "@testing-library/react";
import userEvent, { type UserEvent } from "@testing-library/user-event";
import type { ReactElement, ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useRuntimeStore } from "@/stores/runtime-store";
import type { AppSnapshot, Settings } from "@/types";
import { makeSettings, makeSnapshot } from "./fixtures";

export { resetStores } from "./reset";

export interface RenderOptions {
  /** Initial route (default `/`). */
  route?: string;
  /**
   * Snapshot to seed the runtime store with (status `ready`). Default: an empty snapshot.
   * Pass `null` to leave the store in its initial `loading` state.
   */
  snapshot?: AppSnapshot | null;
  settings?: Settings;
  /** Mount a toaster so toast text can be asserted. */
  toaster?: boolean;
}

/** Put the runtime store into `ready` with the given data. */
export function seedRuntime(snapshot: AppSnapshot = makeSnapshot(), settings: Settings = makeSettings()): void {
  const state = useRuntimeStore.getState();
  state.setSnapshot(snapshot);
  state.setSettings(settings);
  state.setReady();
}

/**
 * Render inside a MemoryRouter + TooltipProvider with the runtime store seeded.
 * Returns the Testing Library result plus a ready `user` (user-event).
 * Stores and the fake backend are reset automatically after each test.
 */
export function renderWithProviders(
  ui: ReactElement,
  { route = "/", snapshot, settings, toaster = false }: RenderOptions = {},
) {
  if (snapshot !== null) seedRuntime(snapshot, settings);

  const wrapper = ({ children }: { children: ReactNode }) => (
    <MemoryRouter initialEntries={[route]}>
      <TooltipProvider>
        {children}
        {toaster && <Toaster />}
      </TooltipProvider>
    </MemoryRouter>
  );
  const user: UserEvent = userEvent.setup();
  return Object.assign(render(ui, { wrapper }), { user });
}
