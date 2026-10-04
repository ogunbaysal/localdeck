import { toast } from "sonner";
import { describeError } from "./errors";
import type { ErrorPayload, StopReport } from "@/types";

const SUCCESS_TOAST_MS = 2500;

export interface RunActionOptions {
  /** Success toast text; defaults to `label`. `false` suppresses it (caller reports itself). */
  success?: string | false;
}

/**
 * Run a backend call and report the outcome. Success shows a short toast; any failure
 * shows a code-specific error toast that includes the backend message. Returns the
 * result, or `undefined` when the call failed. Failures are never swallowed silently.
 */
export async function runAction<T>(
  label: string,
  fn: () => Promise<T>,
  options: RunActionOptions = {},
): Promise<T | undefined> {
  try {
    const result = await fn();
    if (options.success !== false) {
      toast.success(options.success ?? label, { duration: SUCCESS_TOAST_MS });
    }
    return result;
  } catch (error) {
    const { title, message } = describeError(error);
    toast.error(title, { description: `${label}: ${message}` });
    return undefined;
  }
}

/**
 * Report a `StopReport`: graceful stop only *signals* processes, and some may have
 * failed (permissions, already gone). `verb` reads like "Stop requested" / "Force stopped".
 */
export function announceStopReport(projectName: string, report: StopReport, verb: string): void {
  const { signaled, failed } = report;
  if (failed.length === 0) {
    toast.success(`${verb} for ${projectName}`, { duration: SUCCESS_TOAST_MS });
    return;
  }
  const first = describeError(failed[0].error);
  const detail = `PID ${failed[0].pid}: ${first.message}`;
  if (signaled.length > 0) {
    toast.warning(`${verb} for ${projectName}, ${failed.length} process(es) failed`, {
      description: detail,
    });
  } else {
    toast.error(first.title, { description: `${verb} failed for ${projectName}. ${detail}` });
  }
}

/** Copy text via the async Clipboard API, falling back to `execCommand` in older webviews. */
export async function copyToClipboard(text: string): Promise<void> {
  if (typeof navigator !== "undefined" && navigator.clipboard) {
    try {
      await navigator.clipboard.writeText(text);
      return;
    } catch {
      // Fall through to the legacy path (permission denied / unfocused document).
    }
  }
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.select();
  const copied = document.execCommand("copy");
  area.remove();
  if (!copied) {
    const failure: ErrorPayload = {
      code: "UNSUPPORTED_OPERATION",
      message: "Copying to the clipboard isn't available in this window.",
    };
    throw failure;
  }
}
