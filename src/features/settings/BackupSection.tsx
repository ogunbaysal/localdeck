import { Download, Upload } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Mono } from "@/components/shared";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { runAction } from "@/lib/actions";
import { exportOverrides, getSettings, importOverrides } from "@/lib/tauri";
import { useRuntimeStore } from "@/stores/runtime-store";
import type { ImportReport } from "@/types";
import { SettingRow, SettingsSection } from "./settings-layout";

const plural = (count: number, word: string) => `${count} ${word}${count === 1 ? "" : "s"}`;

function ImportReportDialog({ report, onClose }: { report: ImportReport | null; onClose: () => void }) {
  return (
    <Dialog open={report !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Import finished</DialogTitle>
          <DialogDescription>
            {report && (
              <>
                {plural(report.applied, "project")} updated
                {report.skipped.length > 0 && <>, {report.skipped.length} skipped</>}.{" "}
                {report.settingsApplied ? "Settings were applied." : "Settings were not changed."}
              </>
            )}
          </DialogDescription>
        </DialogHeader>
        {report && report.skipped.length > 0 && (
          <div>
            <h3 className="mb-1 text-xs font-medium tracking-wide text-muted-foreground uppercase">Skipped</h3>
            <ul className="max-h-60 divide-y overflow-y-auto rounded-md border">
              {report.skipped.map((skip, index) => (
                <li key={`${skip.subject}:${index}`} className="px-2.5 py-1.5">
                  <Mono className="block break-all font-medium">{skip.subject}</Mono>
                  <p className="text-xs text-muted-foreground">{skip.reason}</p>
                </li>
              ))}
            </ul>
          </div>
        )}
        <DialogFooter>
          <Button onClick={onClose}>Done</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** Export/import of settings and per-project overrides as a JSON file chosen in a native dialog. */
export function BackupSection() {
  const [busy, setBusy] = useState<"export" | "import" | null>(null);
  const [report, setReport] = useState<ImportReport | null>(null);

  const doExport = async () => {
    setBusy("export");
    const path = await runAction("Export overrides", exportOverrides, { success: false });
    setBusy(null);
    // `null` = the user cancelled the save dialog: nothing to report.
    if (path) toast.success("Overrides exported", { description: path, duration: 6000 });
  };

  const doImport = async () => {
    setBusy("import");
    const imported = await runAction("Import overrides", importOverrides, { success: false });
    setBusy(null);
    if (!imported) return;
    setReport(imported);
    // Imported settings change the store the Settings controls read from.
    if (imported.settingsApplied) {
      const settings = await runAction("Reload settings", getSettings, { success: false });
      if (settings) useRuntimeStore.getState().setSettings(settings);
    }
  };

  return (
    <SettingsSection title="Backup">
      <SettingRow
        label="Export"
        description={
          <>
            Saves your settings and per-project choices (names, URLs, editor, terminal, pins, launch
            profiles) to a JSON file. Start commands are exported as script references such as{" "}
            <Mono>pkg:dev</Mono>, never as raw commands.
          </>
        }
        control={
          <Button variant="outline" size="sm" disabled={busy !== null} onClick={() => void doExport()}>
            <Download /> Export overrides…
          </Button>
        }
      />
      <SettingRow
        label="Import"
        description="Applies a file exported by LocalDeck. Commands are rebuilt from their script references on this machine; projects that aren't on disk are skipped and listed afterwards."
        control={
          <Button variant="outline" size="sm" disabled={busy !== null} onClick={() => void doImport()}>
            <Upload /> Import overrides…
          </Button>
        }
      />
      <ImportReportDialog report={report} onClose={() => setReport(null)} />
    </SettingsSection>
  );
}
