/**
 * Global-shortcut helpers. The backend stores a Tauri accelerator string such as
 * `CommandOrControl+Shift+K`; these functions turn a `keydown` into one and render one
 * for people (`⌘⇧K` on macOS, `Ctrl+Shift+K` elsewhere). Pure, no DOM access.
 */
import type { Platform } from "@/types";

export type ShortcutEvent = Pick<
  KeyboardEvent,
  "key" | "code" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey"
>;

/** Modifier tokens in canonical (output) order. */
const MODIFIER_ORDER = ["CommandOrControl", "Control", "Alt", "Shift", "Super"] as const;
type Modifier = (typeof MODIFIER_ORDER)[number];

const MODIFIER_KEYS: readonly string[] = ["Control", "Shift", "Alt", "Meta", "AltGraph", "OS", "Super", "Hyper", "Fn"];

/** True for keys that are only modifiers (pressing them alone never completes a shortcut). */
export function isModifierKey(key: string): boolean {
  return MODIFIER_KEYS.includes(key);
}

/** `KeyboardEvent.code` → accelerator key for the non-alphanumeric keys Tauri understands. */
const NAMED_CODES: Readonly<Record<string, string>> = {
  Space: "Space",
  Tab: "Tab",
  Enter: "Enter",
  NumpadEnter: "Enter",
  Backspace: "Backspace",
  Delete: "Delete",
  Insert: "Insert",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  Minus: "-",
  Equal: "=",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  BracketLeft: "[",
  BracketRight: "]",
};

/** Same, from `KeyboardEvent.key` when `code` is missing (synthetic events, odd layouts). */
const NAMED_KEYS: Readonly<Record<string, string>> = {
  " ": "Space",
  Tab: "Tab",
  Enter: "Enter",
  Backspace: "Backspace",
  Delete: "Delete",
  Insert: "Insert",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  "-": "-",
  "=": "=",
  ",": ",",
  ".": ".",
  "/": "/",
  "\\": "\\",
  ";": ";",
  "'": "'",
  "`": "`",
  "[": "[",
  "]": "]",
};

/** The accelerator key for an event, or `null` when it has none Tauri can register. */
function acceleratorKey(event: ShortcutEvent): string | null {
  const { code, key } = event;
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1];
  const digit = /^Digit([0-9])$/.exec(code);
  if (digit) return digit[1];
  if (/^F([1-9]|1\d|2[0-4])$/.test(code)) return code;
  const named = NAMED_CODES[code];
  if (named) return named;

  // No usable `code`: fall back to the produced character / key name.
  if (/^[a-zA-Z]$/.test(key)) return key.toUpperCase();
  if (/^[0-9]$/.test(key)) return key;
  if (/^F([1-9]|1\d|2[0-4])$/.test(key)) return key;
  return NAMED_KEYS[key] ?? null;
}

/**
 * Accelerator for a `keydown`, or `null` while the combination isn't registrable:
 * only modifiers so far, no modifier at all, Escape (reserved for cancelling), or a key
 * Tauri has no name for. `⌘`/`Ctrl` map to `CommandOrControl` (macOS ⌘ = Meta, elsewhere
 * Ctrl); the other of the two stays distinct (`Control` on macOS, `Super` elsewhere).
 */
export function eventToAccelerator(event: ShortcutEvent, platform: Platform): string | null {
  if (isModifierKey(event.key) || event.key === "Escape") return null;
  const key = acceleratorKey(event);
  if (key === null) return null;

  const mac = platform === "macos";
  const modifiers = new Set<Modifier>();
  if (mac ? event.metaKey : event.ctrlKey) modifiers.add("CommandOrControl");
  if (mac && event.ctrlKey) modifiers.add("Control");
  if (event.altKey) modifiers.add("Alt");
  if (event.shiftKey) modifiers.add("Shift");
  if (!mac && event.metaKey) modifiers.add("Super");
  if (modifiers.size === 0) return null;

  return [...MODIFIER_ORDER.filter((modifier) => modifiers.has(modifier)), key].join("+");
}

const MAC_SYMBOLS: Readonly<Record<string, string>> = {
  CommandOrControl: "⌘",
  CmdOrCtrl: "⌘",
  Command: "⌘",
  Cmd: "⌘",
  Super: "⌘",
  Meta: "⌘",
  Control: "⌃",
  Ctrl: "⌃",
  Alt: "⌥",
  Option: "⌥",
  Shift: "⇧",
};

const OTHER_NAMES: Readonly<Record<string, string>> = {
  CommandOrControl: "Ctrl",
  CmdOrCtrl: "Ctrl",
  Control: "Ctrl",
  Ctrl: "Ctrl",
  Command: "Super",
  Cmd: "Super",
  Meta: "Super",
  Super: "Super",
  Option: "Alt",
};

const ARROWS: Readonly<Record<string, string>> = { Up: "↑", Down: "↓", Left: "←", Right: "→" };

/** `⌘⇧K` on macOS, `Ctrl+Shift+K` elsewhere. Unknown tokens pass through unchanged. */
export function formatAccelerator(accelerator: string, platform: Platform): string {
  const tokens = accelerator.split("+").filter((token) => token.length > 0);
  if (platform === "macos") {
    return tokens.map((token) => MAC_SYMBOLS[token] ?? ARROWS[token] ?? token).join("");
  }
  return tokens.map((token) => OTHER_NAMES[token] ?? ARROWS[token] ?? token).join("+");
}
