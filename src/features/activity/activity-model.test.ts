import { describe, expect, test } from "bun:test";
import { makeActivity } from "@/test/fixtures";
import type { ActivityKind } from "@/types";
import {
  ACTIVITY_KINDS,
  dayLabel,
  filterActivity,
  groupByDay,
  matchesActivityGroup,
  refetchDelay,
} from "./activity-model";

const at = (y: number, m: number, d: number, h = 12, min = 0) => new Date(y, m - 1, d, h, min).getTime();
const NOW = at(2026, 10, 4, 15);

describe("day grouping", () => {
  test("labels Today, Yesterday and older dates (with the year only when it differs)", () => {
    expect(dayLabel(at(2026, 10, 4, 0, 1), NOW)).toBe("Today");
    expect(dayLabel(at(2026, 10, 3, 23, 59), NOW)).toBe("Yesterday");
    expect(dayLabel(at(2026, 9, 30), NOW)).toBe("Sep 30");
    expect(dayLabel(at(2025, 12, 31), NOW)).toBe("Dec 31, 2025");
  });

  test("yesterday is calendar-based across a month boundary", () => {
    expect(dayLabel(at(2026, 2, 28), at(2026, 3, 1))).toBe("Yesterday");
  });

  test("keeps newest-first order, one group per local day, even for same-day events that aren't adjacent", () => {
    const events = [
      makeActivity({ id: 5, createdAt: at(2026, 10, 4, 14) }),
      makeActivity({ id: 4, createdAt: at(2026, 10, 4, 9) }),
      makeActivity({ id: 3, createdAt: at(2026, 10, 3, 22) }),
      makeActivity({ id: 2, createdAt: at(2026, 10, 3, 1) }),
      makeActivity({ id: 1, createdAt: at(2026, 9, 1) }),
    ];
    const days = groupByDay(events, NOW);
    expect(days.map((day) => [day.label, day.events.map((event) => event.id)])).toEqual([
      ["Today", [5, 4]],
      ["Yesterday", [3, 2]],
      ["Sep 1", [1]],
    ]);
  });

  test("no events, no groups", () => {
    expect(groupByDay([], NOW)).toEqual([]);
  });
});

describe("filters", () => {
  const kinds = Object.keys(ACTIVITY_KINDS) as ActivityKind[];
  const events = kinds.map((kind, index) => makeActivity({ id: index + 1, kind, projectId: index % 2 === 0 ? 1 : 2 }));

  test("every kind belongs to exactly one non-All group", () => {
    for (const kind of kinds) {
      const event = makeActivity({ kind });
      const groups = (["lifecycle", "ports", "problems"] as const).filter((group) => matchesActivityGroup(event, group));
      expect(groups).toEqual([ACTIVITY_KINDS[kind].group]);
      expect(matchesActivityGroup(event, "all")).toBe(true);
    }
  });

  test("groups: lifecycle = start/stop/auto-restart, ports = opened/closed, problems = conflicts, crashes, losses, give-ups", () => {
    const pick = (group: "lifecycle" | "ports" | "problems") =>
      filterActivity(events, { projectId: null, group }).map((event) => event.kind).sort();
    expect(pick("lifecycle")).toEqual(["autoRestarted", "projectStarted", "projectStopped"]);
    expect(pick("ports")).toEqual(["portClosed", "portOpened"]);
    expect(pick("problems")).toEqual(["autoRestartGaveUp", "managedExited", "portConflict", "runtimeLost"]);
  });

  test("project and group filters combine", () => {
    const result = filterActivity(events, { projectId: 2, group: "problems" });
    expect(result.length).toBeGreaterThan(0);
    expect(result.every((event) => event.projectId === 2 && ACTIVITY_KINDS[event.kind].group === "problems")).toBe(true);
  });
});

describe("refetchDelay", () => {
  test("fetches immediately the first time and once the window has passed", () => {
    expect(refetchDelay(null, 1000)).toBe(0);
    expect(refetchDelay(1000, 3000)).toBe(0);
    expect(refetchDelay(1000, 9000)).toBe(0);
  });

  test("waits out the rest of the window, never longer than it", () => {
    expect(refetchDelay(1000, 1500)).toBe(1500);
    expect(refetchDelay(1000, 2900)).toBe(100);
    expect(refetchDelay(5000, 1000)).toBe(2000);
  });
});
