import { Copy, ExternalLink, FolderSearch, OctagonX, ShieldAlert, Square } from "lucide-react";
import { useState, type ReactNode } from "react";
import { Link } from "react-router";
import { ConfirmDialog, CopyButton, Mono } from "@/components/shared";
import { Button } from "@/components/ui/button";
import { copyToClipboard, runAction } from "@/lib/actions";
import { formatUptime } from "@/lib/format";
import { forceKillProcess, openPortUrl, terminateProcess } from "@/lib/tauri";
import type { PortProcess } from "@/types";
import { formatEndpoint, type PortRow } from "./port-model";

const OWNERSHIP_NOTE = {
  ambiguous:
    "Several processes hold this port and LocalDeck can't prove which project it belongs to, so it isn't attributed to any.",
  unassigned:
    "No working-directory or process-tree evidence ties this port to a project. Global services such as databases usually show up like this.",
  shared: "One project owns this port, but more than one of its components holds it.",
  assigned: null,
} as const;

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="contents">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="min-w-0">{children}</dd>
    </div>
  );
}

function ProcessLine({
  process,
  onForceKill,
}: {
  process: PortProcess;
  onForceKill: (process: PortProcess) => void;
}) {
  const name = process.name ?? "unnamed process";
  return (
    <li className="flex flex-wrap items-center gap-x-3 gap-y-1.5 py-1.5">
      <span className="flex min-w-0 flex-1 items-baseline gap-2">
        <Mono className="truncate font-medium">{name}</Mono>
        <Mono className="text-muted-foreground">pid {process.pid}</Mono>
        {process.startedAt !== null && (
          <span className="text-xs text-muted-foreground">up {formatUptime(process.startedAt)}</span>
        )}
      </span>
      <span className="flex items-center gap-1.5">
        <Button
          variant="outline"
          size="xs"
          aria-label={`Terminate ${name} (pid ${process.pid})`}
          onClick={() =>
            void runAction(
              `Terminate pid ${process.pid}`,
              () => terminateProcess(process.pid, process.startedAt),
              { success: `Asked ${name} (pid ${process.pid}) to terminate` },
            )
          }
        >
          <Square /> Terminate
        </Button>
        <Button
          variant="destructive"
          size="xs"
          aria-label={`Force kill ${name} (pid ${process.pid})`}
          onClick={() => onForceKill(process)}
        >
          <OctagonX /> Force Kill
        </Button>
      </span>
    </li>
  );
}

/** Everything known about one listening endpoint, plus open/copy/reveal/terminate actions. */
export function PortDetail({ row, onReveal }: { row: PortRow; onReveal: (projectId: number) => void }) {
  const { port, project, component } = row;
  const [killTarget, setKillTarget] = useState<PortProcess | null>(null);
  const note = OWNERSHIP_NOTE[port.ownership];

  return (
    <div
      role="region"
      aria-label={`Port ${port.port} details`}
      className="space-y-3 px-3 py-3 text-sm"
    >
      <dl className="grid grid-cols-[6rem_minmax(0,1fr)] items-baseline gap-x-3 gap-y-1.5">
        <Field label="Address">
          <span className="flex flex-wrap gap-x-3 gap-y-0.5">
            {port.addresses.map((address) => (
              <Mono key={address}>{formatEndpoint(address, port.port)}</Mono>
            ))}
          </span>
        </Field>
        <Field label="Protocol">
          <Mono>{port.transport.toUpperCase()}</Mono> · listening
          {port.http === "confirmed" && " · HTTP confirmed"}
          {port.http === "likely" && " · probably HTTP"}
          {port.http === "probing" && " · checking for HTTP…"}
        </Field>
        <Field label="Project">
          {project ? (
            <span>
              <Link
                to={`/projects/${project.id}`}
                className="font-medium underline-offset-2 hover:underline"
              >
                {project.name}
              </Link>
              {component && <span className="text-muted-foreground"> · {component.name}</span>}
              {!component && port.ownership === "shared" && (
                <span className="text-muted-foreground"> · several components</span>
              )}
            </span>
          ) : (
            <span className="text-muted-foreground">—</span>
          )}
        </Field>
        {port.url && (
          <Field label="URL">
            <span className="flex items-center gap-1">
              <Mono className="truncate">{port.url}</Mono>
              <CopyButton value={port.url} label="Copy address" />
            </span>
          </Field>
        )}
      </dl>

      {note && <p className="text-xs text-muted-foreground">{note}</p>}

      <div>
        <h3 className="mb-0.5 text-xs font-medium tracking-wide text-muted-foreground uppercase">
          {port.processes.length > 1 ? `Processes (${port.processes.length})` : "Process"}
        </h3>
        {port.processes.length > 0 ? (
          <ul className="divide-y">
            {port.processes.map((process) => (
              <ProcessLine key={process.pid} process={process} onForceKill={setKillTarget} />
            ))}
          </ul>
        ) : (
          <p className="flex items-start gap-1.5 text-muted-foreground">
            <ShieldAlert aria-hidden className="mt-0.5 size-3.5 shrink-0 text-warning" />
            {port.ownerVisible
              ? "The OS reported this socket without a process."
              : "Owner not visible (limited access). The OS didn't tell LocalDeck which process holds this socket, so it can't be terminated from here."}
          </p>
        )}
      </div>

      {(port.url || project) && (
        <div className="flex flex-wrap items-center gap-2 border-t pt-3">
          {port.url && (
            <>
              <Button
                size="sm"
                onClick={() =>
                  void runAction("Open URL", () => openPortUrl(port.id), {
                    success: `Opened ${port.url}`,
                  })
                }
              >
                <ExternalLink /> Open URL
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  const url = port.url;
                  if (url) {
                    void runAction("Copy URL", () => copyToClipboard(url), { success: "URL copied" });
                  }
                }}
              >
                <Copy /> Copy URL
              </Button>
            </>
          )}
          {project && (
            <Button variant="outline" size="sm" onClick={() => onReveal(project.id)}>
              <FolderSearch /> Reveal project
            </Button>
          )}
        </div>
      )}

      <ConfirmDialog
        open={killTarget !== null}
        onOpenChange={(open) => {
          if (!open) setKillTarget(null);
        }}
        title={`Force kill ${killTarget?.name ?? "process"} (pid ${killTarget?.pid ?? ""})?`}
        description={
          <>
            This sends SIGKILL to the process holding <Mono>:{port.port}</Mono>. It gets no chance to
            clean up, so unsaved work or in-flight writes can be lost. Prefer Terminate unless the
            process ignores it.
          </>
        }
        confirmLabel="Force Kill"
        destructive
        onConfirm={() => {
          const target = killTarget;
          if (!target) return;
          void runAction(
            `Force kill pid ${target.pid}`,
            () => forceKillProcess(target.pid, target.startedAt),
            { success: `Force killed ${target.name ?? "process"} (pid ${target.pid})` },
          );
        }}
      />
    </div>
  );
}
