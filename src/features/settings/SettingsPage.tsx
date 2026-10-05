import { Loader2, Monitor, Moon, Sun, Trash2, type LucideIcon } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/shared";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { runAction } from "@/lib/actions";
import { formatAccelerator } from "@/lib/shortcut";
import { usePlatform } from "@/hooks/use-platform";
import { describeError, toErrorPayload } from "@/lib/errors";
import { forgetStoppedProjects, listEditors, updateSettings } from "@/lib/tauri";
import { selectProjects, selectSettings, useRuntimeStore } from "@/stores/runtime-store";
import type { EditorInfo, ErrorPayload, Settings, Theme } from "@/types";
import { BackupSection } from "./BackupSection";
import { IgnoredProjectsSection } from "./IgnoredProjectsSection";
import { SettingRow, SettingsSection } from "./settings-layout";
import { ShortcutField } from "./ShortcutField";
import { SystemAccessSection } from "./SystemAccessSection";
import { TerminalRow } from "./TerminalRow";

const THEMES: ReadonlyArray<{ value: Theme; label: string; icon: LucideIcon }> = [
  { value: "system", label: "System", icon: Monitor },
  { value: "light", label: "Light", icon: Sun },
  { value: "dark", label: "Dark", icon: Moon },
];

const isTheme = (value: string): value is Theme => THEMES.some((theme) => theme.value === value);

const RETENTION_DAYS = [7, 30, 90, 365] as const;
const FOREVER = "forever";
const AUTO_EDITOR = "auto";

const retentionLabel = (days: number): string => (days === 365 ? "1 year" : `${days} days`);

type EditorsState =
  | { status: "loading" }
  | { status: "ready"; editors: readonly EditorInfo[] }
  | { status: "error"; message: string };

function useEditors(): EditorsState {
  const [state, setState] = useState<EditorsState>({ status: "loading" });
  useEffect(() => {
    let cancelled = false;
    listEditors().then(
      (editors) => {
        if (!cancelled) setState({ status: "ready", editors });
      },
      (error: unknown) => {
        if (!cancelled) setState({ status: "error", message: describeError(error).message });
      },
    );
    return () => {
      cancelled = true;
    };
  }, []);
  return state;
}

function EditorRow({
  settings,
  saving,
  onChange,
}: {
  settings: Settings;
  saving: boolean;
  onChange: (editorId: string | null) => void;
}) {
  const state = useEditors();
  const editors = state.status === "ready" ? state.editors : [];
  const current = settings.preferredEditor;
  const noneInstalled = state.status === "ready" && !editors.some((editor) => editor.available);
  // The saved choice must stay selectable even if the catalogue hasn't loaded or dropped it.
  const currentKnown = current === null || editors.some((editor) => editor.id === current);

  let note: string;
  if (state.status === "error") note = `Couldn't list editors: ${state.message}`;
  else if (noneInstalled) {
    note = "No supported editor was found on this machine. “Open in editor” will report an error until one is installed.";
  } else {
    note = "Used by “Open in editor”. Auto-detect picks the first installed editor. A project can override this.";
  }

  return (
    <SettingRow
      label="Preferred editor"
      htmlFor="setting-editor"
      description={note}
      control={
        <Select
          value={current ?? AUTO_EDITOR}
          disabled={saving}
          onValueChange={(value) => onChange(value === AUTO_EDITOR ? null : value)}
        >
          <SelectTrigger id="setting-editor" className="w-44">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={AUTO_EDITOR}>Auto-detect</SelectItem>
            {!currentKnown && current !== null && <SelectItem value={current}>{current}</SelectItem>}
            {editors.map((editor) => (
              <SelectItem
                key={editor.id}
                value={editor.id}
                disabled={!editor.available && editor.id !== current}
              >
                {editor.name}
                {!editor.available && <span className="text-muted-foreground">(not installed)</span>}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      }
    />
  );
}

/** Every control saves immediately through `update_settings`; the backend's reply is the truth. */
export function SettingsPage() {
  const settings = useRuntimeStore(selectSettings);
  const setSettings = useRuntimeStore((state) => state.setSettings);
  const stoppedCount = useRuntimeStore(
    (state) =>
      selectProjects(state).filter(
        (project) => project.runtimeStatus === "stopped" && !project.ignored && !project.favorite,
      ).length,
  );
  const platform = usePlatform();
  const [saving, setSaving] = useState(false);
  const [confirmForget, setConfirmForget] = useState(false);
  const [shortcutError, setShortcutError] = useState<string | null>(null);

  /**
   * Full-object replace. The controls are driven by the store, so on failure they simply
   * stay on the last value the backend accepted (and the toast says why).
   */
  const save = useCallback(
    async (patch: Partial<Settings>, label: string, success: string): Promise<ErrorPayload | null> => {
      const current = useRuntimeStore.getState().settings;
      if (!current) return null;
      setSaving(true);
      let failure: ErrorPayload | null = null;
      const saved = await runAction(
        label,
        async () => {
          try {
            return await updateSettings({ ...current, ...patch });
          } catch (error) {
            failure = toErrorPayload(error);
            throw error;
          }
        },
        { success },
      );
      setSaving(false);
      if (saved) setSettings(saved);
      return failure;
    },
    [setSettings],
  );

  const saveShortcut = async (accelerator: string | null) => {
    setShortcutError(null);
    const failure = await save(
      { globalShortcut: accelerator },
      "Change global shortcut",
      accelerator === null ? "Global shortcut cleared" : `Global shortcut: ${formatAccelerator(accelerator, platform)}`,
    );
    // Rejected shortcuts (already taken, not registrable) are explained next to the control.
    if (failure?.code === "INVALID_INPUT") setShortcutError(failure.message);
  };

  if (!settings) {
    return (
      <div role="status" aria-label="Loading settings" className="space-y-2">
        <Skeleton className="h-5 w-32" />
        <Skeleton className="h-24 w-full" />
      </div>
    );
  }

  const retentionValue = settings.forgetAfterDays === null ? FOREVER : String(settings.forgetAfterDays);
  const customRetention =
    settings.forgetAfterDays !== null &&
    !(RETENTION_DAYS as readonly number[]).includes(settings.forgetAfterDays);

  const forgetStopped = async () => {
    const removed = await runAction("Forget stopped projects", forgetStoppedProjects, {
      success: false,
    });
    if (removed === undefined) return;
    toast.success(
      removed === 0
        ? "No stopped projects to forget"
        : `Forgot ${removed} stopped project${removed === 1 ? "" : "s"}`,
      { duration: 2500 },
    );
  };

  return (
    <div className="flex max-w-2xl flex-col gap-5">
      <header className="flex items-center justify-between gap-3">
        <h1 className="text-base font-semibold">Settings</h1>
        {saving && (
          <span role="status" className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
            <Loader2 aria-hidden className="size-3.5 animate-spin" /> Saving…
          </span>
        )}
      </header>

      <SettingsSection title="Appearance">
        <SettingRow
          label="Theme"
          description="System follows your OS appearance."
          control={
            <ToggleGroup
              type="single"
              variant="outline"
              size="sm"
              spacing={0}
              aria-label="Theme"
              value={settings.theme}
              disabled={saving}
              onValueChange={(value) => {
                if (isTheme(value) && value !== settings.theme) {
                  void save(
                    { theme: value },
                    "Change theme",
                    `Theme: ${THEMES.find((theme) => theme.value === value)?.label}`,
                  );
                }
              }}
            >
              {THEMES.map(({ value, label, icon: Icon }) => (
                <ToggleGroupItem key={value} value={value} className="px-2.5 text-xs">
                  <Icon aria-hidden /> {label}
                </ToggleGroupItem>
              ))}
            </ToggleGroup>
          }
        />
      </SettingsSection>

      <SettingsSection title="Startup">
        <SettingRow
          label="Launch at login"
          htmlFor="setting-launch-at-login"
          description="Start LocalDeck in the background when you sign in."
          control={
            <Switch
              id="setting-launch-at-login"
              checked={settings.launchAtLogin}
              disabled={saving}
              onCheckedChange={(checked) =>
                void save(
                  { launchAtLogin: checked },
                  "Change launch at login",
                  checked ? "LocalDeck will launch at login" : "LocalDeck won't launch at login",
                )
              }
            />
          }
        />
        <SettingRow
          label="Global shortcut"
          htmlFor="setting-global-shortcut"
          description={
            shortcutError ? (
              <span id="setting-global-shortcut-error" role="alert" className="text-destructive">
                {shortcutError}
              </span>
            ) : (
              "Brings LocalDeck to the front and opens the command palette from anywhere. Needs at least one modifier key; Esc cancels recording."
            )
          }
          control={
            <ShortcutField
              id="setting-global-shortcut"
              value={settings.globalShortcut}
              platform={platform}
              disabled={saving}
              invalid={shortcutError !== null}
              describedBy={shortcutError ? "setting-global-shortcut-error" : undefined}
              onCapture={(accelerator) => void saveShortcut(accelerator)}
              onClear={() => void saveShortcut(null)}
            />
          }
        />
      </SettingsSection>

      <SettingsSection title="Notifications">
        <SettingRow
          label="Desktop notifications"
          htmlFor="setting-notifications"
          description="Tell me when a managed command crashes, a port conflict appears, a component disappears or auto-restart gives up."
          control={
            <Switch
              id="setting-notifications"
              checked={settings.notificationsEnabled}
              disabled={saving}
              onCheckedChange={(checked) =>
                void save(
                  { notificationsEnabled: checked },
                  "Change notifications",
                  checked ? "Notifications on" : "Notifications off",
                )
              }
            />
          }
        />
      </SettingsSection>

      <SettingsSection title="Editor and terminal">
        <EditorRow
          settings={settings}
          saving={saving}
          onChange={(editorId) =>
            void save(
              { preferredEditor: editorId },
              "Change preferred editor",
              editorId === null ? "Editor: auto-detect" : "Preferred editor saved",
            )
          }
        />
        <TerminalRow
          settings={settings}
          saving={saving}
          onChange={(terminalId) =>
            void save(
              { preferredTerminal: terminalId },
              "Change preferred terminal",
              terminalId === null ? "Terminal: system default" : "Preferred terminal saved",
            )
          }
        />
      </SettingsSection>

      <SettingsSection title="Remembered projects">
        <SettingRow
          label="Forget stopped projects after"
          htmlFor="setting-retention"
          description="Stopped projects not seen for this long are removed automatically. Ignored and pinned projects are always kept."
          control={
            <Select
              value={retentionValue}
              disabled={saving}
              onValueChange={(value) =>
                void save(
                  { forgetAfterDays: value === FOREVER ? null : Number(value) },
                  "Change retention",
                  value === FOREVER ? "Keeping projects forever" : `Forgetting after ${retentionLabel(Number(value))}`,
                )
              }
            >
              <SelectTrigger id="setting-retention" className="w-36">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={FOREVER}>Keep forever</SelectItem>
                {customRetention && settings.forgetAfterDays !== null && (
                  <SelectItem value={String(settings.forgetAfterDays)}>
                    {retentionLabel(settings.forgetAfterDays)}
                  </SelectItem>
                )}
                {RETENTION_DAYS.map((days) => (
                  <SelectItem key={days} value={String(days)}>
                    {retentionLabel(days)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          }
        />
        <SettingRow
          label="Forget stopped projects now"
          description={
            stoppedCount === 0
              ? "No stopped projects to forget."
              : `${stoppedCount} stopped ${stoppedCount === 1 ? "project" : "projects"} (not ignored or pinned) will be removed.`
          }
          control={
            <Button
              variant="outline"
              size="sm"
              disabled={stoppedCount === 0}
              onClick={() => setConfirmForget(true)}
            >
              <Trash2 /> Forget stopped projects
            </Button>
          }
        />
      </SettingsSection>

      <IgnoredProjectsSection />
      <BackupSection />
      <SystemAccessSection />

      <SettingsSection title="About">
        <div className="space-y-1 py-2.5">
          <p className="font-medium">LocalDeck v0.1</p>
          <p className="text-xs leading-relaxed text-muted-foreground">
            A project-centric control center for local development. Everything stays on this
            machine: no account, no telemetry, nothing is sent anywhere. The only network traffic
            is probing localhost ports to tell which ones speak HTTP.
          </p>
        </div>
      </SettingsSection>

      <ConfirmDialog
        open={confirmForget}
        onOpenChange={setConfirmForget}
        title={`Forget ${stoppedCount} stopped project${stoppedCount === 1 ? "" : "s"}?`}
        description="LocalDeck removes these projects and their saved overrides. A project can reappear if LocalDeck detects it running again. Ignored and pinned projects are not affected."
        confirmLabel="Forget projects"
        destructive
        onConfirm={forgetStopped}
      />
    </div>
  );
}
