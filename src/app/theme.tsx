import { useLayoutEffect, type ReactNode } from "react";
import { useMediaQuery } from "@/hooks/use-media-query";
import { useRuntimeStore } from "@/stores/runtime-store";
import type { Theme } from "@/types";

export type ResolvedTheme = "light" | "dark";

/** `system` follows the OS; explicit choices win. */
export function resolveTheme(theme: Theme, prefersDark: boolean): ResolvedTheme {
  if (theme === "system") return prefersDark ? "dark" : "light";
  return theme;
}

/** Effective theme from `settings.theme` (default `system` until settings load) and the OS preference. */
export function useResolvedTheme(): ResolvedTheme {
  const theme = useRuntimeStore((state) => state.settings?.theme ?? "system");
  const prefersDark = useMediaQuery("(prefers-color-scheme: dark)");
  return resolveTheme(theme, prefersDark);
}

/** Keeps the `dark` class on `<html>` in sync, replacing the pre-mount script's guess. */
export function ThemeProvider({ children }: { children: ReactNode }) {
  const resolved = useResolvedTheme();
  useLayoutEffect(() => {
    document.documentElement.classList.toggle("dark", resolved === "dark");
  }, [resolved]);
  return children;
}
