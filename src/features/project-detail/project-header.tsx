import { Ellipsis, ExternalLink, FolderOpen, Settings2 } from "lucide-react";
import { Fragment, useState } from "react";
import { Link } from "react-router";
import { ConfirmDialog } from "@/components/shared/confirm-dialog";
import { CopyButton } from "@/components/shared/copy-button";
import { Mono } from "@/components/shared/mono";
import { StatusBadge } from "@/components/shared/status-badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { ProjectAction, ProjectActionGroup, ProjectActionId } from "@/lib/project-actions";
import type { Project } from "@/types";
import { ActionButton } from "./action-button";
import { ROOT_KIND_LABEL } from "./labels";

/** Shown as header buttons (per state) or beside the path, so they stay out of the overflow menu. */
const ALWAYS_OUTSIDE_MENU: ReadonlySet<ProjectActionId> = new Set([
  "start",
  "restart",
  "stop",
  "force-stop",
  "open-folder",
]);

const GROUP_ORDER: readonly ProjectActionGroup[] = ["open", "lifecycle", "manage"];

function stackLine(project: Project): string {
  const parts = project.stack.map((tag) => tag.label);
  if (project.packageManager) parts.push(project.packageManager);
  return parts.join(" · ");
}

export function ProjectHeader({
  project,
  actions,
  onOpenSettings,
  onForgotten,
}: {
  project: Project;
  actions: readonly ProjectAction[];
  onOpenSettings: () => void;
  /** Called after the Forget action's `run()` settles (success is checked by the caller). */
  onForgotten: () => void;
}) {
  const [confirmId, setConfirmId] = useState<ProjectActionId | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);

  const byId = (id: ProjectActionId) => actions.find((action) => action.id === id);
  const confirming = confirmId ? byId(confirmId) : undefined;

  const invoke = (action: ProjectAction | undefined) => {
    if (!action || action.disabled) return;
    if (action.confirm) {
      setConfirmId(action.id);
      setConfirmOpen(true);
      return;
    }
    void action.run();
  };

  const confirmed = async () => {
    if (!confirming) return;
    const { id, run } = confirming;
    await run();
    if (id === "forget") onForgotten();
  };

  const stopped = project.runtimeStatus === "stopped";
  const start = byId("start");
  const openUrl = byId("open-url");
  const restart = byId("restart");
  const stop = byId("stop");
  const forceStop = byId("force-stop");
  const openFolder = byId("open-folder");
  const showOpen = !stopped && project.primaryUrl !== null && openUrl !== undefined;

  const menuActions = actions.filter(
    (action) => !ALWAYS_OUTSIDE_MENU.has(action.id) && !(showOpen && action.id === "open-url"),
  );
  const groups = GROUP_ORDER.map((group) => menuActions.filter((action) => action.group === group)).filter(
    (group) => group.length > 0,
  );

  const stack = stackLine(project);

  return (
    <header className="space-y-2">
      <Link
        to="/"
        className="inline-flex text-xs text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50 rounded-sm"
      >
        ← Projects
      </Link>

      <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-3">
        <div className="min-w-0 space-y-1">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <h1 className="min-w-0 truncate text-lg font-semibold tracking-tight">{project.name}</h1>
            <StatusBadge
              status={project.runtimeStatus}
              stopPending={project.stopPending}
              attention={project.attention.length > 0}
            />
          </div>
          {stack && <p className="text-muted-foreground">{stack}</p>}
          <div className="flex min-w-0 flex-wrap items-center gap-x-1 gap-y-0.5">
            <Mono className="min-w-0 break-all text-muted-foreground">{project.rootPath}</Mono>
            <CopyButton value={project.rootPath} label="Copy path" />
            {openFolder && (
              <Button
                type="button"
                variant="ghost"
                size="xs"
                className="text-muted-foreground"
                onClick={() => invoke(openFolder)}
              >
                <FolderOpen aria-hidden />
                Open folder
              </Button>
            )}
            <span className="text-xs text-muted-foreground/80">· {ROOT_KIND_LABEL[project.rootKind]}</span>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-1.5" role="group" aria-label="Project actions">
          {stopped && start && (
            <ActionButton
              label="Start"
              icon={start.icon}
              variant="default"
              disabled={start.disabled}
              disabledReason={start.disabledReason}
              onClick={() => invoke(start)}
            />
          )}
          {showOpen && openUrl && (
            <ActionButton
              label="Open"
              icon={ExternalLink}
              variant="default"
              disabled={openUrl.disabled}
              disabledReason={openUrl.disabledReason}
              onClick={() => invoke(openUrl)}
            />
          )}
          {!stopped && restart && (
            <ActionButton
              label="Restart"
              icon={restart.icon}
              disabled={restart.disabled}
              disabledReason={restart.disabledReason}
              onClick={() => invoke(restart)}
            />
          )}
          {!stopped && stop && (
            <ActionButton
              label="Stop"
              icon={stop.icon}
              disabled={stop.disabled}
              disabledReason={stop.disabledReason}
              onClick={() => invoke(stop)}
            />
          )}
          {forceStop && (
            <ActionButton
              label="Force Stop"
              icon={forceStop.icon}
              variant="destructive"
              disabled={forceStop.disabled}
              disabledReason={forceStop.disabledReason}
              onClick={() => invoke(forceStop)}
            />
          )}

          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button type="button" variant="outline" size="icon-sm" aria-label="More actions">
                <Ellipsis aria-hidden />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-56">
              {groups.map((group, index) => (
                <Fragment key={group[0].group}>
                  {index > 0 && <DropdownMenuSeparator />}
                  {group.map((action) => (
                    <DropdownMenuItem
                      key={action.id}
                      variant={action.destructive ? "destructive" : "default"}
                      disabled={action.disabled}
                      title={action.disabled ? action.disabledReason : undefined}
                      onSelect={() => invoke(action)}
                    >
                      <action.icon aria-hidden />
                      <span className="min-w-0 flex-1 truncate">{action.label}</span>
                      {action.disabled && action.disabledReason && (
                        <span className="sr-only">{action.disabledReason}</span>
                      )}
                    </DropdownMenuItem>
                  ))}
                </Fragment>
              ))}
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={onOpenSettings}>
                <Settings2 aria-hidden />
                Project settings…
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>

      {confirming?.confirm && (
        <ConfirmDialog
          open={confirmOpen}
          onOpenChange={setConfirmOpen}
          title={confirming.confirm.title}
          description={confirming.confirm.description}
          confirmLabel={confirming.confirm.confirmLabel}
          destructive={confirming.destructive}
          onConfirm={confirmed}
        />
      )}
    </header>
  );
}
