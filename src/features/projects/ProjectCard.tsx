import { ExternalLink, OctagonX, Play, Square, Star, TriangleAlert, X } from "lucide-react";
import { memo, type ReactNode } from "react";
import { Link } from "react-router";
import { MetricsInline, Mono, PortChips, StatusBadge } from "@/components/shared";
import { Button } from "@/components/ui/button";
import { runAction } from "@/lib/actions";
import { ellipsizeMiddle } from "@/lib/format";
import type { ProjectAction } from "@/lib/project-actions";
import { dismissAttention } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import type { EditorInfo, Project } from "@/types";
import { ProjectActionsMenu } from "./ProjectActionsMenu";
import { useProjectActions } from "./use-project-actions";

const MAX_STACK_LABELS = 3;
const MAX_PATH_CHARS = 64;

/** `Vite · React · Tauri`: the first few stack labels only. */
export function summarizeStack(project: Project): string {
  return project.stack
    .slice(0, MAX_STACK_LABELS)
    .map((tag) => tag.label)
    .join(" · ");
}

function FavoriteButton({ project, action }: { project: Project; action: ProjectAction | undefined }) {
  const label = `${project.favorite ? "Unpin" : "Pin"} ${project.name}`;
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-xs"
      aria-label={label}
      title={label}
      disabled={!action}
      onClick={() => void action?.run()}
      className={cn(
        "text-muted-foreground",
        project.favorite
          ? "text-warning"
          : "opacity-0 group-focus-within/card:opacity-100 group-hover/card:opacity-100 focus-visible:opacity-100",
      )}
    >
      <Star className={cn(project.favorite && "fill-current")} />
    </Button>
  );
}

function AttentionLine({ project }: { project: Project }) {
  const [item, ...rest] = project.attention;
  if (!item) return null;
  return (
    <div className="flex items-start gap-1.5 text-xs text-warning">
      <TriangleAlert aria-hidden className="mt-px size-3.5 shrink-0" />
      <span className="min-w-0">
        {item.message}
        {rest.length > 0 && <span className="text-muted-foreground"> · +{rest.length} more</span>}
      </span>
      {item.dismissible && (
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          aria-label={`Dismiss: ${item.message}`}
          title="Dismiss"
          className="-my-0.5 size-5 text-muted-foreground"
          onClick={() =>
            void runAction("Dismiss", () => dismissAttention(project.id, item.id), { success: false })
          }
        >
          <X />
        </Button>
      )}
    </div>
  );
}

/** The one primary action for the project's current state, plus the quiet Stop / Force Stop. */
function CardActions({
  project,
  find,
  trigger,
}: {
  project: Project;
  find: (id: ProjectAction["id"]) => ProjectAction | undefined;
  trigger: (action: ProjectAction) => void;
}) {
  const { name } = project;
  const live = project.runtimeStatus !== "stopped";
  const open = find("open-url");
  const start = find("start");
  // A pending graceful stop offers Force Stop in place of Stop.
  const stopLike = project.stopPending ? find("force-stop") : find("stop");
  const stopLabel = project.stopPending ? "Force Stop" : "Stop";

  const stopButton = (primary: boolean): ReactNode =>
    stopLike && (
      <Button
        type="button"
        size="sm"
        variant={project.stopPending ? "destructive" : primary ? "default" : "ghost"}
        aria-label={`${stopLabel} ${name}`}
        onClick={() => trigger(stopLike)}
        className={cn(!primary && !project.stopPending && "text-muted-foreground")}
      >
        {project.stopPending ? <OctagonX /> : <Square />}
        {stopLabel}
      </Button>
    );

  if (live && project.primaryUrl && open) {
    return (
      <>
        <Button
          type="button"
          size="sm"
          aria-label={`Open ${name}`}
          title={project.primaryUrl.url}
          onClick={() => trigger(open)}
        >
          <ExternalLink />
          Open
        </Button>
        {stopButton(false)}
      </>
    );
  }
  if (live) return stopButton(true);
  if (start && !start.disabled) {
    return (
      <Button type="button" size="sm" aria-label={`Start ${name}`} onClick={() => trigger(start)}>
        <Play />
        Start
      </Button>
    );
  }
  return (
    <Button asChild size="sm" variant="outline">
      <Link to={`/projects/${project.id}`} aria-label={`Details for ${name}`}>
        Details
      </Link>
    </Button>
  );
}

export interface ProjectCardProps {
  project: Project;
  editors?: readonly EditorInfo[];
  defaultEditorId?: string | null;
}

/**
 * Compact dashboard row for one project. Memoised: `project` keeps its identity across
 * 2 s snapshots unless it really changed (structural sharing in the runtime store).
 */
export const ProjectCard = memo(function ProjectCard({
  project,
  editors,
  defaultEditorId = null,
}: ProjectCardProps) {
  const { actions, find, trigger, dialog } = useProjectActions(project, editors, defaultEditorId);
  const live = project.runtimeStatus !== "stopped";
  const stack = summarizeStack(project);
  const hasPorts = live && project.ports.length > 0;

  return (
    <article
      aria-label={project.name}
      data-status={project.runtimeStatus}
      data-attention={project.attention.length > 0 || undefined}
      className="group/card flex items-start gap-3 rounded-md border bg-card px-3 py-2.5 text-card-foreground"
    >
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <div className="flex min-w-0 items-center gap-2">
          <Link
            to={`/projects/${project.id}`}
            className="min-w-0 truncate rounded-sm font-medium outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring/50"
          >
            {project.name}
          </Link>
          <FavoriteButton project={project} action={find("favorite")} />
          <StatusBadge
            status={project.runtimeStatus}
            stopPending={project.stopPending}
            attention={project.attention.length > 0}
            className="shrink-0"
          />
        </div>
        {(stack || hasPorts || live) && (
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
            {stack && <span className="text-muted-foreground">{stack}</span>}
            {hasPorts && <PortChips ports={project.ports} max={4} />}
            {live && <MetricsInline metrics={project.metrics} />}
          </div>
        )}
        <Mono className="truncate text-xs text-muted-foreground" title={project.rootPath}>
          {ellipsizeMiddle(project.rootPath, MAX_PATH_CHARS)}
        </Mono>
        <AttentionLine project={project} />
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <CardActions project={project} find={find} trigger={trigger} />
        <ProjectActionsMenu
          projectId={project.id}
          projectName={project.name}
          actions={actions}
          onSelect={trigger}
        />
      </div>
      {dialog}
    </article>
  );
});
