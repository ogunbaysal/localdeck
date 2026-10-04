import { useCallback, useEffect, useState } from "react";
import { toErrorPayload } from "@/lib/errors";
import { listEditors } from "@/lib/tauri";
import type { EditorInfo, ErrorPayload } from "@/types";

export interface EditorsState {
  /** `null` until the first response (or after a failure). */
  editors: readonly EditorInfo[] | null;
  error: ErrorPayload | null;
  loading: boolean;
  reload: () => void;
}

/** Installed-editor list from the backend, fetched once on mount and on `reload()`. */
export function useEditors(): EditorsState {
  const [editors, setEditors] = useState<readonly EditorInfo[] | null>(null);
  const [error, setError] = useState<ErrorPayload | null>(null);
  const [loading, setLoading] = useState(true);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    listEditors().then(
      (list) => {
        if (cancelled) return;
        setEditors(list);
        setError(null);
        setLoading(false);
      },
      (failure: unknown) => {
        if (cancelled) return;
        setEditors(null);
        setError(toErrorPayload(failure));
        setLoading(false);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [attempt]);

  const reload = useCallback(() => setAttempt((n) => n + 1), []);
  return { editors, error, loading, reload };
}
