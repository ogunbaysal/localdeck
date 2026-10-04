import { useEffect, useRef, useState } from "react";
import { toErrorPayload } from "@/lib/errors";
import {
  getSettings,
  getSnapshot,
  listenOpenProject,
  listenRuntimeUpdates,
} from "@/lib/tauri";
import { useRuntimeStore } from "@/stores/runtime-store";

/**
 * Startup sequence: subscribe to `runtime://updated` and `ui://open-project`, then load
 * the snapshot and settings. Any failure puts the runtime store into `status: "error"`
 * with the `ErrorPayload`; `retry()` runs the whole sequence again. Listeners are removed
 * on unmount and before every retry.
 */
export function useRuntimeBootstrap(onOpenProject: (projectId: number) => void): {
  retry: () => void;
} {
  const [attempt, setAttempt] = useState(0);
  const openProject = useRef(onOpenProject);
  useEffect(() => {
    openProject.current = onOpenProject;
  }, [onOpenProject]);

  useEffect(() => {
    let disposed = false;
    const cleanups: Array<() => void> = [];
    const track = (unlisten: () => void) => {
      if (disposed) unlisten();
      else cleanups.push(unlisten);
    };

    const store = useRuntimeStore.getState();
    store.setLoading();

    void (async () => {
      try {
        track(await listenRuntimeUpdates((snapshot) => store.setSnapshot(snapshot)));
        track(await listenOpenProject((projectId) => openProject.current(projectId)));
        const [snapshot, settings] = await Promise.all([getSnapshot(), getSettings()]);
        if (disposed) return;
        store.setSnapshot(snapshot);
        store.setSettings(settings);
        store.setReady();
      } catch (error) {
        if (!disposed) store.setError(toErrorPayload(error));
      }
    })();

    return () => {
      disposed = true;
      for (const unlisten of cleanups) unlisten();
    };
  }, [attempt]);

  return { retry: () => setAttempt((value) => value + 1) };
}
