import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { ConfirmDialog } from "@/components/shared";
import { getProjectActions, type ProjectAction, type ProjectActionId } from "@/lib/project-actions";
import { listEditors } from "@/lib/tauri";
import type { EditorInfo, Project } from "@/types";

/**
 * Editors are the same for every card, so the page fetches them once. A failed lookup
 * only costs the generic "Open in Editor" label; the open action itself still reports
 * any error when clicked.
 */
export function useEditors(): readonly EditorInfo[] | undefined {
  const [editors, setEditors] = useState<readonly EditorInfo[]>();
  useEffect(() => {
    let active = true;
    listEditors().then(
      (list) => {
        if (active) setEditors(list);
      },
      () => {},
    );
    return () => {
      active = false;
    };
  }, []);
  return editors;
}

export interface ProjectActionsApi {
  actions: readonly ProjectAction[];
  find: (id: ProjectActionId) => ProjectAction | undefined;
  /** Runs the action, or asks for confirmation first when the action requires it. */
  trigger: (action: ProjectAction) => void;
  /** Render once per owner; null until a confirmable action is triggered. */
  dialog: ReactNode;
}

/**
 * Actions for one project from the shared `getProjectActions` source, plus the
 * confirmation flow: `confirm` actions (Force Stop, Forget) never run until the user
 * confirms in the dialog.
 */
export function useProjectActions(
  project: Project,
  editors: readonly EditorInfo[] | undefined,
  defaultEditorId: string | null,
): ProjectActionsApi {
  const actions = useMemo(
    () => getProjectActions(project, { editors, defaultEditorId }),
    [project, editors, defaultEditorId],
  );
  const [pendingId, setPendingId] = useState<ProjectActionId | null>(null);
  const pending = pendingId === null ? undefined : actions.find((action) => action.id === pendingId);

  const find = useCallback((id: ProjectActionId) => actions.find((action) => action.id === id), [actions]);
  const trigger = useCallback((action: ProjectAction) => {
    if (action.confirm) setPendingId(action.id);
    else void action.run();
  }, []);

  const dialog = pending?.confirm ? (
    <ConfirmDialog
      open
      onOpenChange={(open) => {
        if (!open) setPendingId(null);
      }}
      title={pending.confirm.title}
      description={pending.confirm.description}
      confirmLabel={pending.confirm.confirmLabel}
      destructive={pending.destructive}
      onConfirm={() => pending.run()}
    />
  ) : null;

  return { actions, find, trigger, dialog };
}
