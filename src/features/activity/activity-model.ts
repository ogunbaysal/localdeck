import {
  AlertTriangle,
  ArrowRightLeft,
  CirclePlay,
  CircleStop,
  OctagonAlert,
  RotateCw,
  Unplug,
  type LucideIcon,
} from "lucide-react";
import type { ActivityGroup } from "@/stores/ui-store";
import type { ActivityEvent, ActivityKind } from "@/types";

export type { ActivityGroup };

/** Filter chips: everything, or one family of events. */
export const ACTIVITY_GROUPS: ReadonlyArray<{ value: ActivityGroup; label: string }> = [
  { value: "all", label: "All" },
  { value: "lifecycle", label: "Lifecycle" },
  { value: "ports", label: "Ports" },
  { value: "problems", label: "Problems" },
];

export const isActivityGroup = (value: string): value is ActivityGroup =>
  ACTIVITY_GROUPS.some((group) => group.value === value);

export type ActivityTone = "success" | "muted" | "info" | "warning" | "danger";

export interface ActivityKindInfo {
  group: Exclude<ActivityGroup, "all">;
  icon: LucideIcon;
  tone: ActivityTone;
  /** Short accessible name for the icon. */
  label: string;
}

export const ACTIVITY_KINDS: Readonly<Record<ActivityKind, ActivityKindInfo>> = {
  projectStarted: { group: "lifecycle", icon: CirclePlay, tone: "success", label: "Started" },
  projectStopped: { group: "lifecycle", icon: CircleStop, tone: "muted", label: "Stopped" },
  autoRestarted: { group: "lifecycle", icon: RotateCw, tone: "info", label: "Auto-restarted" },
  portOpened: { group: "ports", icon: ArrowRightLeft, tone: "success", label: "Port opened" },
  portClosed: { group: "ports", icon: ArrowRightLeft, tone: "muted", label: "Port closed" },
  portConflict: { group: "problems", icon: AlertTriangle, tone: "warning", label: "Port conflict" },
  managedExited: { group: "problems", icon: OctagonAlert, tone: "danger", label: "Crashed" },
  runtimeLost: { group: "problems", icon: Unplug, tone: "warning", label: "Component lost" },
  autoRestartGaveUp: { group: "problems", icon: OctagonAlert, tone: "danger", label: "Auto-restart gave up" },
};

export const TONE_CLASS: Readonly<Record<ActivityTone, string>> = {
  success: "text-success",
  muted: "text-muted-foreground",
  info: "text-foreground",
  warning: "text-warning",
  danger: "text-destructive",
};

export function matchesActivityGroup(event: ActivityEvent, group: ActivityGroup): boolean {
  return group === "all" || ACTIVITY_KINDS[event.kind].group === group;
}

export interface ActivityFilters {
  /** `null` = every project. */
  projectId: number | null;
  group: ActivityGroup;
}

export function filterActivity(events: readonly ActivityEvent[], filters: ActivityFilters): ActivityEvent[] {
  return events.filter(
    (event) =>
      (filters.projectId === null || event.projectId === filters.projectId) &&
      matchesActivityGroup(event, filters.group),
  );
}

export interface ActivityDay {
  /** Local calendar day, `YYYY-MM-DD`. */
  key: string;
  label: string;
  events: ActivityEvent[];
}

const pad = (value: number) => String(value).padStart(2, "0");

function dayKey(date: Date): string {
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** `Today`, `Yesterday`, otherwise `Oct 3` (with the year when it isn't `now`'s year). */
export function dayLabel(epochMs: number, now: number): string {
  const date = new Date(epochMs);
  const today = new Date(now);
  if (dayKey(date) === dayKey(today)) return "Today";
  const yesterday = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1);
  if (dayKey(date) === dayKey(yesterday)) return "Yesterday";
  const base = `${MONTHS[date.getMonth()]} ${date.getDate()}`;
  return date.getFullYear() === today.getFullYear() ? base : `${base}, ${date.getFullYear()}`;
}

/** Group an already newest-first list into local-time days, preserving order. */
export function groupByDay(events: readonly ActivityEvent[], now: number): ActivityDay[] {
  const days: ActivityDay[] = [];
  for (const event of events) {
    const key = dayKey(new Date(event.createdAt));
    const last = days[days.length - 1];
    if (last?.key === key) last.events.push(event);
    else days.push({ key, label: dayLabel(event.createdAt, now), events: [event] });
  }
  return days;
}

/** Minimum gap between two history fetches triggered by snapshot ticks. */
export const ACTIVITY_REFETCH_MIN_MS = 2000;

/** How long to wait before the next fetch may start (0 = now). `lastStartedAt` null = never fetched. */
export function refetchDelay(lastStartedAt: number | null, now: number, minGapMs = ACTIVITY_REFETCH_MIN_MS): number {
  if (lastStartedAt === null) return 0;
  return Math.min(minGapMs, Math.max(0, lastStartedAt + minGapMs - now));
}
