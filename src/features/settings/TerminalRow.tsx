import { useTerminals } from "@/features/project-detail/use-terminals";
import { describeError } from "@/lib/errors";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import type { Settings } from "@/types";
import { SettingRow } from "./settings-layout";

const SYSTEM_DEFAULT = "default";

/** Terminal used by “Open terminal”; `null` = the platform default. Projects can override it. */
export function TerminalRow({
  settings,
  saving,
  onChange,
}: {
  settings: Settings;
  saving: boolean;
  onChange: (terminalId: string | null) => void;
}) {
  const { terminals, error, loading } = useTerminals();
  const list = terminals ?? [];
  const current = settings.preferredTerminal;
  // The saved choice must stay selectable even if the catalogue hasn't loaded or dropped it.
  const currentKnown = current === null || list.some((terminal) => terminal.id === current);

  let note: string;
  if (error) note = `Couldn't list terminals: ${describeError(error).message}`;
  else if (terminals && !terminals.some((terminal) => terminal.available)) {
    note = "No supported terminal was found on this machine. “Open terminal” will report an error until one is installed.";
  } else {
    note = "Used by “Open terminal”. System default uses your platform's standard terminal. A project can override this.";
  }

  return (
    <SettingRow
      label="Preferred terminal"
      htmlFor="setting-terminal"
      description={note}
      control={
        <Select
          value={current ?? SYSTEM_DEFAULT}
          disabled={saving || (loading && terminals === null)}
          onValueChange={(value) => onChange(value === SYSTEM_DEFAULT ? null : value)}
        >
          <SelectTrigger id="setting-terminal" className="w-44">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={SYSTEM_DEFAULT}>System default</SelectItem>
            {!currentKnown && current !== null && <SelectItem value={current}>{current}</SelectItem>}
            {list.map((terminal) => (
              <SelectItem
                key={terminal.id}
                value={terminal.id}
                disabled={!terminal.available && terminal.id !== current}
              >
                {terminal.name}
                {!terminal.available && <span className="text-muted-foreground">(not installed)</span>}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      }
    />
  );
}
