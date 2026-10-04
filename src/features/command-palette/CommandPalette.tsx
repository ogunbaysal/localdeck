import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { ConfirmDialog, StatusDot } from "@/components/shared";
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { usePlatform } from "@/hooks/use-platform";
import { paletteShortcutLabel } from "@/lib/platform";
import { listEditors } from "@/lib/tauri";
import {
  selectPorts,
  selectSettings,
  selectVisibleProjects,
  useRuntimeStore,
} from "@/stores/runtime-store";
import { useUiStore } from "@/stores/ui-store";
import type { EditorInfo, PortInfo } from "@/types";
import {
  buildPaletteItems,
  filterPaletteItems,
  PALETTE_GROUP_HEADINGS,
  PALETTE_GROUP_ORDER,
  type PaletteContext,
  type PaletteItem,
} from "./palette-items";

function PaletteBody({
  editors,
  onChoose,
}: {
  editors: readonly EditorInfo[];
  onChoose: (item: PaletteItem) => void;
}) {
  const projects = useRuntimeStore(selectVisibleProjects);
  const ports = useRuntimeStore(selectPorts);
  const defaultEditorId = useRuntimeStore((state) => selectSettings(state)?.preferredEditor ?? null);
  const platform = usePlatform();
  const navigate = useNavigate();
  const [query, setQuery] = useState("");

  const context = useMemo<PaletteContext>(
    () => ({
      navigate: (to) => void navigate(to),
      revealPort: (port: PortInfo) => {
        const ui = useUiStore.getState();
        ui.setPortsFilter("all");
        ui.setPortsSearch(`:${port.port}`);
        ui.selectPort(port.id);
        void navigate("/ports");
      },
    }),
    [navigate],
  );
  const items = useMemo(
    () => buildPaletteItems({ projects, ports, editors, defaultEditorId }, context),
    [projects, ports, editors, defaultEditorId, context],
  );
  const matches = useMemo(() => filterPaletteItems(items, ports, query), [items, ports, query]);

  return (
    // Filtering is ours (project-aware, port-aware); cmdk only handles keyboard + selection.
    <Command shouldFilter={false} loop label="Search projects, ports and actions">
      <CommandInput
        value={query}
        onValueChange={setQuery}
        placeholder="Search projects, ports, actions…"
      />
      <CommandList className="max-h-[min(18rem,calc(100vh-11rem))]">
        <CommandEmpty>No matching projects, ports or actions.</CommandEmpty>
        {PALETTE_GROUP_ORDER.map((group) => {
          const groupItems = matches.filter((item) => item.group === group);
          if (groupItems.length === 0) return null;
          return (
            <CommandGroup key={group} heading={PALETTE_GROUP_HEADINGS[group]}>
              {groupItems.map((item) => (
                <CommandItem key={item.id} value={item.id} onSelect={() => onChoose(item)}>
                  {item.status ? (
                    <StatusDot status={item.status} />
                  ) : (
                    item.icon && <item.icon className={item.destructive ? "text-destructive" : undefined} />
                  )}
                  <span className="min-w-0 flex-1 truncate">{item.label}</span>
                  {item.meta && " "}
                  {item.meta && (
                    <span className="max-w-[40%] shrink-0 truncate font-mono text-xs text-muted-foreground">
                      {item.meta}
                    </span>
                  )}
                </CommandItem>
              ))}
            </CommandGroup>
          );
        })}
      </CommandList>
      <div className="flex items-center justify-between gap-3 border-t px-2 py-1.5 text-xs text-muted-foreground">
        <span>↑↓ navigate · ↵ run · esc close</span>
        <span>
          <kbd className="rounded border bg-muted px-1 font-mono text-[11px]">
            {paletteShortcutLabel(platform)}
          </kbd>{" "}
          toggles
        </span>
      </div>
    </Command>
  );
}

/** ⌘K / Ctrl+K palette. Mounted once by the shell; open state lives in the UI store. */
export function CommandPalette() {
  const open = useUiStore((state) => state.paletteOpen);
  const setOpen = useUiStore((state) => state.setPaletteOpen);
  // Editors only label "Open in …"; load them when the palette first opens and keep them.
  const [editors, setEditors] = useState<readonly EditorInfo[] | null>(null);
  const [pending, setPending] = useState<PaletteItem | null>(null);

  useEffect(() => {
    if (!open || editors !== null) return;
    let cancelled = false;
    listEditors().then(
      (list) => {
        if (!cancelled) setEditors(list);
      },
      // Not worth interrupting: actions fall back to "Open in Editor" and we retry next open.
      () => undefined,
    );
    return () => {
      cancelled = true;
    };
  }, [open, editors]);

  const choose = (item: PaletteItem) => {
    setOpen(false);
    if (item.confirm) setPending(item);
    else void item.run();
  };

  return (
    <>
      <CommandDialog
        open={open}
        onOpenChange={setOpen}
        title="Command palette"
        description="Search projects, ports and actions"
        className="top-[12%]"
      >
        <PaletteBody editors={editors ?? []} onChoose={choose} />
      </CommandDialog>
      <ConfirmDialog
        open={pending !== null}
        onOpenChange={(next) => {
          if (!next) setPending(null);
        }}
        title={pending?.confirm?.title ?? ""}
        description={pending?.confirm?.description ?? ""}
        confirmLabel={pending?.confirm?.confirmLabel}
        destructive={pending?.destructive}
        onConfirm={() => void pending?.run()}
      />
    </>
  );
}
