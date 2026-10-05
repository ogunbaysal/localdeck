import { ChevronRight, Copy, Ellipsis, FolderOpen, OctagonX, Square } from "lucide-react";
import { memo, useCallback, useMemo, useState } from "react";
import { ConfirmDialog } from "@/components/shared/confirm-dialog";
import { Mono } from "@/components/shared/mono";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { copyToClipboard, runAction } from "@/lib/actions";
import { formatBytes, formatCpu, formatUptime } from "@/lib/format";
import { forceKillProcess, openProcessCwd, terminateProcess } from "@/lib/tauri";
import { killCommand } from "@/lib/shell-quote";
import { cn } from "@/lib/utils";
import { selectDiagnosticsExpanded, useUiStore } from "@/stores/ui-store";
import type { ProcessInfo } from "@/types";
import { ACCESS_HINT, ACCESS_LABEL, LIMITED_ACCESS_HINT, ORIGIN_LABEL, ORIGIN_SHORT_LABEL } from "./labels";
import { useNow } from "./use-now";

const DASH = "—";
const INDENT_PX = 12;

/** The command line as the OS reports it (the backend already redacts secrets). */
function commandLine(process: ProcessInfo): string {
  if (process.args.length > 0) return process.args.join(" ");
  return process.executable ?? process.name;
}

function describe(process: ProcessInfo): string {
  return `${process.name} (PID ${process.pid})`;
}

interface RowActions {
  onTerminate: (process: ProcessInfo) => void;
  /** Opens the confirmation dialog; the kill itself only runs after the explicit confirm. */
  onForceKill: (process: ProcessInfo) => void;
  onCopyCommand: (process: ProcessInfo) => void;
  onCopyPid: (process: ProcessInfo) => void;
  /** Copies `kill -TERM <pid>` / `kill -KILL <pid>` for the user's own shell; nothing is run. */
  onCopyKill: (process: ProcessInfo, force: boolean) => void;
  onOpenCwd: (process: ProcessInfo) => void;
}

const ROW_ACTIONS: Omit<RowActions, "onForceKill"> = {
  onTerminate: (process) => {
    void runAction("Terminate", () => terminateProcess(process.pid, process.startedAt), {
      success: `Terminate requested for ${describe(process)}`,
    });
  },
  onCopyCommand: (process) => {
    void runAction("Copy command", () => copyToClipboard(commandLine(process)), { success: "Command copied" });
  },
  onCopyPid: (process) => {
    void runAction("Copy PID", () => copyToClipboard(String(process.pid)), { success: "PID copied" });
  },
  onCopyKill: (process, force) => {
    void runAction(
      force ? "Copy force-kill command" : "Copy kill command",
      () => copyToClipboard(killCommand(process.pid, force)),
      { success: force ? "Force-kill command copied (not run)" : "Kill command copied (not run)" },
    );
  },
  onOpenCwd: (process) => {
    void runAction("Open working directory", () => openProcessCwd(process.pid), {
      success: `Opened working directory of ${describe(process)}`,
    });
  },
};

const ProcessRow = memo(function ProcessRow({
  process,
  now,
  actions,
}: {
  process: ProcessInfo;
  now: number;
  actions: RowActions;
}) {
  const hidden = process.accessLevel === "unavailable";
  const command = commandLine(process);
  return (
    <TableRow data-pid={process.pid}>
      <TableCell>
        <Mono className="tabular-nums">{process.pid}</Mono>
      </TableCell>
      <TableCell>
        <Mono className="tabular-nums text-muted-foreground">{process.parentPid ?? DASH}</Mono>
      </TableCell>
      <TableCell style={{ paddingLeft: `${8 + process.depth * INDENT_PX}px` }}>
        <span className="block max-w-28 truncate" title={process.name}>
          {process.name}
        </span>
      </TableCell>
      <TableCell className="tabular-nums">{hidden ? DASH : formatCpu(process.cpuUsage)}</TableCell>
      <TableCell className="tabular-nums">{hidden ? DASH : formatBytes(process.memoryBytes)}</TableCell>
      <TableCell className="tabular-nums">{formatUptime(process.startedAt, now)}</TableCell>
      <TableCell>
        <Badge
          variant="outline"
          title={ACCESS_HINT[process.accessLevel]}
          className={cn(
            process.accessLevel === "partial" && "border-warning/40 text-warning",
            hidden && "text-muted-foreground",
          )}
        >
          {ACCESS_LABEL[process.accessLevel]}
        </Badge>
      </TableCell>
      <TableCell>
        <Badge
          variant="outline"
          title={ORIGIN_LABEL[process.origin]}
          className={cn(process.origin === "localDeckManaged" && "border-primary/40 text-primary")}
        >
          {ORIGIN_SHORT_LABEL[process.origin]}
        </Badge>
      </TableCell>
      <TableCell>
        <Tooltip>
          <TooltipTrigger asChild>
            <Mono className="block max-w-44 truncate text-xs text-muted-foreground">{command}</Mono>
          </TooltipTrigger>
          <TooltipContent className="max-w-lg break-all font-mono">{command}</TooltipContent>
        </Tooltip>
      </TableCell>
      <TableCell className="text-right">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button type="button" variant="ghost" size="icon-xs" aria-label={`Actions for ${describe(process)}`}>
              <Ellipsis aria-hidden />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-56">
            <DropdownMenuItem onSelect={() => actions.onTerminate(process)}>
              <Square aria-hidden />
              Terminate
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => actions.onCopyCommand(process)}>
              <Copy aria-hidden />
              Copy command
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => actions.onCopyPid(process)}>
              <Copy aria-hidden />
              Copy PID
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => actions.onCopyKill(process, false)}>
              <Copy aria-hidden />
              Copy kill command (SIGTERM)
            </DropdownMenuItem>
            <DropdownMenuItem disabled={process.cwd === null} onSelect={() => actions.onOpenCwd(process)}>
              <FolderOpen aria-hidden />
              Open working directory
              {process.cwd === null && <span className="sr-only"> (unavailable: the OS didn't report one)</span>}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem variant="destructive" onSelect={() => actions.onCopyKill(process, true)}>
              <Copy aria-hidden />
              Copy force-kill command (SIGKILL)
            </DropdownMenuItem>
            <DropdownMenuItem variant="destructive" onSelect={() => actions.onForceKill(process)}>
              <OctagonX aria-hidden />
              Force Kill…
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </TableCell>
    </TableRow>
  );
});

function ProcessTable({ processes }: { processes: readonly ProcessInfo[] }) {
  const now = useNow(2000);
  const [killTarget, setKillTarget] = useState<ProcessInfo | null>(null);
  const [killOpen, setKillOpen] = useState(false);

  const requestForceKill = useCallback((process: ProcessInfo) => {
    setKillTarget(process);
    setKillOpen(true);
  }, []);
  // Stable identity so memoised rows don't re-render because of the parent.
  const actions = useMemo<RowActions>(() => ({ ...ROW_ACTIONS, onForceKill: requestForceKill }), [requestForceKill]);

  const limited = processes.some((process) => process.accessLevel !== "full");

  if (processes.length === 0) {
    return <p className="px-1 py-2 text-muted-foreground">No processes are running for this project.</p>;
  }

  return (
    <div className="space-y-2">
      {limited && <p className="text-xs text-muted-foreground">{LIMITED_ACCESS_HINT}</p>}
      <div className="rounded-lg border">
        <Table className="text-xs">
          <TableHeader>
            <TableRow className="text-muted-foreground hover:bg-transparent">
              <TableHead>PID</TableHead>
              <TableHead>PPID</TableHead>
              <TableHead>Name</TableHead>
              <TableHead>CPU</TableHead>
              <TableHead>Memory</TableHead>
              <TableHead>Uptime</TableHead>
              <TableHead>Access</TableHead>
              <TableHead>Origin</TableHead>
              <TableHead>Command</TableHead>
              <TableHead>
                <span className="sr-only">Actions</span>
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {processes.map((process) => (
              <ProcessRow key={process.pid} process={process} now={now} actions={actions} />
            ))}
          </TableBody>
        </Table>
      </div>

      <ConfirmDialog
        open={killOpen}
        onOpenChange={setKillOpen}
        destructive
        title={killTarget ? `Force kill ${describe(killTarget)}?` : "Force kill process?"}
        description="Force Kill sends SIGKILL. The process cannot catch or ignore it, so it gets no chance to save state or clean up, and unsaved work or in-flight writes may be lost. Use Terminate first unless the process is stuck."
        confirmLabel="Force Kill"
        onConfirm={async () => {
          if (!killTarget) return;
          const { pid, startedAt } = killTarget;
          await runAction("Force kill", () => forceKillProcess(pid, startedAt), {
            success: `Force killed ${describe(killTarget)}`,
          });
        }}
      />
    </div>
  );
}

/** Collapsible raw-process diagnostics. Hidden by default; the open state survives navigation. */
export const ProcessesSection = memo(function ProcessesSection({
  projectId,
  processes,
}: {
  projectId: number;
  processes: readonly ProcessInfo[];
}) {
  const expanded = useUiStore(selectDiagnosticsExpanded(projectId));
  const setExpanded = useUiStore((state) => state.setDiagnosticsExpanded);

  return (
    <section aria-label="Process diagnostics">
      <Collapsible open={expanded} onOpenChange={(open) => setExpanded(projectId, open)} className="space-y-2">
        <h2 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
          <CollapsibleTrigger asChild>
            <button
              type="button"
              className="-ml-1 inline-flex items-center gap-1 rounded-sm px-1 py-0.5 uppercase outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/50"
            >
              <ChevronRight aria-hidden className={cn("size-3.5 transition-transform", expanded && "rotate-90")} />
              Processes ({processes.length})
            </button>
          </CollapsibleTrigger>
        </h2>
        <CollapsibleContent className="space-y-2">
          <p className="text-muted-foreground">
            Raw OS processes LocalDeck attributes to this project. For diagnostics; prefer the actions above.
          </p>
          <ProcessTable processes={processes} />
        </CollapsibleContent>
      </Collapsible>
    </section>
  );
});
