import { RefreshCw } from "lucide-react";
import { useCallback, useEffect, useId, useState, type FormEvent } from "react";
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
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { describeError, toErrorPayload } from "@/lib/errors";
import { listProjectScripts, refreshProject, setPrimaryCommand, setProjectOverride } from "@/lib/tauri";
import type { ErrorPayload, Project, ProjectScripts } from "@/types";
import { useEditors } from "./use-editors";

const DEFAULT_EDITOR = "default";
const HOST_PORT = /^[\w.-]+(?::\d{1,5})?(?:[/?#]\S*)?$|^\[[0-9a-fA-F:.]+\](?::\d{1,5})?(?:[/?#]\S*)?$/;

/**
 * Loose client-side check of the URL override; the backend is authoritative.
 * Returns a message, or `null` when the value is empty or plausible.
 */
export function looseUrlProblem(value: string): string | null {
  const url = value.trim();
  if (url === "") return null;
  if (/\s/.test(url)) return "A URL can't contain spaces.";
  const separator = url.indexOf("://");
  if (separator >= 0) {
    const scheme = url.slice(0, separator).toLowerCase();
    if (scheme !== "http" && scheme !== "https") return "Use an http:// or https:// URL.";
    return url.length > separator + 3 ? null : "Add a host after the scheme.";
  }
  return HOST_PORT.test(url) ? null : "Enter a URL like http://localhost:3000.";
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

type ScriptsState =
  | { status: "loading"; data: null }
  | { status: "error"; data: null; error: ErrorPayload }
  | { status: "ready"; data: ProjectScripts };

/** Primary start command: pick a package.json script. Each change applies immediately. */
function PrimaryCommandSection({
  project,
  onError,
}: {
  project: Project;
  onError: (error: ErrorPayload | null) => void;
}) {
  const groupId = useId();
  const [state, setState] = useState<ScriptsState>({ status: "loading", data: null });
  const [pending, setPending] = useState(false);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let cancelled = false;
    listProjectScripts(project.id).then(
      (data) => {
        if (!cancelled) setState({ status: "ready", data });
      },
      (failure: unknown) => {
        if (!cancelled) setState({ status: "error", data: null, error: toErrorPayload(failure) });
      },
    );
    return () => {
      cancelled = true;
    };
  }, [project.id, attempt]);

  const choose = useCallback(
    async (script: string | null) => {
      setPending(true);
      onError(null);
      try {
        await setPrimaryCommand(project.id, script);
        toast.success(script === null ? "Start command cleared" : `Start command set to ${script}`);
        setAttempt((n) => n + 1);
      } catch (failure) {
        onError(toErrorPayload(failure));
      } finally {
        setPending(false);
      }
    },
    [project.id, onError],
  );

  const data = state.data;
  return (
    <section className="space-y-2 rounded-lg border p-3" aria-labelledby={groupId}>
      <div className="flex items-center justify-between gap-2">
        <h3 id={groupId} className="font-medium">
          Primary start command
        </h3>
        <Button
          type="button"
          variant="ghost"
          size="xs"
          disabled={pending || data === null || data.primaryScript === null}
          onClick={() => void choose(null)}
        >
          Clear
        </Button>
      </div>
      <p className="text-muted-foreground">
        Currently: {project.primaryCommand ? <Mono>{project.primaryCommand.label}</Mono> : "Not set"}
      </p>

      {state.status === "loading" && (
        <div role="status" aria-label="Loading scripts" className="space-y-1.5">
          <Skeleton className="h-6 w-full" />
          <Skeleton className="h-6 w-full" />
        </div>
      )}
      {state.status === "error" && (
        <ErrorState error={state.error} className="py-3" onRetry={() => setAttempt((n) => n + 1)} />
      )}
      {data !== null && (
        <>
          <p className="text-xs text-muted-foreground">
            Package manager: <Mono>{data.packageManager}</Mono>
            {!data.packageManagerDetected && " (assumed: no lockfile found)"}
          </p>
          {data.scripts.length === 0 ? (
            <p className="text-muted-foreground">
              No package.json scripts found. LocalDeck v0.1 reads start commands from package.json scripts.
            </p>
          ) : (
            <div role="radiogroup" aria-labelledby={groupId} className="max-h-44 space-y-0.5 overflow-y-auto">
              {data.scripts.map((script) => (
                <label
                  key={script.name}
                  className="flex cursor-pointer items-baseline gap-2 rounded-md px-1.5 py-1 hover:bg-muted/60"
                >
                  <input
                    type="radio"
                    name={`${groupId}-script`}
                    className="translate-y-0.5 accent-primary"
                    checked={data.primaryScript === script.name}
                    disabled={pending}
                    onChange={() => void choose(script.name)}
                  />
                  <span className="font-medium">{script.name}</span>
                  <Mono className="min-w-0 flex-1 truncate text-xs text-muted-foreground" title={script.command}>
                    {script.command}
                  </Mono>
                </label>
              ))}
            </div>
          )}
        </>
      )}
    </section>
  );
}

function SettingsForm({
  project,
  onOpenChange,
}: {
  project: Project;
  onOpenChange: (open: boolean) => void;
}) {
  const ids = { name: useId(), url: useId(), editor: useId() };
  const [displayName, setDisplayName] = useState(project.overrides.displayName ?? "");
  const [primaryUrl, setPrimaryUrl] = useState(project.overrides.primaryUrl ?? "");
  const [editor, setEditor] = useState(project.overrides.preferredEditor ?? DEFAULT_EDITOR);
  const [urlProblem, setUrlProblem] = useState<string | null>(null);
  const [error, setError] = useState<ErrorPayload | null>(null);
  const [busy, setBusy] = useState<"save" | "refresh" | null>(null);
  const editors = useEditors();

  const save = async (event: FormEvent) => {
    event.preventDefault();
    const problem = looseUrlProblem(primaryUrl);
    setUrlProblem(problem);
    if (problem) return;
    setBusy("save");
    setError(null);
    try {
      await setProjectOverride(project.id, {
        displayName: displayName.trim() || null,
        primaryUrl: primaryUrl.trim() || null,
        preferredEditor: editor === DEFAULT_EDITOR ? null : editor,
      });
      toast.success("Project settings saved");
      onOpenChange(false);
    } catch (failure) {
      setError(toErrorPayload(failure));
    } finally {
      setBusy(null);
    }
  };

  const refresh = async () => {
    setBusy("refresh");
    setError(null);
    try {
      await refreshProject(project.id);
      toast.success(`Refreshed ${project.name}`);
    } catch (failure) {
      setError(toErrorPayload(failure));
    } finally {
      setBusy(null);
    }
  };

  const known = editors.editors?.some((candidate) => candidate.id === editor) ?? false;

  return (
    <form onSubmit={(event) => void save(event)} className="grid gap-3">
      <div className="grid gap-1.5">
        <Label htmlFor={ids.name}>Display name</Label>
        <Input
          id={ids.name}
          value={displayName}
          placeholder={project.detectedName}
          onChange={(event) => setDisplayName(event.target.value)}
        />
        <p className="text-xs text-muted-foreground">Leave blank to use the detected name.</p>
      </div>

      <div className="grid gap-1.5">
        <Label htmlFor={ids.url}>Primary URL override</Label>
        <Input
          id={ids.url}
          value={primaryUrl}
          placeholder="http://localhost:3000"
          className="font-mono"
          aria-invalid={urlProblem !== null}
          aria-describedby={urlProblem ? `${ids.url}-problem` : undefined}
          onChange={(event) => {
            setPrimaryUrl(event.target.value);
            setUrlProblem(null);
          }}
        />
        {urlProblem ? (
          <p id={`${ids.url}-problem`} className="text-xs text-destructive">
            {urlProblem}
          </p>
        ) : (
          <p className="text-xs text-muted-foreground">Leave blank to let LocalDeck pick the URL.</p>
        )}
      </div>

      <div className="grid gap-1.5">
        <Label htmlFor={ids.editor}>Preferred editor</Label>
        <Select value={editor} onValueChange={setEditor} disabled={editors.loading && editors.editors === null}>
          <SelectTrigger id={ids.editor} className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent position="popper">
            <SelectItem value={DEFAULT_EDITOR}>Use global default</SelectItem>
            {editors.editors?.map((candidate) => (
              <SelectItem key={candidate.id} value={candidate.id}>
                {candidate.name}
                {!candidate.available && <span className="text-muted-foreground"> · not installed</span>}
              </SelectItem>
            ))}
            {editors.editors && editor !== DEFAULT_EDITOR && !known && (
              <SelectItem value={editor}>{editor} · unknown</SelectItem>
            )}
          </SelectContent>
        </Select>
        {editors.error && (
          <p className="text-xs text-destructive">
            Couldn't list editors: {editors.error.message}{" "}
            <button type="button" className="underline" onClick={editors.reload}>
              Retry
            </button>
          </p>
        )}
      </div>

      <PrimaryCommandSection project={project} onError={setError} />

      {error && <InlineError error={error} />}

      <DialogFooter className="sm:justify-between">
        <Button type="button" variant="ghost" disabled={busy !== null} onClick={() => void refresh()}>
          <RefreshCw aria-hidden className={busy === "refresh" ? "animate-spin" : undefined} />
          Refresh detection
        </Button>
        <div className="flex flex-col-reverse gap-2 sm:flex-row">
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button type="submit" disabled={busy !== null}>
            {busy === "save" ? "Saving…" : "Save"}
          </Button>
        </div>
      </DialogFooter>
    </form>
  );
}

/** Per-project overrides (name, URL, editor) and the primary start command. */
export function ProjectSettingsDialog({
  project,
  open,
  onOpenChange,
}: {
  project: Project;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[calc(100vh-2rem)] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Project settings</DialogTitle>
          <DialogDescription>
            Overrides for <span className="font-medium text-foreground">{project.name}</span>. Saved on this
            machine only.
          </DialogDescription>
        </DialogHeader>
        {open && <SettingsForm project={project} onOpenChange={onOpenChange} />}
      </DialogContent>
    </Dialog>
  );
}
