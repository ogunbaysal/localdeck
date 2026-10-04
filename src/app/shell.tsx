import { useEffect } from "react";
import { Outlet } from "react-router";
import { CommandPalette } from "@/features/command-palette/CommandPalette";
import { ErrorState } from "@/components/shared/error-state";
import { LimitedAccessBanner } from "@/components/shared/limited-access-banner";
import { Skeleton } from "@/components/ui/skeleton";
import { useMediaQuery } from "@/hooks/use-media-query";
import { usePlatform } from "@/hooks/use-platform";
import { isPaletteShortcut } from "@/lib/platform";
import { useRuntimeStore } from "@/stores/runtime-store";
import { useUiStore } from "@/stores/ui-store";
import { useRetryBootstrap } from "./bootstrap";
import { Sidebar } from "./sidebar";
import { TopBar } from "./top-bar";

/** Window width (px) under which the sidebar always collapses to icons. */
const NARROW_QUERY = "(max-width: 860px)";

/** ⌘K / Ctrl+K toggles the command palette anywhere in the window. */
function useGlobalPaletteShortcut() {
  const platform = usePlatform();
  const togglePalette = useUiStore((state) => state.togglePalette);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!isPaletteShortcut(event, platform)) return;
      event.preventDefault();
      togglePalette();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [platform, togglePalette]);
}

function LoadingSkeleton() {
  return (
    <div role="status" aria-label="Loading projects" className="space-y-3">
      <Skeleton className="h-5 w-40" />
      <div className="space-y-2">
        {Array.from({ length: 4 }, (_, index) => (
          <Skeleton key={index} className="h-16 w-full" />
        ))}
      </div>
    </div>
  );
}

/** App frame: top bar, sidebar and the routed page (or the loading / fatal-error state). */
export function Shell() {
  useGlobalPaletteShortcut();
  const status = useRuntimeStore((state) => state.status);
  const error = useRuntimeStore((state) => state.error);
  const retry = useRetryBootstrap();
  const autoCollapsed = useMediaQuery(NARROW_QUERY);
  const userCollapsed = useUiStore((state) => state.sidebarCollapsed);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <TopBar />
      <div className="flex min-h-0 flex-1">
        <Sidebar collapsed={autoCollapsed || userCollapsed} autoCollapsed={autoCollapsed} />
        <main className="min-w-0 flex-1 overflow-y-auto">
          <div className="mx-auto flex w-full max-w-6xl flex-col gap-3 px-5 py-4">
            {status === "loading" && <LoadingSkeleton />}
            {status === "error" && error && (
              <ErrorState error={error} onRetry={retry} retryLabel="Retry" />
            )}
            {status === "ready" && (
              <>
                <LimitedAccessBanner />
                <Outlet />
              </>
            )}
          </div>
        </main>
      </div>
      <CommandPalette />
    </div>
  );
}
