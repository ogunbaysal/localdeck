import { Search, ShieldAlert } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { usePlatform } from "@/hooks/use-platform";
import { formatTime } from "@/lib/format";
import { paletteShortcutLabel } from "@/lib/platform";
import { getLimitedAccessReasons } from "@/lib/scanner";
import { useRuntimeStore } from "@/stores/runtime-store";
import { useUiStore } from "@/stores/ui-store";

/** Scanner heartbeat; turns into an amber "Limited access" marker when something can't be read. */
function ScannerIndicator() {
  const snapshot = useRuntimeStore((state) => state.snapshot);
  if (!snapshot) return null;
  const { scanner, updatedAt } = snapshot;
  const reasons = getLimitedAccessReasons(scanner);
  const limited = reasons.length > 0;

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          tabIndex={0}
          data-slot="scanner-indicator"
          data-limited={limited}
          className="inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
        >
          {limited ? (
            <ShieldAlert aria-hidden className="size-3.5 text-warning" />
          ) : (
            <span aria-hidden className="size-1.5 rounded-full bg-success" />
          )}
          <span className="max-[860px]:sr-only">{limited ? "Limited access" : "Live"}</span>
        </span>
      </TooltipTrigger>
      <TooltipContent align="end" className="flex-col items-start gap-0.5">
        {limited ? (
          reasons.map((reason) => (
            <span key={reason.id}>
              <span className="font-medium">{reason.title}.</span> {reason.detail}
            </span>
          ))
        ) : (
          <span>
            Scanning every {scanner.intervalMs / 1000}s · last scan {scanner.lastScanMs} ms · updated{" "}
            {formatTime(updatedAt)}
          </span>
        )}
      </TooltipContent>
    </Tooltip>
  );
}

/**
 * Window top bar and drag region. On macOS the leading edge is padded for the overlay
 * title bar's traffic lights (`<html data-platform="macos">`).
 */
export function TopBar() {
  const platform = usePlatform();
  const setPaletteOpen = useUiStore((state) => state.setPaletteOpen);

  return (
    <header
      data-tauri-drag-region
      className="grid h-10 shrink-0 grid-cols-[1fr_minmax(0,24rem)_1fr] items-center border-b bg-sidebar px-3 in-data-[platform=macos]:pl-[78px]"
    >
      <div data-tauri-drag-region className="flex min-w-0 items-center">
        <span data-tauri-drag-region className="truncate text-sm font-semibold tracking-tight">
          LocalDeck
        </span>
      </div>
      <Button
        variant="outline"
        size="sm"
        aria-label="Search projects, ports and actions"
        className="w-full justify-start gap-2 bg-background font-normal text-muted-foreground"
        onClick={() => setPaletteOpen(true)}
      >
        <Search className="size-3.5" />
        <span className="min-w-0 flex-1 truncate text-left">Search projects, ports, actions…</span>
        <kbd className="rounded border bg-muted px-1 font-mono text-[11px] leading-4 text-muted-foreground">
          {paletteShortcutLabel(platform)}
        </kbd>
      </Button>
      <div data-tauri-drag-region className="flex min-w-0 items-center justify-end">
        <ScannerIndicator />
      </div>
    </header>
  );
}
