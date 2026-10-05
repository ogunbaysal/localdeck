import { History, Trash2 } from "lucide-react";
import { memo, useMemo, useState } from "react";
import { Link } from "react-router";
import { ConfirmDialog, EmptyState, ErrorState } from "@/components/shared";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { runAction } from "@/lib/actions";
import { formatRelative, formatTime } from "@/lib/format";
import { clearActivity } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { selectProjects, useRuntimeStore } from "@/stores/runtime-store";
import { useUiStore } from "@/stores/ui-store";
import type { ActivityEvent } from "@/types";
import {
  ACTIVITY_GROUPS,
  ACTIVITY_KINDS,
  filterActivity,
  groupByDay,
  isActivityGroup,
  TONE_CLASS,
} from "./activity-model";
import { useActivity } from "./use-activity";

const ALL_PROJECTS = "all";

const ActivityRow = memo(function ActivityRow({
  event,
  projectName,
  now,
}: {
  event: ActivityEvent;
  /** `undefined` = the project no longer exists; `null` = the event has no project. */
  projectName: string | null | undefined;
  now: number;
}) {
  const kind = ACTIVITY_KINDS[event.kind];
  const Icon = kind.icon;
  return (
    <li
      data-slot="activity-row"
      data-kind={event.kind}
      className="grid grid-cols-[auto_auto_minmax(0,9rem)_minmax(0,1fr)] items-baseline gap-x-3 py-1.5 max-sm:grid-cols-[auto_auto_minmax(0,1fr)]"
    >
      <time
        dateTime={new Date(event.createdAt).toISOString()}
        className="flex w-24 items-baseline gap-1.5 font-mono text-xs tabular-nums"
      >
        <span>{formatTime(event.createdAt)}</span>
        <span className="font-sans text-muted-foreground">{formatRelative(event.createdAt, now)}</span>
      </time>
      <span className={cn("self-center", TONE_CLASS[kind.tone])}>
        <Icon aria-hidden className="size-3.5" />
        <span className="sr-only">{kind.label}</span>
      </span>
      <span className="min-w-0 truncate max-sm:hidden">
        {event.projectId !== null && projectName !== undefined && projectName !== null ? (
          <Link
            to={`/projects/${event.projectId}`}
            className="font-medium underline-offset-2 hover:underline focus-visible:underline"
          >
            {projectName}
          </Link>
        ) : event.projectId !== null ? (
          <span className="text-muted-foreground" title="This project was forgotten or removed">
            Unknown project
          </span>
        ) : (
          <span className="text-muted-foreground">—</span>
        )}
      </span>
      <span className="min-w-0 break-words">{event.message}</span>
    </li>
  );
});

/** Global timeline of everything LocalDeck observed, newest first, grouped by local day. */
export function ActivityPage() {
  const { events, error, loading, reload } = useActivity();
  const projects = useRuntimeStore(selectProjects);
  // Subscribing to `updatedAt` re-renders every scan, which keeps the relative times fresh.
  useRuntimeStore((state) => state.snapshot?.updatedAt);
  const projectId = useUiStore((state) => state.activityProjectId);
  const group = useUiStore((state) => state.activityGroup);
  const setProjectId = useUiStore((state) => state.setActivityProjectId);
  const setGroup = useUiStore((state) => state.setActivityGroup);
  const [confirmClear, setConfirmClear] = useState(false);

  const names = useMemo(() => new Map(projects.map((project) => [project.id, project.name])), [projects]);
  const sortedProjects = useMemo(
    () => [...projects].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" })),
    [projects],
  );
  const visible = useMemo(
    () => (events ? filterActivity(events, { projectId, group }) : []),
    [events, projectId, group],
  );
  const now = Date.now();
  const days = groupByDay(visible, now);

  const clear = async () => {
    const done = await runAction("Clear activity history", async () => {
      await clearActivity();
      return true;
    }, { success: "Activity history cleared" });
    if (done) reload();
  };

  const filtered = projectId !== null || group !== "all";
  const total = events?.length ?? 0;

  return (
    <div className="flex flex-col gap-3">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="flex items-baseline gap-2 text-base font-semibold">
          Activity
          {events !== null && (
            <span className="font-mono text-xs font-normal text-muted-foreground tabular-nums">
              {filtered ? `${visible.length} of ${total}` : total}
            </span>
          )}
        </h1>
        <Button variant="outline" size="sm" disabled={total === 0} onClick={() => setConfirmClear(true)}>
          <Trash2 /> Clear history
        </Button>
      </header>

      <div className="flex flex-wrap items-center gap-2">
        <Select
          value={projectId === null ? ALL_PROJECTS : String(projectId)}
          onValueChange={(value) => setProjectId(value === ALL_PROJECTS ? null : Number(value))}
        >
          <SelectTrigger aria-label="Project" className="h-8 w-48">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={ALL_PROJECTS}>All projects</SelectItem>
            {projectId !== null && !names.has(projectId) && (
              <SelectItem value={String(projectId)}>Unknown project</SelectItem>
            )}
            {sortedProjects.map((project) => (
              <SelectItem key={project.id} value={String(project.id)}>
                {project.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <ToggleGroup
          type="single"
          variant="outline"
          size="sm"
          spacing={0}
          aria-label="Event kind"
          value={group}
          onValueChange={(value) => {
            // Radix reports "" when the pressed item is clicked again: keep the selection.
            if (isActivityGroup(value)) setGroup(value);
          }}
        >
          {ACTIVITY_GROUPS.map(({ value, label }) => (
            <ToggleGroupItem key={value} value={value} className="px-2.5 text-xs">
              {label}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>

      {events === null && loading && (
        <div role="status" aria-label="Loading activity" className="space-y-2">
          {Array.from({ length: 6 }, (_, index) => (
            <Skeleton key={index} className="h-6 w-full" />
          ))}
        </div>
      )}

      {events === null && !loading && error && <ErrorState error={error} onRetry={reload} />}

      {events !== null && error && (
        <p role="alert" className="text-xs text-warning">
          Couldn't refresh activity: {error.message}
        </p>
      )}

      {events !== null && total === 0 && (
        <EmptyState
          icon={History}
          title="No activity yet"
          description="Starts, stops, crashes, port changes and conflicts show up here as LocalDeck sees them."
        />
      )}

      {events !== null && total > 0 && visible.length === 0 && (
        <EmptyState
          title="Nothing matches these filters"
          action={
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                setProjectId(null);
                setGroup("all");
              }}
            >
              Show everything
            </Button>
          }
        />
      )}

      {days.map((day) => (
        <section key={day.key} aria-label={day.label} className="space-y-0.5">
          <h2 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">{day.label}</h2>
          <ul className="divide-y border-y">
            {day.events.map((event) => (
              <ActivityRow
                key={event.id}
                event={event}
                projectName={event.projectId === null ? null : names.get(event.projectId)}
                now={now}
              />
            ))}
          </ul>
        </section>
      ))}

      <ConfirmDialog
        open={confirmClear}
        onOpenChange={setConfirmClear}
        title="Clear activity history?"
        description="This deletes every recorded event for all projects, including the recent activity shown on project pages. New events keep being recorded."
        confirmLabel="Clear history"
        destructive
        onConfirm={clear}
      />
    </div>
  );
}
