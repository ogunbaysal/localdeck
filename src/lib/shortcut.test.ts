import { describe, expect, test } from "bun:test";
import { eventToAccelerator, formatAccelerator, type ShortcutEvent } from "./shortcut";

function press(key: string, init: Partial<ShortcutEvent> = {}): ShortcutEvent {
  return {
    key,
    code: "",
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    ...init,
  };
}

describe("eventToAccelerator", () => {
  test("⌘ on macOS and Ctrl elsewhere both normalise to CommandOrControl", () => {
    const mac = press("k", { code: "KeyK", metaKey: true, shiftKey: true });
    expect(eventToAccelerator(mac, "macos")).toBe("CommandOrControl+Shift+K");
    const linux = press("K", { code: "KeyK", ctrlKey: true, shiftKey: true });
    expect(eventToAccelerator(linux, "linux")).toBe("CommandOrControl+Shift+K");
    expect(eventToAccelerator(linux, "other")).toBe("CommandOrControl+Shift+K");
  });

  test("the other of ⌘/Ctrl stays distinct: Control on macOS, Super elsewhere", () => {
    expect(
      eventToAccelerator(press("k", { code: "KeyK", metaKey: true, ctrlKey: true }), "macos"),
    ).toBe("CommandOrControl+Control+K");
    expect(eventToAccelerator(press("k", { code: "KeyK", ctrlKey: true }), "macos")).toBe("Control+K");
    expect(
      eventToAccelerator(press("k", { code: "KeyK", ctrlKey: true, metaKey: true }), "linux"),
    ).toBe("CommandOrControl+Super+K");
    expect(eventToAccelerator(press("k", { code: "KeyK", metaKey: true }), "linux")).toBe("Super+K");
  });

  test("modifiers come out in a fixed order regardless of how they were pressed", () => {
    const event = press("p", { code: "KeyP", shiftKey: true, altKey: true, ctrlKey: true });
    expect(eventToAccelerator(event, "linux")).toBe("CommandOrControl+Alt+Shift+P");
  });

  test("a bare key is not a shortcut: at least one modifier is required", () => {
    expect(eventToAccelerator(press("k", { code: "KeyK" }), "macos")).toBeNull();
    expect(eventToAccelerator(press("F5", { code: "F5" }), "linux")).toBeNull();
  });

  test("pressing only modifiers yields nothing yet", () => {
    expect(eventToAccelerator(press("Meta", { code: "MetaLeft", metaKey: true }), "macos")).toBeNull();
    expect(eventToAccelerator(press("Shift", { code: "ShiftLeft", shiftKey: true }), "linux")).toBeNull();
    expect(
      eventToAccelerator(press("Control", { code: "ControlLeft", ctrlKey: true, altKey: true }), "linux"),
    ).toBeNull();
  });

  test("Escape is reserved for cancelling", () => {
    expect(eventToAccelerator(press("Escape", { code: "Escape", ctrlKey: true }), "linux")).toBeNull();
  });

  test("uses the physical key so Option+letter on macOS isn't recorded as a symbol", () => {
    const event = press("π", { code: "KeyP", altKey: true, metaKey: true });
    expect(eventToAccelerator(event, "macos")).toBe("CommandOrControl+Alt+P");
  });

  test("digits, function keys, arrows, space and punctuation map to Tauri key names", () => {
    const ctrl = { ctrlKey: true };
    expect(eventToAccelerator(press("1", { ...ctrl, code: "Digit1" }), "linux")).toBe("CommandOrControl+1");
    expect(eventToAccelerator(press("F12", { ...ctrl, code: "F12" }), "linux")).toBe("CommandOrControl+F12");
    expect(eventToAccelerator(press("ArrowUp", { ...ctrl, code: "ArrowUp" }), "linux")).toBe("CommandOrControl+Up");
    expect(eventToAccelerator(press(" ", { ...ctrl, code: "Space" }), "linux")).toBe("CommandOrControl+Space");
    expect(eventToAccelerator(press(",", { ...ctrl, code: "Comma" }), "linux")).toBe("CommandOrControl+,");
    expect(eventToAccelerator(press("/", { ...ctrl, code: "Slash" }), "linux")).toBe("CommandOrControl+/");
  });

  test("falls back to event.key when the event carries no code", () => {
    expect(eventToAccelerator(press("j", { metaKey: true }), "macos")).toBe("CommandOrControl+J");
    expect(eventToAccelerator(press("ArrowLeft", { altKey: true }), "linux")).toBe("Alt+Left");
  });

  test("a dead key still resolves through its physical code", () => {
    expect(eventToAccelerator(press("Dead", { code: "KeyE", ctrlKey: true }), "linux")).toBe("CommandOrControl+E");
  });

  test("keys Tauri has no name for are rejected", () => {
    expect(eventToAccelerator(press("ContextMenu", { ctrlKey: true }), "linux")).toBeNull();
    expect(eventToAccelerator(press("Unidentified", { ctrlKey: true }), "linux")).toBeNull();
  });
});

describe("formatAccelerator", () => {
  test("macOS shows modifier symbols without separators", () => {
    expect(formatAccelerator("CommandOrControl+Shift+K", "macos")).toBe("⌘⇧K");
    expect(formatAccelerator("CommandOrControl+Control+Alt+Space", "macos")).toBe("⌘⌃⌥Space");
    expect(formatAccelerator("Alt+Up", "macos")).toBe("⌥↑");
  });

  test("other platforms spell modifiers out", () => {
    expect(formatAccelerator("CommandOrControl+Shift+K", "linux")).toBe("Ctrl+Shift+K");
    expect(formatAccelerator("CommandOrControl+Super+Alt+Left", "other")).toBe("Ctrl+Super+Alt+←");
  });

  test("tokens it doesn't know pass through", () => {
    expect(formatAccelerator("Hyper+K", "linux")).toBe("Hyper+K");
  });

  test("round-trips what eventToAccelerator produces", () => {
    const accelerator = eventToAccelerator(press("k", { code: "KeyK", metaKey: true, altKey: true }), "macos");
    expect(accelerator && formatAccelerator(accelerator, "macos")).toBe("⌘⌥K");
  });
});
