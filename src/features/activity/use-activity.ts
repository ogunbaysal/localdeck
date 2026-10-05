import { useCallback, useEffect, useRef, useState } from "react";
import { toErrorPayload } from "@/lib/errors";
import { listActivity } from "@/lib/tauri";
import { useRuntimeStore } from "@/stores/runtime-store";
import type { ActivityEvent, ErrorPayload } from "@/types";
import { refetchDelay } from "./activity-model";

export interface ActivityState {
  /** Newest first; `null` until the first response. Kept (stale) when a refresh fails. */
  events: readonly ActivityEvent[] | null;
  error: ErrorPayload | null;
  loading: boolean;
  /** Fetch right now (retry / after clearing). */
  reload: () => void;
}

/**
 * The retained activity history. Loads on mount and again whenever the snapshot's
 * `updatedAt` changes, at most once per `ACTIVITY_REFETCH_MIN_MS` (a trailing fetch
 * picks up changes that arrive inside the window).
 */
export function useActivity(): ActivityState {
  const updatedAt = useRuntimeStore((state) => state.snapshot?.updatedAt ?? null);
  const [state, setState] = useState<Omit<ActivityState, "reload">>({
    events: null,
    error: null,
    loading: true,
  });
  const lastStartedAt = useRef<number | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const generation = useRef(0);

  const load = useCallback(() => {
    clearTimeout(timer.current);
    lastStartedAt.current = Date.now();
    const mine = ++generation.current;
    listActivity().then(
      (events) => {
        if (mine === generation.current) setState({ events, error: null, loading: false });
      },
      (failure: unknown) => {
        if (mine === generation.current) {
          setState((previous) => ({ events: previous.events, error: toErrorPayload(failure), loading: false }));
        }
      },
    );
  }, []);

  useEffect(() => {
    clearTimeout(timer.current);
    const delay = refetchDelay(lastStartedAt.current, Date.now());
    if (delay === 0) load();
    else timer.current = setTimeout(load, delay);
  }, [updatedAt, load]);

  useEffect(
    () => () => {
      clearTimeout(timer.current);
      generation.current++;
      lastStartedAt.current = null;
    },
    [],
  );

  const reload = useCallback(() => {
    setState((previous) => (previous.events === null ? { ...previous, loading: true } : previous));
    load();
  }, [load]);

  return { ...state, reload };
}
