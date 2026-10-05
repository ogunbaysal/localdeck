import { useCallback, useEffect, useState } from "react";
import { toErrorPayload } from "@/lib/errors";
import { listTerminals } from "@/lib/tauri";
import type { ErrorPayload, TerminalInfo } from "@/types";

export interface TerminalsState {
  /** `null` until the first response (or after a failure). */
  terminals: readonly TerminalInfo[] | null;
  error: ErrorPayload | null;
  loading: boolean;
  reload: () => void;
}

/** Known terminal applications from the backend, fetched once on mount and on `reload()`. */
export function useTerminals(): TerminalsState {
  const [terminals, setTerminals] = useState<readonly TerminalInfo[] | null>(null);
  const [error, setError] = useState<ErrorPayload | null>(null);
  const [loading, setLoading] = useState(true);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    listTerminals().then(
      (list) => {
        if (cancelled) return;
        setTerminals(list);
        setError(null);
        setLoading(false);
      },
      (failure: unknown) => {
        if (cancelled) return;
        setTerminals(null);
        setError(toErrorPayload(failure));
        setLoading(false);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [attempt]);

  const reload = useCallback(() => setAttempt((n) => n + 1), []);
  return { terminals, error, loading, reload };
}
