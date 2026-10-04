import type { PortsFilter } from "@/stores/ui-store";
import type { Component, PortInfo, PortProcess, Project } from "@/types";

/** A listening endpoint joined with the project/component it is *provably* attributed to. */
export interface PortRow {
  port: PortInfo;
  /** Only set for `assigned`/`shared` ownership. Never guessed for ambiguous/unassigned rows. */
  project: Project | undefined;
  /** Only set when the endpoint maps to one specific component of `project`. */
  component: Component | undefined;
}

/** True when the project attribution is backed by evidence (`assigned` or `shared`). */
export const isOwned = (port: PortInfo): boolean =>
  port.ownership === "assigned" || port.ownership === "shared";

/** Rows ordered by port number (then id, so twin endpoints keep a stable order). */
export function buildPortRows(
  ports: readonly PortInfo[],
  projects: readonly Project[],
): PortRow[] {
  const byId = new Map<number, Project>();
  for (const project of projects) byId.set(project.id, project);
  const rows = ports.map<PortRow>((port) => {
    const project =
      isOwned(port) && port.projectId !== null ? byId.get(port.projectId) : undefined;
    const component =
      project && port.componentId !== null
        ? project.components.find((candidate) => candidate.id === port.componentId)
        : undefined;
    return { port, project, component };
  });
  return rows.sort((a, b) => a.port.port - b.port.port || a.port.id.localeCompare(b.port.id));
}

/* ── Search ────────────────────────────────────────────────────────────── */

const PORT_TOKEN =
  /^(?:https?:\/\/)?(?:(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1?\])?:)?(\d{1,5})(?:\/.*)?$/i;

/** Digits typed as a port in `token` (`3000`, `:3000`, `localhost:3000`), else null. */
export function portDigits(token: string): string | null {
  return PORT_TOKEN.exec(token)?.[1] ?? null;
}

/**
 * The port number when the *whole* search text is a port (`3000`, `:3000`,
 * `localhost:3000`, `http://localhost:3000/`); null for anything else.
 */
export function parsePortQuery(text: string): number | null {
  const digits = portDigits(text.trim());
  if (digits === null) return null;
  const value = Number(digits);
  return value >= 1 && value <= 65535 ? value : null;
}

function haystack({ port, project, component }: PortRow): string[] {
  const entries: string[] = [];
  if (project) entries.push(project.name, project.detectedName);
  if (component) {
    entries.push(component.name, component.kind);
    if (component.technology) entries.push(component.technology);
  }
  for (const process of port.processes) {
    if (process.name) entries.push(process.name);
  }
  if (port.url) entries.push(port.url);
  entries.push(...port.addresses);
  return entries.map((entry) => entry.toLowerCase());
}

/**
 * Every whitespace-separated token must match: a port-number prefix (`3000`, `:30`,
 * `localhost:3000`) or a substring of the project, component, process name, URL or
 * address. An empty query matches everything.
 */
export function matchesPortSearch(row: PortRow, query: string): boolean {
  const tokens = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (tokens.length === 0) return true;
  const portText = String(row.port.port);
  const entries = haystack(row);
  return tokens.every((token) => {
    const digits = portDigits(token);
    if (digits !== null && portText.startsWith(digits)) return true;
    return entries.some((entry) => entry.includes(token));
  });
}

/** `assigned` = owned by a project (assigned/shared); `unassigned` = unassigned or ambiguous. */
export function matchesPortsFilter(port: PortInfo, filter: PortsFilter): boolean {
  switch (filter) {
    case "all":
      return true;
    case "http":
      return port.isHttp;
    case "assigned":
      return isOwned(port);
    case "unassigned":
      return !isOwned(port);
  }
}

/* ── Display helpers ───────────────────────────────────────────────────── */

/** `127.0.0.1:3000`, `[::1]:3000`; wildcard addresses read as `*:3000`. */
export function formatEndpoint(address: string, port: number): string {
  if (address === "0.0.0.0" || address === "::") return `*:${port}`;
  return address.includes(":") ? `[${address}]:${port}` : `${address}:${port}`;
}

/** `localhost:3000` for a URL (falls back to the raw text when it doesn't parse). */
export function hostPort(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

/** Distinct processes (by PID) across `rows`, in first-seen order. */
export function uniqueProcesses(rows: readonly PortRow[]): PortProcess[] {
  const seen = new Map<number, PortProcess>();
  for (const { port } of rows) {
    for (const process of port.processes) {
      if (!seen.has(process.pid)) seen.set(process.pid, process);
    }
  }
  return [...seen.values()];
}

/* ── "What owns port N?" ───────────────────────────────────────────────── */

export interface AnswerSegment {
  text: string;
  mono?: boolean;
}

export interface PortAnswer {
  /** `found` = a definite owner; `warning` = ambiguous / not visible / can't tell; `empty` = nothing there. */
  tone: "found" | "warning" | "empty";
  segments: AnswerSegment[];
}

const pidText = (processes: readonly PortProcess[]): string =>
  `${processes.length === 1 ? "pid" : "pids"} ${processes.map((process) => process.pid).join(", ")}`;

function answerForRow(row: PortRow, label: string): PortAnswer {
  const { port, project, component } = row;
  const head: AnswerSegment = { text: `Port ${port.port}${label}`, mono: true };
  const processes = port.processes;
  const named = processes.find((process) => process.name)?.name ?? null;

  if (port.ownership === "ambiguous") {
    const count = processes.length;
    return {
      tone: "warning",
      segments: [
        head,
        {
          text:
            count > 1
              ? ` is held by ${count} processes — ownership ambiguous (${pidText(processes)})`
              : " — ownership ambiguous",
        },
      ],
    };
  }
  if (!port.ownerVisible || processes.length === 0) {
    return {
      tone: "warning",
      segments: [head, { text: " is in use, but its owner isn't visible (limited access)" }],
    };
  }
  const segments: AnswerSegment[] = [head, { text: " → " }];
  if (isOwned(port) && project) {
    segments.push({ text: project.name });
    if (component) segments.push({ text: ` · ${component.name}` });
    else if (port.ownership === "shared") segments.push({ text: " · shared by several components" });
    if (named) segments.push({ text: ` (${named})`, mono: true });
  } else {
    if (named) segments.push({ text: named, mono: true });
    else segments.push({ text: "unnamed process" });
  }
  segments.push({ text: ` · ${pidText(processes)}`, mono: true });
  if (!isOwned(port)) segments.push({ text: " · not attributed to any project" });
  return { tone: "found", segments };
}

/**
 * Immediate answer to "what owns port N?": one line per listening endpoint on that port
 * (twin endpoints on different addresses are reported separately, never merged by guess).
 */
export function answersForPort(
  portNumber: number,
  rows: readonly PortRow[],
  socketsReadable: boolean,
): PortAnswer[] {
  const matches = rows.filter((row) => row.port.port === portNumber);
  if (matches.length === 0) {
    return [
      socketsReadable
        ? {
            tone: "empty",
            segments: [{ text: "Nothing is listening on " }, { text: `:${portNumber}`, mono: true }],
          }
        : {
            tone: "warning",
            segments: [
              { text: "Can't tell what is on " },
              { text: `:${portNumber}`, mono: true },
              { text: " — listening ports can't be read right now" },
            ],
          },
    ];
  }
  return matches.map((row) =>
    answerForRow(
      row,
      matches.length > 1 ? ` (${row.port.addresses.map((a) => formatEndpoint(a, row.port.port)).join(", ")})` : "",
    ),
  );
}
