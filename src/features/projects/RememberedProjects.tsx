import { Play } from "lucide-react";
import { memo } from "react";
import { Link } from "react-router";
import { Mono, StatusDot } from "@/components/shared";
import { Button } from "@/components/ui/button";
import { ellipsizeMiddle } from "@/lib/format";
import type { EditorInfo, Project } from "@/types";
import { summarizeStack } from "./ProjectCard";
import { useProjectActions } from "./use-project-actions";

const MAX_PATH_CHARS = 56;

/**
 * One-line row for a remembered (stopped) project: the row opens the detail page, and a
 * Start button appears when LocalDeck knows how to start it.
 */
const RememberedRow = memo(function RememberedRow({
  project,
  editors,
  defaultEditorId,
}: {
  project: Project;
  editors?: readonly EditorInfo[];
  defaultEditorId: string | null;
}) {
  const { find, trigger } = useProjectActions(project, editors, defaultEditorId);
  const start = find("start");
  const stack = summarizeStack(project);

  return (
    <li
      data-slot="remembered-row"
      className="relative flex items-center gap-3 rounded-md border bg-card px-3 py-2 hover:bg-accent/40"
    >
      <StatusDot status="stopped" />
      <div className="flex min-w-0 flex-1 flex-wrap items-baseline gap-x-3">
        <Link
          to={`/projects/${project.id}`}
          className="rounded-sm font-medium outline-none after:absolute after:inset-0 focus-visible:after:ring-2 focus-visible:after:ring-ring/50 focus-visible:after:rounded-md"
        >
          {project.name}
        </Link>
        {stack && <span className="text-xs text-muted-foreground">{stack}</span>}
        <Mono className="min-w-0 truncate text-xs text-muted-foreground" title={project.rootPath}>
          {ellipsizeMiddle(project.rootPath, MAX_PATH_CHARS)}
        </Mono>
      </div>
      {start && !start.disabled && (
        <Button
          type="button"
          size="sm"
          variant="outline"
          aria-label={`Start ${project.name}`}
          className="relative z-10"
          onClick={() => trigger(start)}
        >
          <Play />
          Start
        </Button>
      )}
    </li>
  );
});

/** “Previously discovered” list shown under the empty state when nothing is running. */
export function RememberedProjects({
  projects,
  editors,
  defaultEditorId,
}: {
  projects: readonly Project[];
  editors?: readonly EditorInfo[];
  defaultEditorId: string | null;
}) {
  return (
    <ul className="flex flex-col gap-1.5" aria-label="Previously discovered projects">
      {projects.map((project) => (
        <RememberedRow
          key={project.id}
          project={project}
          editors={editors}
          defaultEditorId={defaultEditorId}
        />
      ))}
    </ul>
  );
}
