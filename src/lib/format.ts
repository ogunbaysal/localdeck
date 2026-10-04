/** Display formatting helpers. Pure, locale-independent and unit tested. */

const DASH = "—";
const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/** `286 MB`, `1.2 GB`, `512 B`. Binary (1024) steps; one decimal below 10. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return DASH;
  let value = bytes;
  let unit = 0;
  while (Math.round(value) >= 1024 && unit < BYTE_UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  if (unit === 0) return `${Math.round(value)} B`;
  const text = value < 10 ? value.toFixed(1) : String(Math.round(value));
  // `9.96` rounds to "10.0": drop the pointless decimal.
  return `${text.endsWith(".0") ? text.slice(0, -2) : text} ${BYTE_UNITS[unit]}`;
}

/** `4.2%`, `0%`, `130%`. CPU is percent of one core and may exceed 100. */
export function formatCpu(percent: number): string {
  if (!Number.isFinite(percent) || percent < 0) return DASH;
  if (percent < 0.05) return "0%";
  if (percent >= 100) return `${Math.round(percent)}%`;
  return `${percent.toFixed(1)}%`;
}

/** Compact duration: `<1s`, `45s`, `3m 12s`, `2h 05m`, `3d 4h`. */
export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return DASH;
  const totalSeconds = Math.floor(ms / 1000);
  if (totalSeconds < 1) return "<1s";
  if (totalSeconds < 60) return `${totalSeconds}s`;
  const minutes = Math.floor(totalSeconds / 60);
  if (minutes < 60) return `${minutes}m ${String(totalSeconds % 60).padStart(2, "0")}s`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ${String(minutes % 60).padStart(2, "0")}m`;
  return `${Math.floor(hours / 24)}d ${hours % 24}h`;
}

/** Time since `startedAt` (epoch ms); `—` when the OS didn't report a start time. */
export function formatUptime(startedAt: number | null, now: number = Date.now()): string {
  if (startedAt === null) return DASH;
  return formatDuration(Math.max(0, now - startedAt));
}

/** Local wall-clock `HH:MM` (24h), matching the activity list in the UX spec. */
export function formatTime(epochMs: number): string {
  if (!Number.isFinite(epochMs)) return DASH;
  const date = new Date(epochMs);
  const hh = String(date.getHours()).padStart(2, "0");
  const mm = String(date.getMinutes()).padStart(2, "0");
  return `${hh}:${mm}`;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** `18:04` for today, `Oct 3, 18:04` for earlier days (relative to `now`). */
export function formatDateTime(epochMs: number, now: number = Date.now()): string {
  if (!Number.isFinite(epochMs)) return DASH;
  const date = new Date(epochMs);
  const today = new Date(now);
  const sameDay =
    date.getFullYear() === today.getFullYear() &&
    date.getMonth() === today.getMonth() &&
    date.getDate() === today.getDate();
  return sameDay
    ? formatTime(epochMs)
    : `${MONTHS[date.getMonth()]} ${date.getDate()}, ${formatTime(epochMs)}`;
}

/** `just now`, `5m ago`, `2h ago`, `3d ago`. Future timestamps read as `just now`. */
export function formatRelative(epochMs: number, now: number = Date.now()): string {
  if (!Number.isFinite(epochMs)) return DASH;
  const seconds = Math.floor((now - epochMs) / 1000);
  if (seconds < 10) return "just now";
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

/**
 * Shorten `text` to at most `maxLength` characters by replacing the middle with `…`.
 * Biased towards the tail (a path's last segments matter more than its first).
 */
export function ellipsizeMiddle(text: string, maxLength: number): string {
  if (maxLength < 1 || text.length <= maxLength) return text;
  if (maxLength === 1) return "…";
  const keep = maxLength - 1;
  const tail = Math.ceil(keep * 0.6);
  const head = keep - tail;
  return `${text.slice(0, head)}…${text.slice(text.length - tail)}`;
}

/**
 * Display form of a filesystem path. LocalDeck doesn't know the user's home directory,
 * so `~` is only substituted when the caller passes `homeDir`; never guessed.
 */
export function humanPath(
  path: string,
  options: { homeDir?: string | null; maxLength?: number } = {},
): string {
  const { homeDir, maxLength } = options;
  let display = path;
  if (homeDir) {
    const home = homeDir.endsWith("/") && homeDir.length > 1 ? homeDir.slice(0, -1) : homeDir;
    if (path === home) display = "~";
    else if (path.startsWith(`${home}/`)) display = `~${path.slice(home.length)}`;
  }
  return maxLength === undefined ? display : ellipsizeMiddle(display, maxLength);
}

/** `:1420`. */
export function formatPort(port: number): string {
  return `:${port}`;
}

/** `:1420 · :3000`, or `:1420 · :3000 +2` when `max` truncates. Empty list → `—`. */
export function formatPortList(ports: readonly number[], max?: number): string {
  if (ports.length === 0) return DASH;
  const shown = max === undefined ? ports : ports.slice(0, Math.max(1, max));
  const text = shown.map(formatPort).join(" · ");
  const hidden = ports.length - shown.length;
  return hidden > 0 ? `${text} +${hidden}` : text;
}

/** `1 process` / `4 processes`. */
export function formatProcessCount(count: number): string {
  return `${count} ${count === 1 ? "process" : "processes"}`;
}
