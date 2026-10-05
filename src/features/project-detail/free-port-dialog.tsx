import { TriangleAlert } from "lucide-react";
import { useEffect, useId, useRef, useState, type FormEvent } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { runAction } from "@/lib/actions";
import { describeError } from "@/lib/errors";
import { restartProject, startProject, suggestFreePort } from "@/lib/tauri";
import { selectPorts, selectProjects, useRuntimeStore } from "@/stores/runtime-store";
import type { PortInfo, Project } from "@/types";

export type FreePortMode = "start" | "restart";

const FALLBACK_PORT = 3000;

/** The port the project is most likely to want: its first listening port, else its URL's port, else 3000. */
export function preferredPort(project: Project): number {
  const first = project.ports[0];
  if (first !== undefined) return first;
  if (project.primaryUrl) {
    try {
      const port = Number(new URL(project.primaryUrl.url).port);
      if (Number.isInteger(port) && port > 0) return port;
    } catch {
      // A URL override that doesn't parse falls through to the default.
    }
  }
  return FALLBACK_PORT;
}

/** The entered text as a usable TCP port, or `null`. */
export function parsePort(text: string): number | null {
  const trimmed = text.trim();
  if (!/^\d{1,5}$/.test(trimmed)) return null;
  const port = Number(trimmed);
  return port >= 1 && port <= 65535 ? port : null;
}

/** "node (PID 4242) · web-app" for whoever listens on the port, from the snapshot. */
function describeHolder(entry: PortInfo, projects: readonly Project[]): string {
  const owners = entry.processes.map((process) =>
    process.name ? `${process.name} (PID ${process.pid})` : `PID ${process.pid}`,
  );
  const holder = owners.length > 0 ? owners.join(", ") : "another process";
  const project = entry.projectId === null ? undefined : projects.find((candidate) => candidate.id === entry.projectId);
  return project ? `${holder} of ${project.name}` : holder;
}

function FreePortForm({
  project,
  mode,
  profileId,
  onOpenChange,
}: {
  project: Project;
  mode: FreePortMode;
  profileId: string | null;
  onOpenChange: (open: boolean) => void;
}) {
  const fieldId = useId();
  const ports = useRuntimeStore(selectPorts);
  const projects = useRuntimeStore(selectProjects);
  const [text, setText] = useState(() => String(preferredPort(project)));
  const [looking, setLooking] = useState(true);
  const [lookupError, setLookupError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // Only the latest lookup may write the field (a slow reply must not overwrite typing).
  const lookupSeq = useRef(0);

  const lookup = async (from: number) => {
    const seq = ++lookupSeq.current;
    setLooking(true);
    setLookupError(null);
    try {
      const free = await suggestFreePort(from);
      if (seq === lookupSeq.current) setText(String(free));
    } catch (failure) {
      if (seq === lookupSeq.current) setLookupError(describeError(failure).message);
    } finally {
      if (seq === lookupSeq.current) setLooking(false);
    }
  };

  useEffect(() => {
    void lookup(preferredPort(project));
    return () => {
      lookupSeq.current++;
    };
    // Prefill once per opening; the form unmounts when the dialog closes.
  }, []);

  const port = parsePort(text);
  const holder = port === null ? undefined : ports.find((entry) => entry.port === port);
  // A restart frees the project's own ports first, so they are not a conflict.
  const conflict = holder && !(mode === "restart" && holder.projectId === project.id) ? holder : undefined;

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (port === null) return;
    lookupSeq.current++;
    setBusy(true);
    const verb = mode === "start" ? "Start" : "Restart";
    const run = mode === "start" ? startProject : restartProject;
    const done = await runAction(`${verb} ${project.name} on port ${port}`, async () => {
      await run(project.id, profileId, port);
      return true;
    }, { success: `${mode === "start" ? "Starting" : "Restarting"} ${project.name} on port ${port}` });
    setBusy(false);
    if (done) onOpenChange(false);
  };

  return (
    <form onSubmit={(event) => void submit(event)} className="grid gap-3">
      <div className="grid gap-1.5">
        <Label htmlFor={fieldId}>Port</Label>
        <div className="flex items-center gap-2">
          <Input
            id={fieldId}
            inputMode="numeric"
            value={text}
            className="w-28 font-mono tabular-nums"
            aria-invalid={text.trim() !== "" && port === null}
            aria-describedby={`${fieldId}-help`}
            onChange={(event) => {
              lookupSeq.current++;
              setLooking(false);
              setText(event.target.value);
            }}
          />
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={looking || busy}
            onClick={() => void lookup(port ?? preferredPort(project))}
          >
            Find next free
          </Button>
        </div>
        <p id={`${fieldId}-help`} className="text-xs text-muted-foreground">
          {looking
            ? "Looking for a free port…"
            : port === null
              ? "Enter a port between 1 and 65535."
              : "LocalDeck sets PORT for the command and adds --port for known dev servers."}
        </p>
        {lookupError && <p className="text-xs text-destructive">Couldn't look up a free port: {lookupError}</p>}
        {conflict && (
          <p role="status" className="flex items-start gap-1.5 text-xs text-warning">
            <TriangleAlert aria-hidden className="mt-px size-3.5 shrink-0" />
            <span>
              Port {conflict.port} is already in use by {describeHolder(conflict, projects)}. The command will
              probably fail to bind; pick another port or use Find next free.
            </span>
          </p>
        )}
      </div>
      <DialogFooter>
        <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
          Cancel
        </Button>
        <Button type="submit" disabled={port === null || busy}>
          {busy ? "Starting…" : mode === "start" ? "Start" : "Restart"}
        </Button>
      </DialogFooter>
    </form>
  );
}

/** "Start on a free port…" / "Restart on a free port…": runs the chosen command with `PORT` set. */
export function FreePortDialog({
  project,
  mode,
  profileId,
  open,
  onOpenChange,
}: {
  project: Project;
  mode: FreePortMode;
  /** `null` = the primary command. */
  profileId: string | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const verb = mode === "start" ? "Start" : "Restart";
  const profile = profileId === null ? null : project.launchProfiles.find((candidate) => candidate.id === profileId);
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{verb} on a free port</DialogTitle>
          <DialogDescription>
            {verb} <span className="font-medium text-foreground">{project.name}</span>
            {profile ? <> with the “{profile.name}” profile</> : null} listening on a port of your choice.
          </DialogDescription>
        </DialogHeader>
        {open && <FreePortForm project={project} mode={mode} profileId={profileId} onOpenChange={onOpenChange} />}
      </DialogContent>
    </Dialog>
  );
}
