import { ExternalLink } from "lucide-react";
import type { ReactNode } from "react";
import { CopyButton } from "@/components/shared/copy-button";
import { Mono } from "@/components/shared/mono";
import { SectionHeader } from "@/components/shared/section-header";
import { Button } from "@/components/ui/button";
import { runAction } from "@/lib/actions";
import { formatBytes, formatCpu, formatPort, formatUptime } from "@/lib/format";
import type { ProjectAction } from "@/lib/project-actions";
import { openPortUrl } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import type { PortInfo, Project } from "@/types";
import { useNow } from "./use-now";
import { URL_SOURCE_LABEL } from "./labels";

const MAX_PORT_CHIPS = 8;

const chipClass =
  "inline-flex h-5 items-center rounded-md border bg-muted/50 px-1.5 font-mono text-xs tabular-nums text-foreground";

function Metric({ label, children, className }: { label: string; children: ReactNode; className?: string }) {
  return (
    <div className={cn("min-w-0 space-y-0.5", className)}>
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="flex min-h-5 min-w-0 flex-wrap items-center gap-1 tabular-nums">{children}</dd>
    </div>
  );
}

/** Port chip: a button that opens the endpoint when it is a confirmed/likely HTTP port, plain otherwise. */
function PortChip({ port, endpoint }: { port: number; endpoint: PortInfo | undefined }) {
  if (!endpoint?.isHttp || endpoint.url === null) {
    return <span className={chipClass}>{formatPort(port)}</span>;
  }
  const { id, url } = endpoint;
  return (
    <button
      type="button"
      title={`Open ${url}`}
      aria-label={`Open ${url}`}
      className={cn(chipClass, "outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring/50")}
      onClick={() => void runAction("Open URL", () => openPortUrl(id), { success: `Opened ${url}` })}
    >
      {formatPort(port)}
    </button>
  );
}

/** Uptime of the project = age of its oldest component root process. */
function oldestRootStart(project: Project): number | null {
  let oldest: number | null = null;
  for (const component of project.components) {
    const root = project.processes.find((process) => process.pid === component.rootPid);
    if (root?.startedAt != null && (oldest === null || root.startedAt < oldest)) oldest = root.startedAt;
  }
  return oldest;
}

function Uptime({ startedAt }: { startedAt: number | null }) {
  const now = useNow(1000);
  return <>{formatUptime(startedAt, now)}</>;
}

export function OverviewSection({
  project,
  ports,
  actions,
  onChooseCommand,
}: {
  project: Project;
  /** Ports owned by this project (for chip → URL lookup). */
  ports: readonly PortInfo[];
  actions: readonly ProjectAction[];
  onChooseCommand: () => void;
}) {
  const { metrics, primaryUrl, primaryCommand } = project;
  const startedAt = oldestRootStart(project);
  const openUrl = actions.find((action) => action.id === "open-url");
  const shownPorts = project.ports.slice(0, MAX_PORT_CHIPS);
  const hiddenPorts = project.ports.slice(MAX_PORT_CHIPS);

  return (
    <section aria-label="Overview" className="space-y-2">
      <SectionHeader title="Overview" />
      <dl className="grid grid-cols-[repeat(auto-fill,minmax(7.5rem,1fr))] gap-x-4 gap-y-3 rounded-lg border p-3">
        <Metric label="Ports">
          {project.ports.length === 0 ? (
            <span className="text-muted-foreground">—</span>
          ) : (
            <>
              {shownPorts.map((port) => (
                <PortChip
                  key={port}
                  port={port}
                  endpoint={ports.find((candidate) => candidate.port === port && candidate.isHttp) ?? ports.find((candidate) => candidate.port === port)}
                />
              ))}
              {hiddenPorts.length > 0 && (
                <span className={cn(chipClass, "text-muted-foreground")} title={hiddenPorts.map(formatPort).join(" ")}>
                  +{hiddenPorts.length}
                </span>
              )}
            </>
          )}
        </Metric>
        <Metric label="CPU">{formatCpu(metrics.cpuPercent)}</Metric>
        <Metric label="Memory">{formatBytes(metrics.memoryBytes)}</Metric>
        <Metric label="Processes">{metrics.processCount}</Metric>
        <Metric label="Uptime">
          <Uptime startedAt={startedAt} />
        </Metric>

        <Metric label="Primary URL" className="col-span-full sm:col-span-2">
          {primaryUrl ? (
            <>
              <Mono className="min-w-0 truncate" title={primaryUrl.url}>
                {primaryUrl.url}
              </Mono>
              <span className="text-xs text-muted-foreground">{URL_SOURCE_LABEL[primaryUrl.source]}</span>
              {openUrl && (
                <Button
                  type="button"
                  variant="ghost"
                  size="xs"
                  disabled={openUrl.disabled}
                  onClick={() => void openUrl.run()}
                >
                  <ExternalLink aria-hidden />
                  Open URL
                </Button>
              )}
              <CopyButton value={primaryUrl.url} label="Copy URL" />
            </>
          ) : (
            <span className="text-muted-foreground">No URL detected yet</span>
          )}
        </Metric>
        <Metric label="Start command" className="col-span-full sm:col-span-2">
          {primaryCommand ? (
            <>
              <Mono className="min-w-0 truncate" title={primaryCommand.label}>
                {primaryCommand.label}
              </Mono>
              <Button type="button" variant="link" size="xs" className="h-auto px-1" onClick={onChooseCommand}>
                Change…
              </Button>
            </>
          ) : (
            <>
              <span className="text-muted-foreground">Not set</span>
              <Button type="button" variant="link" size="xs" className="h-auto px-1" onClick={onChooseCommand}>
                Choose…
              </Button>
            </>
          )}
        </Metric>
      </dl>
    </section>
  );
}
