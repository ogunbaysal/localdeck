import type { AccessLevel, ComponentKind, PrimaryUrlSource, ProcessOrigin, RootKind } from "@/types";

/** Every component kind the user may pick when correcting detection, in display order. */
export const COMPONENT_KINDS: readonly { kind: ComponentKind; label: string }[] = [
  { kind: "web", label: "Web" },
  { kind: "api", label: "API" },
  { kind: "worker", label: "Worker" },
  { kind: "database", label: "Database" },
  { kind: "cache", label: "Cache" },
  { kind: "queue", label: "Queue" },
  { kind: "bundler", label: "Bundler" },
  { kind: "watcher", label: "Watcher" },
  { kind: "desktop", label: "Desktop" },
  { kind: "cli", label: "CLI" },
  { kind: "runtime", label: "Runtime" },
  { kind: "other", label: "Other" },
];

const KIND_LABELS = new Map(COMPONENT_KINDS.map(({ kind, label }) => [kind, label]));

export const kindLabel = (kind: ComponentKind): string => KIND_LABELS.get(kind) ?? kind;

export const ROOT_KIND_LABEL: Record<RootKind, string> = {
  git: "Git root",
  workspace: "Workspace root",
  manifest: "Manifest root",
};

export const URL_SOURCE_LABEL: Record<PrimaryUrlSource, string> = {
  override: "Set by you",
  webComponent: "From the web component",
  devServer: "From the dev server",
  confirmedHttp: "Confirmed HTTP endpoint",
};

export const ORIGIN_LABEL: Record<ProcessOrigin, string> = {
  external: "External",
  localDeckManaged: "Managed by LocalDeck",
};

/** Compact form for dense tables (the full text goes in a tooltip). */
export const ORIGIN_SHORT_LABEL: Record<ProcessOrigin, string> = {
  external: "External",
  localDeckManaged: "Managed",
};

export const ACCESS_LABEL: Record<AccessLevel, string> = {
  full: "Full",
  partial: "Partial",
  unavailable: "Unavailable",
};

export const ACCESS_HINT: Record<AccessLevel, string> = {
  full: "LocalDeck can read everything about this process.",
  partial: "The OS limited access to this process, so some details are missing.",
  unavailable: "The OS hid this process's details; LocalDeck only knows it exists.",
};

/** Shown wherever missing metadata may be mistaken for a bug. */
export const LIMITED_ACCESS_HINT =
  "A dash means the OS didn't allow LocalDeck to read that detail (limited access).";
