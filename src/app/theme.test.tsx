import { afterEach, describe, expect, test } from "bun:test";
import { act, render } from "@testing-library/react";
import { useRuntimeStore } from "@/stores/runtime-store";
import { makeSettings } from "@/test/fixtures";
import type { Theme } from "@/types";
import { resolveTheme, ThemeProvider } from "./theme";

const originalMatchMedia = window.matchMedia;

/** Controllable `prefers-color-scheme` for happy-dom. */
function mockPrefersDark(initial: boolean) {
  let dark = initial;
  const listeners = new Set<() => void>();
  window.matchMedia = ((query: string) =>
    ({
      media: query,
      get matches() {
        return query.includes("prefers-color-scheme: dark") ? dark : false;
      },
      addEventListener: (_type: string, listener: () => void) => listeners.add(listener),
      removeEventListener: (_type: string, listener: () => void) => listeners.delete(listener),
    }) as unknown as MediaQueryList) as typeof window.matchMedia;
  return {
    set(value: boolean) {
      dark = value;
      act(() => listeners.forEach((listener) => listener()));
    },
  };
}

function setTheme(theme: Theme) {
  act(() => useRuntimeStore.getState().setSettings(makeSettings({ theme })));
}

const isDark = () => document.documentElement.classList.contains("dark");

afterEach(() => {
  window.matchMedia = originalMatchMedia;
});

describe("resolveTheme", () => {
  test("explicit themes ignore the OS; system follows it", () => {
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
  });
});

describe("ThemeProvider", () => {
  test("before settings load it follows the OS", () => {
    mockPrefersDark(true);
    render(<ThemeProvider>x</ThemeProvider>);
    expect(isDark()).toBe(true);
  });

  test("the saved theme replaces the pre-mount guess", () => {
    mockPrefersDark(true);
    render(<ThemeProvider>x</ThemeProvider>);
    setTheme("light");
    expect(isDark()).toBe(false);
    setTheme("dark");
    expect(isDark()).toBe(true);
  });

  test("with `system` it reacts to prefers-color-scheme changes; explicit themes don't", () => {
    const os = mockPrefersDark(false);
    render(<ThemeProvider>x</ThemeProvider>);
    setTheme("system");
    expect(isDark()).toBe(false);

    os.set(true);
    expect(isDark()).toBe(true);
    os.set(false);
    expect(isDark()).toBe(false);

    setTheme("light");
    os.set(true);
    expect(isDark()).toBe(false);
  });
});
