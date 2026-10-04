import { describe, expect, test } from "bun:test";
import { detectPlatform, isPaletteShortcut, paletteShortcutLabel } from "./platform";

const key = (overrides: Partial<Parameters<typeof isPaletteShortcut>[0]>) => ({
  key: "k",
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  ...overrides,
});

describe("platform helpers", () => {
  test("detectPlatform recognises macOS and Linux webviews", () => {
    expect(detectPlatform("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15")).toBe("macos");
    expect(detectPlatform("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15")).toBe("linux");
    expect(detectPlatform("Mozilla/5.0 (Windows NT 10.0)")).toBe("other");
  });

  test("the palette shortcut is Cmd+K on macOS and Ctrl+K elsewhere, nothing else", () => {
    expect(isPaletteShortcut(key({ metaKey: true }), "macos")).toBe(true);
    expect(isPaletteShortcut(key({ key: "K", metaKey: true }), "macos")).toBe(true);
    expect(isPaletteShortcut(key({ ctrlKey: true }), "macos")).toBe(false);
    expect(isPaletteShortcut(key({ ctrlKey: true }), "linux")).toBe(true);
    expect(isPaletteShortcut(key({ metaKey: true }), "linux")).toBe(false);
    expect(isPaletteShortcut(key({ metaKey: true, shiftKey: true }), "macos")).toBe(false);
    expect(isPaletteShortcut(key({ key: "j", metaKey: true }), "macos")).toBe(false);
    expect(isPaletteShortcut(key({}), "linux")).toBe(false);
  });

  test("the hint matches the platform's shortcut", () => {
    expect(paletteShortcutLabel("macos")).toBe("⌘K");
    expect(paletteShortcutLabel("linux")).toBe("Ctrl+K");
  });
});
