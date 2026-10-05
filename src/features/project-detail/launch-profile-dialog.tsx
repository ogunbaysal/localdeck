import { useEffect, useId, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { ErrorState } from "@/components/shared/error-state";
import { Mono } from "@/components/shared/mono";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { describeError, toErrorPayload } from "@/lib/errors";
import { listProjectScripts, setLaunchProfiles } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import type { ErrorPayload, LaunchProfile, LaunchProfileInput, Project, ProjectScripts, StartOption } from "@/types";
import { groupBySource, SOURCE_LABEL } from "./start-options";

export const MAX_PROFILES = 8;
export const MAX_PROFILE_OPTIONS = 8;
export const MAX_PROFILE_NAME = 60;

/** Ids of the start options a profile was derived from (`PrimaryCommand.script` holds the option id). */
export function profileOptionIds(profile: LaunchProfile): string[] {
  return profile.commands.flatMap((command) => (command.script === null ? [] : [command.script]));
}

/** The wire form of a stored profile: the id keeps it, the option ids re-derive its commands. */
export function toProfileInput(profile: LaunchProfile): LaunchProfileInput {
  return { id: profile.id, name: profile.name, options: profileOptionIds(profile) };
}

/** Validation message for a profile name, or `null` when it is usable. */
export function profileNameProblem(name: string, others: readonly LaunchProfile[]): string | null {
  const trimmed = name.trim();
  if (trimmed === "") return "Give the profile a name.";
  if (trimmed.length > MAX_PROFILE_NAME) return `Use at most ${MAX_PROFILE_NAME} characters.`;
  const lower = trimmed.toLowerCase();
  if (others.some((other) => other.name.trim().toLowerCase() === lower)) {
    return "Another profile already uses this name.";
  }
  return null;
}

type ScriptsState =
  | { status: "loading" }
  | { status: "error"; error: ErrorPayload }
  | { status: "ready"; options: readonly StartOption[] };

function ProfileForm({
  project,
  profile,
  onOpenChange,
  onSaved,
}: {
  project: Project;
  profile: LaunchProfile | null;
  onOpenChange: (open: boolean) => void;
  onSaved: () => void;
}) {
  const ids = { name: useId(), options: useId() };
  const [name, setName] = useState(profile?.name ?? "");
  // Selection order = start order.
  const [selected, setSelected] = useState<string[]>(profile ? profileOptionIds(profile) : []);
  const [scripts, setScripts] = useState<ScriptsState>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);
  const [nameProblem, setNameProblem] = useState<string | null>(null);
  const [optionsProblem, setOptionsProblem] = useState<string | null>(null);
  const [error, setError] = useState<ErrorPayload | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setScripts({ status: "loading" });
    listProjectScripts(project.id).then(
      (data: ProjectScripts) => {
        if (!cancelled) setScripts({ status: "ready", options: data.options });
      },
      (failure: unknown) => {
        if (!cancelled) setScripts({ status: "error", error: toErrorPayload(failure) });
      },
    );
    return () => {
      cancelled = true;
    };
  }, [project.id, attempt]);

  const toggle = (optionId: string) => {
    setOptionsProblem(null);
    setSelected((current) =>
      current.includes(optionId) ? current.filter((id) => id !== optionId) : [...current, optionId],
    );
  };

  const others = project.launchProfiles.filter((other) => other.id !== profile?.id);
  const available = scripts.status === "ready" ? scripts.options : [];
  // Selected ids that discovery no longer finds stay visible so they can be removed.
  const missing = scripts.status === "ready" ? selected.filter((id) => !available.some((o) => o.id === id)) : [];
  const atLimit = selected.length >= MAX_PROFILE_OPTIONS;

  const save = async (event: FormEvent) => {
    event.preventDefault();
    const problem = profileNameProblem(name, others);
    const noOptions = selected.length === 0 ? "Pick at least one command." : null;
    setNameProblem(problem);
    setOptionsProblem(noOptions);
    if (problem || noOptions) return;

    const edited: LaunchProfileInput = { name: name.trim(), options: selected };
    if (profile) edited.id = profile.id;
    const next = profile
      ? project.launchProfiles.map((existing) => (existing.id === profile.id ? edited : toProfileInput(existing)))
      : [...project.launchProfiles.map(toProfileInput), edited];

    setBusy(true);
    setError(null);
    try {
      await setLaunchProfiles(project.id, next);
      toast.success(profile ? `Saved profile “${edited.name}”` : `Created profile “${edited.name}”`);
      onSaved();
      onOpenChange(false);
    } catch (failure) {
      setError(toErrorPayload(failure));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form onSubmit={(event) => void save(event)} className="grid gap-3">
      <div className="grid gap-1.5">
        <Label htmlFor={ids.name}>Name</Label>
        <Input
          id={ids.name}
          value={name}
          placeholder="Web + API"
          maxLength={MAX_PROFILE_NAME + 20}
          aria-invalid={nameProblem !== null}
          aria-describedby={nameProblem ? `${ids.name}-problem` : undefined}
          onChange={(event) => {
            setName(event.target.value);
            setNameProblem(null);
          }}
        />
        {nameProblem && (
          <p id={`${ids.name}-problem`} className="text-xs text-destructive">
            {nameProblem}
          </p>
        )}
      </div>

      <fieldset className="grid gap-1.5" aria-describedby={`${ids.options}-help`}>
        <legend className="mb-1.5 text-sm leading-none font-medium">Commands</legend>
        <p id={`${ids.options}-help`} className="text-xs text-muted-foreground">
          Started together, in the order you select them (up to {MAX_PROFILE_OPTIONS}).
        </p>
        {scripts.status === "loading" && (
          <div role="status" aria-label="Loading start options" className="space-y-1.5">
            <Skeleton className="h-6 w-full" />
            <Skeleton className="h-6 w-full" />
          </div>
        )}
        {scripts.status === "error" && (
          <ErrorState error={scripts.error} className="py-3" onRetry={() => setAttempt((n) => n + 1)} />
        )}
        {scripts.status === "ready" && available.length === 0 && missing.length === 0 && (
          <p className="text-muted-foreground">
            No start options found in this project, so there is nothing to put in a profile.
          </p>
        )}
        {scripts.status === "ready" && (available.length > 0 || missing.length > 0) && (
          <div className="max-h-56 space-y-2 overflow-y-auto rounded-md border p-1">
            {groupBySource(available).map((group) => (
              <div key={group.source} role="group" aria-label={SOURCE_LABEL[group.source]} className="space-y-0.5">
                <p aria-hidden className="px-1.5 text-xs font-medium text-muted-foreground">
                  {SOURCE_LABEL[group.source]}
                </p>
                {group.options.map((option) => (
                  <OptionRow
                    key={option.id}
                    label={option.label}
                    detail={option.detail}
                    position={selected.indexOf(option.id) + 1}
                    disabled={busy || (atLimit && !selected.includes(option.id))}
                    onToggle={() => toggle(option.id)}
                  />
                ))}
              </div>
            ))}
            {missing.map((id) => (
              <OptionRow
                key={id}
                label={id}
                detail="no longer found in the project"
                position={selected.indexOf(id) + 1}
                disabled={busy}
                onToggle={() => toggle(id)}
              />
            ))}
          </div>
        )}
        {optionsProblem && <p className="text-xs text-destructive">{optionsProblem}</p>}
      </fieldset>

      {error && <InlineError error={error} />}

      <DialogFooter>
        <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
          Cancel
        </Button>
        <Button type="submit" disabled={busy}>
          {busy ? "Saving…" : "Save"}
        </Button>
      </DialogFooter>
    </form>
  );
}

function OptionRow({
  label,
  detail,
  position,
  disabled,
  onToggle,
}: {
  label: string;
  detail: string | null;
  /** 1-based start position, or 0 when not selected. */
  position: number;
  disabled: boolean;
  onToggle: () => void;
}) {
  return (
    <label
      className={cn(
        "flex cursor-pointer items-baseline gap-2 rounded-md px-1.5 py-1 hover:bg-muted/60",
        disabled && "cursor-not-allowed opacity-60",
      )}
    >
      <input
        type="checkbox"
        className="translate-y-0.5 accent-primary"
        checked={position > 0}
        disabled={disabled}
        onChange={onToggle}
      />
      <Mono className="font-medium">{label}</Mono>
      {detail && (
        <Mono className="min-w-0 flex-1 truncate text-xs text-muted-foreground" title={detail}>
          {detail}
        </Mono>
      )}
      {position > 0 && (
        <span
          aria-label={`Starts ${position}${position === 1 ? "st" : position === 2 ? "nd" : position === 3 ? "rd" : "th"}`}
          className="ml-auto inline-flex h-4 min-w-4 shrink-0 items-center justify-center rounded-full bg-primary px-1 text-[10px] font-medium text-primary-foreground tabular-nums"
        >
          {position}
        </span>
      )}
    </label>
  );
}

function InlineError({ error }: { error: ErrorPayload }) {
  const { title, message } = describeError(error);
  return (
    <Alert variant="destructive">
      <AlertTitle>{title}</AlertTitle>
      <AlertDescription>{message}</AlertDescription>
    </Alert>
  );
}

/** Create (`profile === null`) or edit one launch profile of a project. */
export function LaunchProfileDialog({
  project,
  profile,
  open,
  onOpenChange,
  onSaved,
}: {
  project: Project;
  profile: LaunchProfile | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Called after the backend accepted the new profile list. */
  onSaved: () => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[calc(100vh-2rem)] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{profile ? "Edit launch profile" : "New launch profile"}</DialogTitle>
          <DialogDescription>
            A launch profile starts several commands of <span className="font-medium text-foreground">{project.name}</span>{" "}
            together.
          </DialogDescription>
        </DialogHeader>
        {open && (
          <ProfileForm project={project} profile={profile} onOpenChange={onOpenChange} onSaved={onSaved} />
        )}
      </DialogContent>
    </Dialog>
  );
}
