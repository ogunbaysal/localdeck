import type { Platform } from "@/types";

/** Best-effort platform from a user-agent string; the scanner's `platform` replaces it once known. */
export function detectPlatform(userAgent: string): Platform {
  if (/Macintosh|Mac OS X/i.test(userAgent)) return "macos";
  if (/Linux|X11/i.test(userAgent)) return "linux";
  return "other";
}

/** `⌘K` on macOS, `Ctrl+K` elsewhere. */
export function paletteShortcutLabel(platform: Platform): string {
  return platform === "macos" ? "⌘K" : "Ctrl+K";
}

type KeyEventLike = Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">;

/** True for ⌘K (macOS) / Ctrl+K (elsewhere) with no other modifiers. */
export function isPaletteShortcut(event: KeyEventLike, platform: Platform): boolean {
  if (event.altKey || event.shiftKey) return false;
  const modifier = platform === "macos" ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
  return modifier && event.key.toLowerCase() === "k";
}
