import { SearchX } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { EmptyState } from "@/components/shared/empty-state";
import { getProjectActions } from "@/lib/project-actions";
import { getSnapshot } from "@/lib/tauri";
import {
  selectPortsForProject,
  selectProjectById,
  selectSettings,
  useRuntimeStore,
} from "@/stores/runtime-store";
import type { Project } from "@/types";
import { ActivitySection } from "./activity-section";
import { AttentionList } from "./attention-list";
import { ComponentsSection } from "./components-section";
import { OverviewSection } from "./overview-section";
import { PortsSection } from "./ports-section";
import { ProcessesSection } from "./processes-section";
import { ProjectHeader } from "./project-header";
import { ProjectSettingsDialog } from "./project-settings-dialog";
import { useEditors } from "./use-editors";

function ProjectNotFound() {
  return (
    <EmptyState
      icon={SearchX}
      title="Project not found"
      description="It may have been forgotten, or the link is out of date."
      action={
        <Link to="/" className="text-sm underline underline-offset-2 hover:text-foreground">
          ← Projects
        </Link>
      }
    />
  );
}

function ProjectDetail({ project }: { project: Project }) {
  const navigate = useNavigate();
  const ports = useRuntimeStore(selectPortsForProject(project.id));
  const defaultEditorId = useRuntimeStore(selectSettings)?.preferredEditor ?? null;
  const { editors } = useEditors();
  const [settingsOpen, setSettingsOpen] = useState(false);

  const actions = useMemo(
    () => getProjectActions(project, { editors: editors ?? undefined, defaultEditorId }),
    [project, editors, defaultEditorId],
  );
  const openSettings = useCallback(() => setSettingsOpen(true), []);

  // `run()` of the Forget action never rejects, so ask the backend whether the project is
  // really gone before leaving; a failed forget keeps the user here (its toast already showed).
  const afterForget = useCallback(() => {
    getSnapshot().then(
      (snapshot) => {
        if (snapshot.projects.some((candidate) => candidate.id === project.id)) return;
        useRuntimeStore.getState().setSnapshot(snapshot);
        void navigate("/", { replace: true });
      },
      () => {
        // The next runtime update shows the "Project not found" state if the forget went through.
      },
    );
  }, [navigate, project.id]);

  return (
    <div className="flex flex-col gap-5">
      <ProjectHeader
        project={project}
        actions={actions}
        onOpenSettings={openSettings}
        onForgotten={afterForget}
      />
      <AttentionList projectId={project.id} items={project.attention} />
      <OverviewSection project={project} ports={ports} actions={actions} onChooseCommand={openSettings} />
      <ComponentsSection projectId={project.id} components={project.components} processes={project.processes} />
      <PortsSection ports={ports} components={project.components} />
      <ActivitySection events={project.recentActivity} />
      <ProcessesSection projectId={project.id} processes={project.processes} />
      <ProjectSettingsDialog project={project} open={settingsOpen} onOpenChange={setSettingsOpen} />
    </div>
  );
}

/** `/projects/:projectId`: one scrollable, project-first page. */
export function ProjectDetailPage() {
  const { projectId: param } = useParams<{ projectId: string }>();
  const projectId = param !== undefined && /^\d+$/.test(param) ? Number(param) : null;
  const project = useRuntimeStore(selectProjectById(projectId));
  if (!project) return <ProjectNotFound />;
  return <ProjectDetail key={project.id} project={project} />;
}
