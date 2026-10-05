import { FolderSearch, SearchX } from "lucide-react";
import { useMemo } from "react";
import { Link } from "react-router";
import { EmptyState, SectionHeader } from "@/components/shared";
import { Button } from "@/components/ui/button";
import { matchesStatusFilter, matchProject } from "@/lib/search";
import {
  selectIgnoredProjects,
  selectPorts,
  selectSettings,
  selectVisibleProjects,
  useRuntimeStore,
} from "@/stores/runtime-store";
import { useUiStore } from "@/stores/ui-store";
import type { EditorInfo, Project } from "@/types";
import { AddProjectButton } from "./add-project-button";
import { ProjectCard } from "./ProjectCard";
import { ProjectsToolbar } from "./ProjectsToolbar";
import { RememberedProjects } from "./RememberedProjects";
import { useEditors } from "./use-project-actions";

const collator = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });

/** Favourites first, then name (case-insensitive, locale-aware); id keeps the order stable. */
export function compareProjects(a: Project, b: Project): number {
  if (a.favorite !== b.favorite) return a.favorite ? -1 : 1;
  return collator.compare(a.name, b.name) || a.id - b.id;
}

function ProjectList({
  title,
  projects,
  editors,
  defaultEditorId,
}: {
  title: string;
  projects: readonly Project[];
  editors: readonly EditorInfo[] | undefined;
  defaultEditorId: string | null;
}) {
  return (
    <section aria-label={title} className="flex flex-col gap-2">
      <SectionHeader title={title} count={projects.length} />
      <ul className="flex flex-col gap-2">
        {projects.map((project) => (
          <li key={project.id}>
            <ProjectCard project={project} editors={editors} defaultEditorId={defaultEditorId} />
          </li>
        ))}
      </ul>
    </section>
  );
}

/** Home route: running and remembered projects with search and status filters. */
export function ProjectsPage() {
  const snapshotReady = useRuntimeStore((state) => state.snapshot !== null);
  const visible = useRuntimeStore(selectVisibleProjects);
  const ignoredCount = useRuntimeStore((state) => selectIgnoredProjects(state).length);
  const ports = useRuntimeStore(selectPorts);
  const defaultEditorId = useRuntimeStore(selectSettings)?.preferredEditor ?? null;
  const query = useUiStore((state) => state.searchQuery);
  const filter = useUiStore((state) => state.statusFilter);
  const setQuery = useUiStore((state) => state.setSearchQuery);
  const setFilter = useUiStore((state) => state.setStatusFilter);
  const editors = useEditors();

  const filtering = query.trim() !== "" || filter !== "all";

  const { running, stopped, matching } = useMemo(() => {
    const matching = visible
      .filter((project) => matchesStatusFilter(project, filter) && matchProject(project, ports, query))
      .sort(compareProjects);
    return {
      matching,
      running: matching.filter((project) => project.runtimeStatus !== "stopped"),
      stopped: matching.filter((project) => project.runtimeStatus === "stopped"),
    };
  }, [visible, ports, query, filter]);

  const totals = useMemo(() => {
    const live = visible.filter((project) => project.runtimeStatus !== "stopped").length;
    return { live, remembered: visible.length - live };
  }, [visible]);

  if (!snapshotReady) return null;

  const clearFilters = () => {
    setQuery("");
    setFilter("all");
  };

  let body;
  if (visible.length === 0 || (!filtering && running.length === 0)) {
    // Nothing is running (and nothing is being searched for): say so, then list what
    // LocalDeck remembers so it can be opened or started.
    body = (
      <>
        <EmptyState
          icon={FolderSearch}
          title="No running projects"
          description={
            <>
              Start one of your development projects and LocalDeck will detect it automatically.
              {ignoredCount > 0 && (
                <>
                  {" "}
                  {ignoredCount} ignored {ignoredCount === 1 ? "project is" : "projects are"} hidden;
                  manage {ignoredCount === 1 ? "it" : "them"} in{" "}
                  <Link to="/settings" className="underline underline-offset-2">
                    Settings
                  </Link>
                  .
                </>
              )}
            </>
          }
          action={<AddProjectButton variant="secondary" />}
        />
        {stopped.length > 0 && (
          <section aria-label="Previously discovered" className="flex flex-col gap-2">
            <SectionHeader title="Previously discovered" count={stopped.length} />
            <RememberedProjects projects={stopped} editors={editors} defaultEditorId={defaultEditorId} />
          </section>
        )}
      </>
    );
  } else if (matching.length === 0) {
    body = (
      <EmptyState
        icon={SearchX}
        title="No matching projects"
        description="Nothing matches the current search or status filter."
        action={
          <Button variant="outline" size="sm" onClick={clearFilters}>
            Clear filters
          </Button>
        }
      />
    );
  } else if (filter === "attention") {
    body = (
      <ProjectList
        title="Needs attention"
        projects={matching}
        editors={editors}
        defaultEditorId={defaultEditorId}
      />
    );
  } else {
    body = (
      <>
        {running.length > 0 && (
          <ProjectList title="Running" projects={running} editors={editors} defaultEditorId={defaultEditorId} />
        )}
        {stopped.length > 0 && (
          <ProjectList title="Stopped" projects={stopped} editors={editors} defaultEditorId={defaultEditorId} />
        )}
      </>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <SectionHeader
        as="h1"
        title="Projects"
        count={visible.length}
        description={visible.length > 0 ? `${totals.live} running · ${totals.remembered} stopped` : undefined}
        actions={<AddProjectButton />}
      />
      {visible.length > 0 && (
        <ProjectsToolbar query={query} onQueryChange={setQuery} filter={filter} onFilterChange={setFilter} />
      )}
      {body}
    </div>
  );
}
