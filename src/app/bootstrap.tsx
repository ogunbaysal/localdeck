import { createContext, useContext, useEffect, type ReactNode } from "react";
import { useNavigate } from "react-router";
import { usePlatform } from "@/hooks/use-platform";
import { useRuntimeBootstrap } from "./use-runtime-bootstrap";

const RetryContext = createContext<(() => void) | null>(null);

/**
 * Loads runtime state from the backend, keeps it subscribed, routes tray clicks
 * (`ui://open-project`) to `/projects/:id` and mirrors the platform onto
 * `<html data-platform>` for platform-specific chrome. Must live inside the Router.
 */
export function RuntimeBootstrap({ children }: { children: ReactNode }) {
  const navigate = useNavigate();
  const { retry } = useRuntimeBootstrap((projectId) => {
    void navigate(`/projects/${projectId}`);
  });

  const platform = usePlatform();
  useEffect(() => {
    document.documentElement.dataset.platform = platform;
  }, [platform]);

  return <RetryContext value={retry}>{children}</RetryContext>;
}

/** Re-run the startup sequence after a fatal load error. */
export function useRetryBootstrap(): () => void {
  const retry = useContext(RetryContext);
  if (!retry) throw new Error("useRetryBootstrap must be used inside <RuntimeBootstrap>");
  return retry;
}
