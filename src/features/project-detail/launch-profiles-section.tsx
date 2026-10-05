import { Pencil, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { ConfirmDialog } from "@/components/shared/confirm-dialog";
import { Mono } from "@/components/shared/mono";
import { SectionHeader } from "@/components/shared/section-header";
import { Button } from "@/components/ui/button";
import { runAction } from "@/lib/actions";
import { getSnapshot, setLaunchProfiles } from "@/lib/tauri";
import { useRuntimeStore } from "@/stores/runtime-store";
import type { LaunchProfile, Project } from "@/types";
import { LaunchProfileDialog, MAX_PROFILES, toProfileInput } from "./launch-profile-dialog";

/** Pull a fresh snapshot so the list reflects the change before the next 2 s scan. Best effort. */
function refreshSnapshot(): void {
  getSnapshot().then(
    (snapshot) => useRuntimeStore.getState().setSnapshot(snapshot),
    () => {
      // The next runtime update delivers the change.
    },
  );
}

/** Named sets of commands that start together; managed here, started from the header menu. */
export function LaunchProfilesSection({ project }: { project: Project }) {
  const [editing, setEditing] = useState<LaunchProfile | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [deleting, setDeleting] = useState<LaunchProfile | null>(null);
  const [deleteOpen, setDeleteOpen] = useState(false);

  const profiles = project.launchProfiles;
  const full = profiles.length >= MAX_PROFILES;

  const openEditor = (profile: LaunchProfile | null) => {
    setEditing(profile);
    setEditorOpen(true);
  };

  const confirmDelete = async () => {
    if (!deleting) return;
    const target = deleting;
    const remaining = profiles.filter((profile) => profile.id !== target.id).map(toProfileInput);
    const done = await runAction(
      `Delete profile ${target.name}`,
      async () => {
        await setLaunchProfiles(project.id, remaining);
        return true;
      },
      { success: `Deleted profile “${target.name}”` },
    );
    if (done) refreshSnapshot();
  };

  return (
    <section aria-label="Launch profiles" className="space-y-2">
      <SectionHeader
        title="Launch profiles"
        count={profiles.length}
        actions={
          <Button
            type="button"
            variant="ghost"
            size="xs"
            disabled={full}
            title={full ? `At most ${MAX_PROFILES} profiles per project` : undefined}
            onClick={() => openEditor(null)}
          >
            <Plus aria-hidden />
            New profile
          </Button>
        }
      />
      {profiles.length === 0 ? (
        <p className="rounded-lg border px-3 py-2 text-muted-foreground">
          A profile starts several commands together, such as a web server and its API. Create one to get a
          Start menu with your profiles.
        </p>
      ) : (
        <ul className="divide-y rounded-lg border">
          {profiles.map((profile) => (
            <li key={profile.id} className="flex items-start gap-2 px-3 py-2">
              <div className="min-w-0 flex-1 space-y-0.5">
                <p className="font-medium">{profile.name}</p>
                <p className="flex flex-wrap gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
                  {profile.commands.map((command, index) => (
                    <Mono key={`${command.label}-${index}`}>{command.label}</Mono>
                  ))}
                </p>
              </div>
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label={`Edit ${profile.name}`}
                title="Edit"
                className="text-muted-foreground"
                onClick={() => openEditor(profile)}
              >
                <Pencil aria-hidden />
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label={`Delete ${profile.name}`}
                title="Delete"
                className="text-muted-foreground"
                onClick={() => {
                  setDeleting(profile);
                  setDeleteOpen(true);
                }}
              >
                <Trash2 aria-hidden />
              </Button>
            </li>
          ))}
        </ul>
      )}

      <LaunchProfileDialog
        project={project}
        profile={editing}
        open={editorOpen}
        onOpenChange={setEditorOpen}
        onSaved={refreshSnapshot}
      />
      <ConfirmDialog
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        destructive
        title={deleting ? `Delete profile “${deleting.name}”?` : "Delete profile?"}
        description="The profile is removed from LocalDeck. Running processes and your project files are not touched."
        confirmLabel="Delete"
        onConfirm={confirmDelete}
      />
    </section>
  );
}
