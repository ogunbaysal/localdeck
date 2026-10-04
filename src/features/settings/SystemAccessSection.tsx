import { Check, ShieldAlert } from "lucide-react";
import type { ReactNode } from "react";
import { Mono } from "@/components/shared";
import { describeError } from "@/lib/errors";
import { selectScanner, useRuntimeStore } from "@/stores/runtime-store";
import type { Platform } from "@/types";
import { SettingsSection } from "./settings-layout";

const PLATFORM_NAMES: Record<Platform, string> = {
  macos: "macOS",
  linux: "Linux",
  other: "Other",
};

function Status({ ok, children }: { ok: boolean; children: ReactNode }) {
  const Icon = ok ? Check : ShieldAlert;
  return (
    <span className="inline-flex items-start gap-1.5">
      <Icon
        aria-hidden
        className={ok ? "mt-0.5 size-3.5 shrink-0 text-success" : "mt-0.5 size-3.5 shrink-0 text-warning"}
      />
      <span className="min-w-0">{children}</span>
    </span>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[minmax(7rem,10rem)_minmax(0,1fr)] gap-x-3 py-1.5">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="min-w-0">{children}</dd>
    </div>
  );
}

/** Read-only scanner diagnostics, with an honest account of what limited access means. */
export function SystemAccessSection() {
  const scanner = useRuntimeStore(selectScanner);

  return (
    <SettingsSection
      title="System access"
      description="What the last scan could and couldn't see. Read-only."
    >
      {scanner === null ? (
        <p className="py-2.5 text-muted-foreground">Waiting for the first scan…</p>
      ) : (
        <dl className="divide-y">
          <Fact label="Processes">
            <Status ok={scanner.processAccess.partial + scanner.processAccess.unavailable === 0}>
              <span className="font-mono text-[0.92em] tabular-nums">
                {scanner.processAccess.full} full · {scanner.processAccess.partial} partial ·{" "}
                {scanner.processAccess.unavailable} unavailable
              </span>{" "}
              <span className="text-muted-foreground">of {scanner.processAccess.total}</span>
            </Status>
          </Fact>
          <Fact label="Listening ports">
            {scanner.socketsAvailable && !scanner.socketError ? (
              <Status ok>Readable</Status>
            ) : (
              <Status ok={false}>
                Unavailable
                {scanner.socketError && (
                  <span className="text-muted-foreground"> — {describeError(scanner.socketError).message}</span>
                )}
              </Status>
            )}
          </Fact>
          <Fact label="Skipped sockets">
            <Status ok={scanner.socketsSkipped === 0}>
              <span className="font-mono text-[0.92em] tabular-nums">{scanner.socketsSkipped}</span>
              {scanner.socketsSkipped > 0 && (
                <span className="text-muted-foreground"> — shown on Ports without an owner</span>
              )}
            </Status>
          </Fact>
          <Fact label="Project history">
            {scanner.persistenceError ? (
              <Status ok={false}>
                Can't be saved
                <span className="text-muted-foreground">
                  {" "}
                  — {describeError(scanner.persistenceError).message}
                </span>
              </Status>
            ) : (
              <Status ok>Saved locally</Status>
            )}
          </Fact>
          <Fact label="Platform">{PLATFORM_NAMES[scanner.platform]}</Fact>
          <Fact label="Scanning">
            every <Mono>{scanner.intervalMs / 1000} s</Mono> · last scan took{" "}
            <Mono>{scanner.lastScanMs} ms</Mono>
          </Fact>
        </dl>
      )}
      <p className="py-2.5 text-xs leading-relaxed text-muted-foreground">
        LocalDeck runs as your user and only sees what the OS shows an unprivileged app. Processes
        owned by other users or the system may be described only partly or not at all, and the ports
        they hold appear on Ports without a project. Nothing is ever attributed to a project on a
        guess, so limited access means fewer answers, not wrong ones.
      </p>
    </SettingsSection>
  );
}
