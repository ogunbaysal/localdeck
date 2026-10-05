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
import { Switch } from "@/components/ui/switch";
import { describeError, toErrorPayload } from "@/lib/errors";
import { listProjectScripts, refreshProject, setPrimaryCommand, setProjectOverride } from "@/lib/tauri";
import type { ErrorPayload, Project, ProjectScripts, StartOption } from "@/types";
import { groupBySource, SOURCE_LABEL } from "./start-options";
import { useEditors } from "./use-editors";
import { useTerminals } from "./use-terminals";

const DEFAULT_EDITOR = "default";
const DEFAULT_TERMINAL = "default";

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

/** Start command: pick one discovered start option, grouped by where it was found. */
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
    async (option: StartOption | null) => {
      setPending(true);
      onError(null);
      try {
        await setPrimaryCommand(project.id, option?.id ?? null);
        toast.success(option === null ? "Start command cleared" : `Start command set to ${option.label}`);
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
  const groups = data === null ? [] : groupBySource(data.options);
  const usesPackageManager = data?.options.some((option) => option.source === "packageJson") ?? false;
  return (
    <section className="space-y-2 rounded-lg border p-3" aria-labelledby={groupId}>
      <div className="flex items-center justify-between gap-2">
        <h3 id={groupId} className="font-medium">
          Start command
        </h3>
        <Button
          type="button"
          variant="ghost"
          size="xs"
          disabled={pending || data === null || data.primaryOption === null}
          onClick={() => void choose(null)}
        >
          Clear
        </Button>
      </div>
      <p className="text-muted-foreground">
        Currently: {project.primaryCommand ? <Mono>{project.primaryCommand.label}</Mono> : "Not set"}
      </p>

      {state.status === "loading" && (
        <div role="status" aria-label="Loading start options" className="space-y-1.5">
          <Skeleton className="h-6 w-full" />
          <Skeleton className="h-6 w-full" />
        </div>
      )}
      {state.status === "error" && (
        <ErrorState error={state.error} className="py-3" onRetry={() => setAttempt((n) => n + 1)} />
      )}
      {data !== null && (
        <>
          {usesPackageManager && (
            <p className="text-xs text-muted-foreground">
              Package manager: <Mono>{data.packageManager}</Mono>
              {!data.packageManagerDetected && " (assumed: no lockfile found)"}
            </p>
          )}
          {data.options.length === 0 ? (
            <p className="text-muted-foreground">
              No start options found. LocalDeck reads package.json scripts, Makefile targets, justfile recipes,
              composer.json scripts, Cargo.toml (crates with a binary), docker-compose files and Python project
              scripts (pyproject.toml) in the project root.
            </p>
          ) : (
            <div className="max-h-52 space-y-2 overflow-y-auto">
              {groups.map((group) => {
                const headingId = `${groupId}-${group.source}`;
                return (
                  <div key={group.source} role="radiogroup" aria-labelledby={headingId} className="space-y-0.5">
                    <p id={headingId} className="px-1.5 text-xs font-medium text-muted-foreground">
                      {SOURCE_LABEL[group.source]}
                    </p>
                    {group.options.map((option) => (
                      <label
                        key={option.id}
                        className="flex cursor-pointer items-baseline gap-2 rounded-md px-1.5 py-1 hover:bg-muted/60"
                      >
                        <input
                          type="radio"
                          name={`${groupId}-option`}
                          className="translate-y-0.5 accent-primary"
                          checked={data.primaryOption === option.id}
                          disabled={pending}
                          onChange={() => void choose(option)}
                        />
                        <Mono className="font-medium">{option.label}</Mono>
                        {option.detail && (
                          <Mono
                            className="min-w-0 flex-1 truncate text-xs text-muted-foreground"
                            title={option.detail}
                          >
                            {option.detail}
                          </Mono>
                        )}
                      </label>
                    ))}
                  </div>
                );
              })}
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
  const ids = { name: useId(), url: useId(), editor: useId(), terminal: useId(), restart: useId() };
  const [displayName, setDisplayName] = useState(project.overrides.displayName ?? "");
  const [primaryUrl, setPrimaryUrl] = useState(project.overrides.primaryUrl ?? "");
  const [editor, setEditor] = useState(project.overrides.preferredEditor ?? DEFAULT_EDITOR);
  const [terminal, setTerminal] = useState(project.overrides.preferredTerminal ?? DEFAULT_TERMINAL);
  const [autoRestart, setAutoRestart] = useState(project.overrides.autoRestart);
  const [urlProblem, setUrlProblem] = useState<string | null>(null);
  const [error, setError] = useState<ErrorPayload | null>(null);
  const [busy, setBusy] = useState<"save" | "refresh" | null>(null);
  const editors = useEditors();
  const terminals = useTerminals();

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
        preferredTerminal: terminal === DEFAULT_TERMINAL ? null : terminal,
        autoRestart,
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
  const terminalKnown = terminals.terminals?.some((candidate) => candidate.id === terminal) ?? false;

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

      <div className="grid gap-1.5">
        <Label htmlFor={ids.terminal}>Terminal</Label>
        <Select
          value={terminal}
          onValueChange={setTerminal}
          disabled={terminals.loading && terminals.terminals === null}
        >
          <SelectTrigger id={ids.terminal} className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent position="popper">
            <SelectItem value={DEFAULT_TERMINAL}>Use global default</SelectItem>
            {terminals.terminals?.map((candidate) => (
              <SelectItem key={candidate.id} value={candidate.id}>
                {candidate.name}
                {!candidate.available && <span className="text-muted-foreground"> · not installed</span>}
              </SelectItem>
            ))}
            {terminals.terminals && terminal !== DEFAULT_TERMINAL && !terminalKnown && (
              <SelectItem value={terminal}>{terminal} · unknown</SelectItem>
            )}
          </SelectContent>
        </Select>
        {terminals.error && (
          <p className="text-xs text-destructive">
            Couldn't list terminals: {terminals.error.message}{" "}
            <button type="button" className="underline" onClick={terminals.reload}>
              Retry
            </button>
          </p>
        )}
      </div>

      <div className="flex items-start justify-between gap-3">
        <div className="grid gap-0.5">
          <Label htmlFor={ids.restart}>Auto-restart</Label>
          <p id={`${ids.restart}-help`} className="text-xs text-muted-foreground">
            Restarts commands started from LocalDeck if they crash: up to 5 attempts with increasing delay.
          </p>
        </div>
        <Switch
          id={ids.restart}
          checked={autoRestart}
          onCheckedChange={setAutoRestart}
          aria-describedby={`${ids.restart}-help`}
        />
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

/** Per-project overrides (name, URL, editor, terminal, auto-restart) and the start command. */
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
