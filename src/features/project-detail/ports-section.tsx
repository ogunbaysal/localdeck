import { ExternalLink } from "lucide-react";
import { memo, useMemo } from "react";
import { Link } from "react-router";
import { CopyButton } from "@/components/shared/copy-button";
import { Mono } from "@/components/shared/mono";
import { SectionHeader } from "@/components/shared/section-header";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { runAction } from "@/lib/actions";
import { openPortUrl } from "@/lib/tauri";
import type { Component as ProjectComponent, HttpState, PortInfo } from "@/types";

const HTTP_LABEL: Record<HttpState, string> = {
  confirmed: "HTTP",
  likely: "HTTP (likely)",
  probing: "Checking…",
  none: "TCP",
};

const SHARED_HINT = "Shared by several components of this project";

function processLabel(port: PortInfo): string {
  if (!port.ownerVisible || port.processes.length === 0) return "Hidden by OS permissions";
  return port.processes.map((process) => `${process.name ?? "unknown"} (${process.pid})`).join(", ");
}

const PortRow = memo(function PortRow({
  port,
  componentName,
}: {
  port: PortInfo;
  componentName: string | null;
}) {
  const { url } = port;
  const shared = port.ownership === "shared";
  return (
    <TableRow data-port={port.port}>
      <TableCell>
        <Mono className="font-medium tabular-nums">:{port.port}</Mono>
        {shared && (
          <Badge variant="outline" className="ml-1.5" title={SHARED_HINT}>
            Shared
          </Badge>
        )}
      </TableCell>
      <TableCell>
        <Mono className="text-muted-foreground">{port.addresses.join(", ")}</Mono>
      </TableCell>
      <TableCell className={port.ownerVisible ? undefined : "text-muted-foreground"}>
        {port.ownerVisible ? <Mono>{processLabel(port)}</Mono> : processLabel(port)}
      </TableCell>
      <TableCell>{componentName ?? (shared ? "Several components" : <span className="text-muted-foreground">—</span>)}</TableCell>
      <TableCell>{HTTP_LABEL[port.http]}</TableCell>
      <TableCell>
        {url ? (
          <span className="inline-flex items-center gap-0.5">
            <Mono className="max-w-56 truncate" title={url}>
              {url}
            </Mono>
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label={`Open ${url}`}
              title={`Open ${url}`}
              onClick={() =>
                void runAction("Open URL", () => openPortUrl(port.id), { success: `Opened ${url}` })
              }
            >
              <ExternalLink aria-hidden />
            </Button>
            <CopyButton value={url} label={`Copy ${url}`} />
          </span>
        ) : (
          <span className="text-muted-foreground">—</span>
        )}
      </TableCell>
    </TableRow>
  );
});

/** Listening endpoints owned by this project (assigned or shared). */
export const PortsSection = memo(function PortsSection({
  ports,
  components,
}: {
  ports: readonly PortInfo[];
  components: readonly ProjectComponent[];
}) {
  const sorted = useMemo(() => [...ports].sort((a, b) => a.port - b.port || a.id.localeCompare(b.id)), [ports]);
  const names = useMemo(() => new Map(components.map((component) => [component.id, component.name])), [components]);

  return (
    <section aria-label="Ports" className="space-y-2">
      <SectionHeader
        title="Ports"
        count={sorted.length}
        description={
          <>
            Ports this project owns. Unassigned or ambiguous ports are on the{" "}
            <Link to="/ports" className="underline underline-offset-2 hover:text-foreground">
              Ports
            </Link>{" "}
            screen.
          </>
        }
      />
      {sorted.length === 0 ? (
        <p className="rounded-lg border border-dashed px-3 py-4 text-center text-muted-foreground">
          No listening ports
        </p>
      ) : (
        <div className="rounded-lg border">
          <Table>
            <TableHeader>
              <TableRow className="text-xs text-muted-foreground hover:bg-transparent">
                <TableHead>Port</TableHead>
                <TableHead>Address</TableHead>
                <TableHead>Process</TableHead>
                <TableHead>Component</TableHead>
                <TableHead>Type</TableHead>
                <TableHead>URL</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {sorted.map((port) => (
                <PortRow
                  key={port.id}
                  port={port}
                  componentName={port.componentId === null ? null : (names.get(port.componentId) ?? null)}
                />
              ))}
            </TableBody>
          </Table>
        </div>
      )}
    </section>
  );
});
