import { CircleHelp, Network, Search, ShieldAlert, X } from "lucide-react";
import { memo, useCallback, useMemo } from "react";
import { Link, useNavigate } from "react-router";
import { EmptyState } from "@/components/shared";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { describeError } from "@/lib/errors";
import { cn } from "@/lib/utils";
import {
  selectPorts,
  selectProjects,
  selectScanner,
  useRuntimeStore,
} from "@/stores/runtime-store";
import { useUiStore, type PortsFilter } from "@/stores/ui-store";
import type { ErrorPayload, PortInfo } from "@/types";
import { PortDetail } from "./PortDetail";
import {
  answersForPort,
  buildPortRows,
  matchesPortSearch,
  matchesPortsFilter,
  parsePortQuery,
  type PortAnswer,
  type PortRow,
} from "./port-model";

const FILTERS: ReadonlyArray<{ value: PortsFilter; label: string }> = [
  { value: "all", label: "All" },
  { value: "http", label: "HTTP" },
  { value: "assigned", label: "Assigned" },
  { value: "unassigned", label: "Unassigned" },
];

const isPortsFilter = (value: string): value is PortsFilter =>
  FILTERS.some((filter) => filter.value === value);

const AMBIGUOUS_EXPLANATION =
  "Several processes hold this port, or they belong to different projects. LocalDeck won't guess an owner.";

/* ── Cells ─────────────────────────────────────────────────────────────── */

const Dash = () => <span className="text-muted-foreground">—</span>;

function ProjectCell({ row }: { row: PortRow }) {
  const { port, project } = row;
  if (port.ownership === "ambiguous") {
    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <Badge
            tabIndex={0}
            variant="outline"
            className="border-warning/50 bg-warning/10 text-warning"
          >
            ambiguous
          </Badge>
        </TooltipTrigger>
        <TooltipContent>{AMBIGUOUS_EXPLANATION}</TooltipContent>
      </Tooltip>
    );
  }
  if (!project) return <Dash />;
  return (
    <span className="flex min-w-0 items-center gap-1.5">
      <Link
        to={`/projects/${project.id}`}
        onClick={(event) => event.stopPropagation()}
        className="truncate font-medium underline-offset-2 hover:underline focus-visible:underline"
      >
        {project.name}
      </Link>
      {project.ignored && (
        <span className="shrink-0 rounded border px-1 text-[11px] leading-4 text-muted-foreground">
          ignored
        </span>
      )}
    </span>
  );
}

function ComponentCell({ row }: { row: PortRow }) {
  if (row.port.ownership === "shared") {
    return (
      <span className="text-muted-foreground" title="Held by several components of the same project">
        shared
      </span>
    );
  }
  return row.component ? <span className="truncate">{row.component.name}</span> : <Dash />;
}

function ProcessCell({ port }: { port: PortInfo }) {
  const { processes } = port;
  if (processes.length === 0) {
    return port.ownerVisible ? (
      <Dash />
    ) : (
      <span className="inline-flex items-center gap-1 text-muted-foreground">
        <ShieldAlert aria-hidden className="size-3.5 shrink-0 text-warning" />
        <span className="truncate">Owner not visible (limited access)</span>
      </span>
    );
  }
  if (processes.length > 1) {
    return (
      <span
        className="truncate"
        title={processes.map((process) => `${process.name ?? "unnamed"} (${process.pid})`).join(", ")}
      >
        {processes.length} processes
      </span>
    );
  }
  const [process] = processes;
  return (
    <span className="flex min-w-0 items-baseline gap-1.5">
      <span className="truncate font-mono text-[0.92em]">{process.name ?? "unnamed"}</span>
      <span className="shrink-0 font-mono text-[0.92em] text-muted-foreground">{process.pid}</span>
    </span>
  );
}

function TypeBadge({ port }: { port: PortInfo }) {
  if (port.http === "probing") {
    return (
      <Badge variant="outline" className="text-muted-foreground" title="Checking whether this port speaks HTTP">
        probing…
      </Badge>
    );
  }
  if (port.http === "confirmed" || port.http === "likely") {
    return (
      <Badge
        variant="secondary"
        title={port.http === "confirmed" ? "Answered an HTTP request" : "Probably HTTP, not yet confirmed"}
        className={cn(port.http === "likely" && "border-dashed border-border")}
      >
        HTTP
      </Badge>
    );
  }
  return (
    <Badge variant="outline" className="text-muted-foreground">
      TCP
    </Badge>
  );
}

/* ── Rows ──────────────────────────────────────────────────────────────── */

const COLUMNS: ReadonlyArray<{ label: string; width?: string }> = [
  { label: "Port", width: "w-20" },
  { label: "Project", width: "w-[26%]" },
  { label: "Component", width: "w-[18%]" },
  { label: "Process" },
  { label: "Type", width: "w-24" },
];

interface PortTableRowProps {
  row: PortRow;
  selected: boolean;
  onToggle: (portId: string) => void;
  onReveal: (projectId: number) => void;
}

/** Row props that matter for rendering; the 2 s snapshot rebuilds `PortRow`s with the same content. */
function sameRow(a: PortTableRowProps, b: PortTableRowProps): boolean {
  return (
    a.selected === b.selected &&
    a.onToggle === b.onToggle &&
    a.onReveal === b.onReveal &&
    a.row.port === b.row.port &&
    a.row.project === b.row.project &&
    a.row.component === b.row.component
  );
}

const PortTableRow = memo(function PortTableRow({ row, selected, onToggle, onReveal }: PortTableRowProps) {
  const { port } = row;
  const detailId = `port-detail-${port.id}`;
  return (
    <>
      <TableRow
        data-state={selected ? "selected" : undefined}
        data-port-id={port.id}
        className="cursor-pointer"
        onClick={() => onToggle(port.id)}
      >
        <TableCell>
          <button
            type="button"
            aria-expanded={selected}
            aria-controls={selected ? detailId : undefined}
            aria-label={`Port ${port.port}`}
            className="-mx-1 rounded px-1 font-mono font-medium tabular-nums outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
          >
            {port.port}
          </button>
        </TableCell>
        <TableCell className="max-w-0">
          <ProjectCell row={row} />
        </TableCell>
        <TableCell className="max-w-0">
          <ComponentCell row={row} />
        </TableCell>
        <TableCell className="max-w-0">
          <ProcessCell port={port} />
        </TableCell>
        <TableCell>
          <TypeBadge port={port} />
        </TableCell>
      </TableRow>
      {selected && (
        <TableRow id={detailId} className="bg-muted/30 hover:bg-muted/30">
          <TableCell colSpan={COLUMNS.length} className="p-0 whitespace-normal">
            <PortDetail row={row} onReveal={onReveal} />
          </TableCell>
        </TableRow>
      )}
    </>
  );
}, sameRow);

/* ── Answer line ───────────────────────────────────────────────────────── */

function AnswerLine({ answers }: { answers: readonly PortAnswer[] }) {
  return (
    <div
      role="status"
      aria-label="Port owner"
      data-slot="port-answer"
      className="space-y-1 rounded-md border bg-muted/40 px-3 py-2"
    >
      {answers.map((answer, index) => (
        <p
          key={index}
          data-tone={answer.tone}
          className={cn(
            "flex items-start gap-2 text-base font-medium",
            answer.tone === "warning" && "text-warning",
            answer.tone === "empty" && "text-muted-foreground",
          )}
        >
          {answer.tone === "warning" ? (
            <ShieldAlert aria-hidden className="mt-0.5 size-4 shrink-0" />
          ) : (
            <CircleHelp aria-hidden className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
          )}
          <span className="min-w-0 break-words">
            {answer.segments.map((segment, at) => (
              <span key={at} className={cn(segment.mono && "font-mono text-[0.95em]")}>
                {segment.text}
              </span>
            ))}
          </span>
        </p>
      ))}
    </div>
  );
}

/* ── Page ──────────────────────────────────────────────────────────────── */

/** Global diagnostic table of every listening endpoint, answering "what owns port N?" first. */
export function PortsPage() {
  const ports = useRuntimeStore(selectPorts);
  const projects = useRuntimeStore(selectProjects);
  const scanner = useRuntimeStore(selectScanner);
  const search = useUiStore((state) => state.portsSearch);
  const filter = useUiStore((state) => state.portsFilter);
  const selectedPortId = useUiStore((state) => state.selectedPortId);
  const setSearch = useUiStore((state) => state.setPortsSearch);
  const setFilter = useUiStore((state) => state.setPortsFilter);
  const selectPort = useUiStore((state) => state.selectPort);
  const navigate = useNavigate();

  const rows = useMemo(() => buildPortRows(ports, projects), [ports, projects]);
  const visible = useMemo(
    () =>
      rows.filter(
        (row) => matchesPortsFilter(row.port, filter) && matchesPortSearch(row, search),
      ),
    [rows, filter, search],
  );

  const socketsReadable = scanner === null || (scanner.socketsAvailable && !scanner.socketError);
  const queriedPort = parsePortQuery(search);
  const answers = useMemo(
    () => (queriedPort === null ? null : answersForPort(queriedPort, rows, socketsReadable)),
    [queriedPort, rows, socketsReadable],
  );

  const onToggle = useCallback(
    (portId: string) => selectPort(useUiStore.getState().selectedPortId === portId ? null : portId),
    [selectPort],
  );
  const onReveal = useCallback(
    (projectId: number) => void navigate(`/projects/${projectId}`),
    [navigate],
  );

  const clearFilters = () => {
    setSearch("");
    setFilter("all");
  };
  const filtering = search.trim() !== "" || filter !== "all";
  const socketProblem = scanner !== null && !socketsReadable;

  return (
    <section className="flex flex-col gap-3">
      <header>
        <h1 className="flex items-baseline gap-2 text-base font-semibold">
          Ports
          <span className="font-mono text-xs font-normal text-muted-foreground tabular-nums">{rows.length}</span>
        </h1>
        <p className="text-muted-foreground">
          Every listening endpoint and the project that owns it, when that can be proven.
        </p>
      </header>

      <div className="flex flex-wrap items-center gap-2">
        <InputGroup className="h-8 max-w-sm min-w-48 flex-1">
          <InputGroupAddon>
            <Search aria-hidden />
          </InputGroupAddon>
          <InputGroupInput
            type="search"
            aria-label="Search ports"
            placeholder="Port, project, process… (try :3000)"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
          {search !== "" && (
            <InputGroupAddon align="inline-end">
              <button
                type="button"
                aria-label="Clear search"
                className="rounded p-0.5 text-muted-foreground hover:text-foreground"
                onClick={() => setSearch("")}
              >
                <X className="size-3.5" />
              </button>
            </InputGroupAddon>
          )}
        </InputGroup>
        <ToggleGroup
          type="single"
          variant="outline"
          size="sm"
          spacing={0}
          aria-label="Filter ports"
          value={filter}
          onValueChange={(value) => {
            if (isPortsFilter(value)) setFilter(value);
          }}
        >
          {FILTERS.map(({ value, label }) => (
            <ToggleGroupItem key={value} value={value} className="px-2.5 text-xs">
              {label}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>

      {answers && <AnswerLine answers={answers} />}

      {socketProblem && rows.length > 0 && (
        <p className="flex items-start gap-1.5 text-xs text-muted-foreground">
          <ShieldAlert aria-hidden className="mt-0.5 size-3.5 shrink-0 text-warning" />
          Showing the last readable ports; the socket table can't be read right now.
        </p>
      )}

      {rows.length === 0 ? (
        socketProblem ? (
          <SocketsUnavailable error={scanner?.socketError ?? null} />
        ) : (
          <EmptyState
            icon={Network}
            title="Nothing is listening"
            description="No TCP port is in the listening state. Start a dev server and it shows up here."
          />
        )
      ) : visible.length === 0 ? (
        <EmptyState
          icon={Search}
          title="No ports match"
          description={
            filtering
              ? "Nothing listening matches the current search and filter."
              : "Nothing is listening."
          }
          action={
            filtering && (
              <Button variant="outline" size="sm" onClick={clearFilters}>
                Clear search and filter
              </Button>
            )
          }
        />
      ) : (
        <div className="overflow-hidden rounded-md border">
          <Table className="table-fixed">
            <TableHeader className="bg-muted/40">
              <TableRow className="hover:bg-transparent">
                {COLUMNS.map(({ label, width }) => (
                  <TableHead
                    key={label}
                    className={cn(
                      "h-7 text-xs font-medium tracking-wide text-muted-foreground uppercase",
                      width,
                    )}
                  >
                    {label}
                  </TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {visible.map((row) => (
                <PortTableRow
                  key={row.port.id}
                  row={row}
                  selected={row.port.id === selectedPortId}
                  onToggle={onToggle}
                  onReveal={onReveal}
                />
              ))}
            </TableBody>
          </Table>
        </div>
      )}
    </section>
  );
}

function SocketsUnavailable({ error }: { error: ErrorPayload | null }) {
  const description = error ? describeError(error) : null;
  return (
    <EmptyState
      icon={ShieldAlert}
      title="Listening ports unavailable"
      description={
        <>
          <span className="block">
            {description ? description.message : "The OS didn't let LocalDeck read the socket table."}
          </span>
          <span className="mt-1 block text-xs">
            {description?.hint ?? "Projects and processes keep working; only port information is missing."}
          </span>
        </>
      }
    />
  );
}
