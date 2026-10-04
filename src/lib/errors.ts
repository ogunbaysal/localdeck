import type { ErrorPayload } from "@/types";

/** `context.origin` value marking failures of the IPC layer itself (not a backend `AppError`). */
export const IPC_ERROR_ORIGIN = "ipc";

/** User-facing summary of an {@link ErrorPayload}, shared by toasts and `ErrorState`. */
export interface ErrorDescription {
  /** Short, code-specific headline. */
  title: string;
  /** The backend's own message (or the IPC failure text). */
  message: string;
  /** Optional one-line suggestion for what to do next. */
  hint: string | null;
}

/** True for objects shaped like the backend's `{ code, message, context? }`. */
export function isErrorPayload(value: unknown): value is ErrorPayload {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Record<string, unknown>;
  if (typeof candidate.code !== "string" || typeof candidate.message !== "string") {
    return false;
  }
  const { context } = candidate;
  return (
    context === undefined ||
    (typeof context === "object" && context !== null && !Array.isArray(context))
  );
}

/**
 * Normalise anything thrown by `invoke`/`listen` into an `ErrorPayload`.
 * Backend errors pass through untouched. Anything else (string rejections such as
 * "command not found", JS errors, running outside Tauri) becomes
 * `UNSUPPORTED_OPERATION` tagged with `context.origin === "ipc"`.
 */
export function toErrorPayload(error: unknown): ErrorPayload {
  if (isErrorPayload(error)) return error;
  let message: string;
  if (typeof error === "string" && error.length > 0) message = error;
  else if (error instanceof Error && error.message.length > 0) message = error.message;
  else message = "Unexpected error talking to the LocalDeck backend";
  return {
    code: "UNSUPPORTED_OPERATION",
    message,
    context: { origin: IPC_ERROR_ORIGIN },
  };
}

const TITLES: Record<string, { title: string; hint: string | null }> = {
  PROCESS_NOT_FOUND: {
    title: "Process already exited",
    hint: "It may have stopped on its own. The list refreshes automatically.",
  },
  PROCESS_PERMISSION_DENIED: {
    title: "Permission denied",
    hint: "LocalDeck can only signal processes owned by your user account.",
  },
  PROJECT_NOT_FOUND: {
    title: "Project not found",
    hint: "It may have been forgotten. Go back to Projects and try again.",
  },
  PORT_NOT_FOUND: {
    title: "Port not found",
    hint: "The port may have closed in the meantime.",
  },
  PORT_SCAN_FAILED: {
    title: "Couldn't read listening ports",
    hint: "Port information may be incomplete until the OS allows socket access.",
  },
  COMMAND_SPAWN_FAILED: {
    title: "Couldn't start the command",
    hint: "Check the project's start command and that its tools are installed.",
  },
  DATABASE_ERROR: {
    title: "Local storage error",
    hint: "LocalDeck couldn't read or write its database, so changes may not be saved.",
  },
  UNSUPPORTED_OPERATION: { title: "Not supported", hint: null },
  INVALID_INPUT: { title: "Invalid input", hint: null },
  NO_PRIMARY_COMMAND: {
    title: "No start command",
    hint: "Choose a package.json script in the project settings.",
  },
  PROJECT_ALREADY_RUNNING: { title: "Already running", hint: null },
  PROCESSES_STILL_RUNNING: {
    title: "Processes are still running",
    hint: "They ignored the graceful stop. Use Force Stop if you're sure.",
  },
  EDITOR_NOT_FOUND: {
    title: "Editor not found",
    hint: "Pick an installed editor in Settings.",
  },
  PATH_NOT_FOUND: {
    title: "Path not found",
    hint: "The folder may have been moved or deleted.",
  },
  OPEN_FAILED: {
    title: "Couldn't open it",
    hint: "The system couldn't find an app to open this. Check your default apps.",
  },
  INTERNAL: {
    title: "Internal error",
    hint: "Something unexpected happened inside LocalDeck. Try again.",
  },
};

const UNKNOWN_TITLE = { title: "Something went wrong", hint: null };

/** Turn any thrown value into a titled, human-readable description. */
export function describeError(error: unknown): ErrorDescription {
  const payload = toErrorPayload(error);
  if (payload.context?.origin === IPC_ERROR_ORIGIN) {
    return {
      title: "Couldn't reach the LocalDeck backend",
      message: payload.message,
      hint: "Try again. If it keeps failing, restart LocalDeck.",
    };
  }
  const entry = TITLES[payload.code] ?? UNKNOWN_TITLE;
  return { title: entry.title, message: payload.message, hint: entry.hint };
}
