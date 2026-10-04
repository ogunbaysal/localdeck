import { describe, expect, test } from "bun:test";
import {
  ellipsizeMiddle,
  formatBytes,
  formatCpu,
  formatDateTime,
  formatDuration,
  formatPortList,
  formatProcessCount,
  formatRelative,
  formatTime,
  formatUptime,
  humanPath,
} from "./format";

describe("formatBytes", () => {
  test("scales through binary units with one decimal below 10", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(286 * 1024 * 1024)).toBe("286 MB");
    expect(formatBytes(1.2 * 1024 ** 3)).toBe("1.2 GB");
  });

  test("promotes a value that rounds up to 1024 instead of printing 1024 KB", () => {
    expect(formatBytes(1023.6 * 1024)).toBe("1 MB");
  });

  test("invalid input renders a dash", () => {
    expect(formatBytes(-1)).toBe("—");
    expect(formatBytes(Number.NaN)).toBe("—");
  });
});

describe("formatCpu", () => {
  test("one decimal below 100, whole numbers above, 0% for idle", () => {
    expect(formatCpu(4.23)).toBe("4.2%");
    expect(formatCpu(0)).toBe("0%");
    expect(formatCpu(0.04)).toBe("0%");
    expect(formatCpu(130.4)).toBe("130%");
  });

  test("invalid input renders a dash", () => {
    expect(formatCpu(-3)).toBe("—");
  });
});

describe("formatDuration / formatUptime", () => {
  test("picks the two most useful units", () => {
    expect(formatDuration(400)).toBe("<1s");
    expect(formatDuration(45_000)).toBe("45s");
    expect(formatDuration(192_000)).toBe("3m 12s");
    expect(formatDuration((2 * 3600 + 5 * 60) * 1000)).toBe("2h 05m");
    expect(formatDuration((3 * 24 + 4) * 3600 * 1000)).toBe("3d 4h");
  });

  test("uptime is measured from startedAt, never negative, dash when unknown", () => {
    expect(formatUptime(1_000_000, 1_000_000 + 125_000)).toBe("2m 05s");
    expect(formatUptime(2_000_000, 1_000_000)).toBe("<1s");
    expect(formatUptime(null)).toBe("—");
  });
});

describe("time formatting (local time)", () => {
  const at = (day: number, hour: number, minute: number) =>
    new Date(2026, 9, day, hour, minute, 30).getTime();

  test("formatTime is zero-padded 24h HH:MM", () => {
    expect(formatTime(at(4, 18, 4))).toBe("18:04");
    expect(formatTime(at(4, 9, 5))).toBe("09:05");
  });

  test("formatDateTime shows the date only when it isn't today", () => {
    const now = at(4, 20, 0);
    expect(formatDateTime(at(4, 18, 4), now)).toBe("18:04");
    expect(formatDateTime(at(3, 18, 4), now)).toBe("Oct 3, 18:04");
  });

  test("formatRelative buckets seconds, minutes, hours, days", () => {
    const now = 10_000_000_000;
    expect(formatRelative(now - 3_000, now)).toBe("just now");
    expect(formatRelative(now + 5_000, now)).toBe("just now");
    expect(formatRelative(now - 30_000, now)).toBe("30s ago");
    expect(formatRelative(now - 5 * 60_000, now)).toBe("5m ago");
    expect(formatRelative(now - 3 * 3_600_000, now)).toBe("3h ago");
    expect(formatRelative(now - 2 * 86_400_000, now)).toBe("2d ago");
  });
});

describe("paths", () => {
  test("humanPath only substitutes ~ for a known home directory, on a segment boundary", () => {
    expect(humanPath("/Users/ogun/dev/app")).toBe("/Users/ogun/dev/app");
    expect(humanPath("/Users/ogun/dev/app", { homeDir: "/Users/ogun" })).toBe("~/dev/app");
    expect(humanPath("/Users/ogun", { homeDir: "/Users/ogun/" })).toBe("~");
    expect(humanPath("/Users/ogunx/app", { homeDir: "/Users/ogun" })).toBe("/Users/ogunx/app");
  });

  test("ellipsizeMiddle keeps both ends and respects the limit", () => {
    const path = "/Users/ogun/Documents/ogunsweek/localdeck";
    const short = ellipsizeMiddle(path, 24);
    expect(short).toHaveLength(24);
    expect(short).toContain("…");
    expect(short.startsWith("/Users")).toBe(true);
    expect(short.endsWith("localdeck")).toBe(true);
    expect(ellipsizeMiddle("short", 24)).toBe("short");
  });

  test("humanPath applies maxLength after the ~ substitution", () => {
    const text = humanPath("/Users/ogun/Documents/ogunsweek/localdeck", {
      homeDir: "/Users/ogun",
      maxLength: 20,
    });
    expect(text).toHaveLength(20);
    expect(text.startsWith("~/")).toBe(true);
  });
});

describe("ports and counts", () => {
  test("formatPortList joins, truncates with +N and handles empty", () => {
    expect(formatPortList([1420, 3000])).toBe(":1420 · :3000");
    expect(formatPortList([1, 2, 3, 4], 2)).toBe(":1 · :2 +2");
    expect(formatPortList([])).toBe("—");
  });

  test("formatProcessCount pluralises", () => {
    expect(formatProcessCount(1)).toBe("1 process");
    expect(formatProcessCount(0)).toBe("0 processes");
    expect(formatProcessCount(4)).toBe("4 processes");
  });
});
