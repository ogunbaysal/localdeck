import { ChevronRight } from "lucide-react";
import { memo, useCallback, useState } from "react";
import { Mono } from "@/components/shared/mono";
import { SectionHeader } from "@/components/shared/section-header";
import { StatusDot } from "@/components/shared/status-badge";
import { MetricsInline } from "@/components/shared/metrics-inline";
import { Badge } from "@/components/ui/badge";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { runAction } from "@/lib/actions";
import { formatBytes, formatCpu, formatPortList } from "@/lib/format";
import { setComponentKind } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import type { Component as ProjectComponent, ComponentKind, ProcessInfo } from "@/types";
import { COMPONENT_KINDS, kindLabel, LIMITED_ACCESS_HINT, ORIGIN_LABEL } from "./labels";

const AUTO = "auto";

function ComponentStatus({ status }: { status: ProjectComponent["status"] }) {
  if (status === "running") {
    return (
      <span className="inline-flex items-center gap-1.5 text-xs text-success">
        <StatusDot status="running" />
        Running
      </span>
    );
  }
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          tabIndex={0}
          className="inline-flex items-center gap-1.5 rounded-sm text-xs text-warning outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
        >
          <StatusDot status="partial" />
          Limited access
        </span>
      </TooltipTrigger>
      <TooltipContent>{LIMITED_ACCESS_HINT}</TooltipContent>
    </Tooltip>
  );
}

function ProcessTree({ name, processes }: { name: string; processes: readonly ProcessInfo[] }) {
  if (processes.length === 0) {
    return <p className="py-1 pl-6 text-xs text-muted-foreground">No process details available.</p>;
  }
  return (
    <ul aria-label={`Processes of ${name}`} className="mt-1.5 space-y-0.5 pl-6 text-xs">
      {processes.map((process) => (
        <li
          key={process.pid}
          className="flex min-w-0 items-baseline gap-2 tabular-nums"
          style={{ paddingLeft: `${process.depth * 14}px` }}
        >
          <Mono className="text-muted-foreground">{process.pid}</Mono>
          <span className="min-w-0 flex-1 truncate">{process.name}</span>
          <span className="shrink-0 text-muted-foreground">
            {process.accessLevel === "unavailable" ? "—" : formatCpu(process.cpuUsage)}
          </span>
          <span className="w-16 shrink-0 text-right text-muted-foreground">
            {process.accessLevel === "unavailable" ? "—" : formatBytes(process.memoryBytes)}
          </span>
        </li>
      ))}
    </ul>
  );
}

const ComponentRow = memo(function ComponentRow({
  component,
  processes,
  onChangeKind,
}: {
  component: ProjectComponent;
  /** All of the project's processes; filtered by `componentId` only when expanded. */
  processes: readonly ProcessInfo[];
  onChangeKind: (componentKey: string, kind: ComponentKind | null) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const { name, kind, detectedKind, kindOverridden, technology, origin, command, ports, metrics } = component;

  return (
    <li className="px-3 py-2" data-component={component.key}>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
        <div className="flex min-w-0 flex-1 basis-64 items-center gap-1.5">
          <button
            type="button"
            aria-expanded={expanded}
            aria-label={`${expanded ? "Hide" : "Show"} processes of ${name}`}
            className="-ml-1 inline-flex size-5 shrink-0 items-center justify-center rounded-sm text-muted-foreground outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring/50"
            onClick={() => setExpanded((open) => !open)}
          >
            <ChevronRight aria-hidden className={cn("size-3.5 transition-transform", expanded && "rotate-90")} />
          </button>
          <span className="min-w-0 truncate font-medium">{name}</span>
          <Badge variant="secondary">{kindLabel(kind)}</Badge>
          {kindOverridden && (
            <span
              className="text-xs text-muted-foreground"
              title={`Detected as ${kindLabel(detectedKind)}; you changed it`}
            >
              corrected
            </span>
          )}
          {technology && <span className="truncate text-muted-foreground">{technology}</span>}
        </div>

        <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
          <Badge variant="outline" className={cn(origin === "localDeckManaged" && "border-primary/40 text-primary")}>
            {ORIGIN_LABEL[origin]}
          </Badge>
          <ComponentStatus status={component.status} />
          {ports.length > 0 && <Mono className="text-xs tabular-nums">{formatPortList(ports)}</Mono>}
          <MetricsInline metrics={metrics} />
          <Select
            value={kindOverridden ? kind : AUTO}
            onValueChange={(value) => onChangeKind(component.key, value === AUTO ? null : (value as ComponentKind))}
          >
            <SelectTrigger size="sm" aria-label={`Change type of ${name}`} className="text-xs">
              <SelectValue>Change type</SelectValue>
            </SelectTrigger>
            <SelectContent position="popper" align="end">
              <SelectItem value={AUTO}>Auto (detected: {kindLabel(detectedKind)})</SelectItem>
              <SelectSeparator />
              {COMPONENT_KINDS.map(({ kind: option, label }) => (
                <SelectItem key={option} value={option}>
                  {label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>

      {command && (
        <p className="mt-0.5 pl-6 text-xs text-muted-foreground">
          <Mono className="block truncate" title={command}>
            {command}
          </Mono>
        </p>
      )}

      {expanded && (
        <ProcessTree name={name} processes={processes.filter((process) => process.componentId === component.id)} />
      )}
    </li>
  );
});

export const ComponentsSection = memo(function ComponentsSection({
  projectId,
  components,
  processes,
}: {
  projectId: number;
  components: readonly ProjectComponent[];
  processes: readonly ProcessInfo[];
}) {
  const changeKind = useCallback(
    (componentKey: string, kind: ComponentKind | null) => {
      void runAction("Change type", () => setComponentKind(projectId, componentKey, kind), {
        success: kind === null ? "Using the detected type" : `Type set to ${kindLabel(kind)}`,
      });
    },
    [projectId],
  );

  return (
    <section aria-label="Components" className="space-y-2">
      <SectionHeader title="Components" count={components.length} />
      {components.length === 0 ? (
        <p className="rounded-lg border border-dashed px-3 py-4 text-center text-muted-foreground">
          No running components
        </p>
      ) : (
        <ul className="divide-y rounded-lg border">
          {components.map((component) => (
            <ComponentRow key={component.id} component={component} processes={processes} onChangeKind={changeKind} />
          ))}
        </ul>
      )}
    </section>
  );
});
