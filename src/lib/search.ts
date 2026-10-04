import type { PortInfo, Project } from "@/types";

/** Dashboard status chips. `running` includes partial; `attention` is orthogonal to runtime status. */
export type StatusFilter = "all" | "running" | "stopped" | "attention";

export function matchesStatusFilter(project: Project, filter: StatusFilter): boolean {
  switch (filter) {
    case "all":
      return true;
    case "running":
      return project.runtimeStatus !== "stopped";
    case "stopped":
      return project.runtimeStatus === "stopped";
    case "attention":
      return project.attention.length > 0;
  }
}

/** Hosts that may prefix a port in a query (`localhost:3000`). */
const LOCAL_HOST = /^(?:https?:\/\/)?(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1?\])?:(\d{1,5})(?:\/.*)?$/i;

/** Returns the port prefix typed in `token` (`3000`, `:3000`, `localhost:3000`) or null. */
function portPrefix(token: string): string | null {
  if (/^\d{1,5}$/.test(token)) return token;
  const match = LOCAL_HOST.exec(token);
  return match?.[1] ?? null;
}

function projectHaystack(project: Project): string[] {
  const entries: string[] = [project.name, project.detectedName, project.rootPath];
  if (project.packageManager) entries.push(project.packageManager);
  if (project.primaryUrl) entries.push(project.primaryUrl.url);
  for (const tag of project.stack) entries.push(tag.label, tag.id);
  for (const component of project.components) {
    entries.push(component.name, component.kind);
    if (component.technology) entries.push(component.technology);
  }
  return entries.map((entry) => entry.toLowerCase());
}

function portNumbers(project: Project, ports: readonly PortInfo[]): string[] {
  const numbers = new Set<number>(project.ports);
  for (const port of ports) {
    if (port.projectId === project.id) numbers.add(port.port);
  }
  return Array.from(numbers, String);
}

/**
 * Case-insensitive project search. The query is split on whitespace and every token
 * must match something: the name, root path, URL, stack labels, component
 * name/kind/technology (substring), or a port the project owns (prefix match on
 * `3000`, `:3000`, `localhost:3000`, `http://127.0.0.1:3000`). An empty query matches
 * everything. Ignored projects are not special-cased; callers filter them out first.
 */
export function matchProject(
  project: Project,
  ports: readonly PortInfo[],
  query: string,
): boolean {
  const tokens = query.toLowerCase().split(/\s+/).filter((token) => token !== "" && token !== "~");
  if (tokens.length === 0) return true;

  const haystack = projectHaystack(project);
  const owned = portNumbers(project, ports);

  return tokens.every((token) => {
    const prefix = portPrefix(token);
    if (prefix !== null && owned.some((port) => port.startsWith(prefix))) return true;
    // `~/Projects/app` → `/projects/app` so it matches inside an absolute root path.
    const needle = token.startsWith("~/") ? token.slice(1) : token;
    return haystack.some((entry) => entry.includes(needle));
  });
}
