import { Eye } from "lucide-react";
import { Mono } from "@/components/shared";
import { Button } from "@/components/ui/button";
import { getProjectActions } from "@/lib/project-actions";
import { selectIgnoredProjects, useRuntimeStore } from "@/stores/runtime-store";
import { SettingsSection } from "./settings-layout";

/** Ignored projects stay hidden from Projects, search and the palette until un-ignored here. */
export function IgnoredProjectsSection() {
  const ignored = useRuntimeStore(selectIgnoredProjects);

  return (
    <SettingsSection
      title="Ignored projects"
      description="Hidden from Projects, search and the command palette. LocalDeck keeps them so you can bring them back."
    >
      {ignored.length === 0 ? (
        <p className="py-2.5 text-muted-foreground">No ignored projects.</p>
      ) : (
        <ul className="divide-y">
          {ignored.map((project) => (
            <li key={project.id} className="flex items-center justify-between gap-3 py-2">
              <div className="min-w-0">
                <div className="truncate font-medium">{project.name}</div>
                <Mono className="block truncate text-xs text-muted-foreground" title={project.rootPath}>
                  {project.rootPath}
                </Mono>
              </div>
              <Button
                variant="outline"
                size="sm"
                aria-label={`Stop ignoring ${project.name}`}
                onClick={() =>
                  void getProjectActions(project)
                    .find((action) => action.id === "ignore")
                    ?.run()
                }
              >
                <Eye /> Stop ignoring
              </Button>
            </li>
          ))}
        </ul>
      )}
    </SettingsSection>
  );
}
