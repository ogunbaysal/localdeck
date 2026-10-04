import { describe, expect, test } from "bun:test";
import type { ErrorCode, ErrorPayload } from "@/types";
import { describeError, isErrorPayload, toErrorPayload } from "./errors";

describe("isErrorPayload", () => {
  test("accepts the backend shape with and without context", () => {
    expect(isErrorPayload({ code: "PROJECT_NOT_FOUND", message: "x" })).toBe(true);
    expect(isErrorPayload({ code: "PROJECT_NOT_FOUND", message: "x", context: { projectId: "4" } })).toBe(true);
  });

  test("rejects everything else", () => {
    expect(isErrorPayload("boom")).toBe(false);
    expect(isErrorPayload(null)).toBe(false);
    expect(isErrorPayload(new Error("boom"))).toBe(false);
    expect(isErrorPayload({ code: "OPEN_FAILED" })).toBe(false);
    expect(isErrorPayload({ code: 7, message: "x" })).toBe(false);
    expect(isErrorPayload({ code: "OPEN_FAILED", message: "x", context: ["a"] })).toBe(false);
  });
});

describe("toErrorPayload", () => {
  test("passes backend payloads through untouched", () => {
    const payload: ErrorPayload = { code: "PATH_NOT_FOUND", message: "gone", context: { path: "/x" } };
    expect(toErrorPayload(payload)).toBe(payload);
  });

  test("wraps strings and Errors as IPC failures keeping their text", () => {
    expect(toErrorPayload("Command get_snapshot not found")).toEqual({
      code: "UNSUPPORTED_OPERATION",
      message: "Command get_snapshot not found",
      context: { origin: "ipc" },
    });
    expect(toErrorPayload(new Error("window.__TAURI_INTERNALS__ is undefined")).message).toBe(
      "window.__TAURI_INTERNALS__ is undefined",
    );
  });

  test("falls back to a generic message for opaque values", () => {
    const payload = toErrorPayload(undefined);
    expect(payload.message.length).toBeGreaterThan(0);
    expect(payload.context?.origin).toBe("ipc");
  });
});

describe("describeError", () => {
  // Exhaustive on purpose: adding an ErrorCode in Rust breaks type-checking here until
  // the code gets its own wording in errors.ts.
  const known: Record<ErrorCode, true> = {
    PROCESS_NOT_FOUND: true,
    PROCESS_PERMISSION_DENIED: true,
    PROJECT_NOT_FOUND: true,
    PORT_NOT_FOUND: true,
    PORT_SCAN_FAILED: true,
    COMMAND_SPAWN_FAILED: true,
    DATABASE_ERROR: true,
    UNSUPPORTED_OPERATION: true,
    INVALID_INPUT: true,
    NO_PRIMARY_COMMAND: true,
    PROJECT_ALREADY_RUNNING: true,
    PROCESSES_STILL_RUNNING: true,
    INTERNAL: true,
    EDITOR_NOT_FOUND: true,
    PATH_NOT_FOUND: true,
    OPEN_FAILED: true,
  };

  test("every backend code has a specific title and keeps the backend message", () => {
    const titles = new Set<string>();
    for (const code of Object.keys(known) as ErrorCode[]) {
      const { title, message } = describeError({ code, message: `msg for ${code}` });
      expect(title).not.toBe("Something went wrong");
      expect(message).toBe(`msg for ${code}`);
      titles.add(title);
    }
    expect(titles.size).toBe(Object.keys(known).length);
  });

  test("permission errors explain the limitation", () => {
    const { title, hint } = describeError({ code: "PROCESS_PERMISSION_DENIED", message: "Not permitted" });
    expect(title).toBe("Permission denied");
    expect(hint).toContain("your user account");
  });

  test("an unknown future code still shows the backend's message", () => {
    const future = { code: "SOMETHING_NEW", message: "new failure" };
    expect(describeError(future)).toEqual({ title: "Something went wrong", message: "new failure", hint: null });
  });

  test("IPC failures are described as an unreachable backend, not as 'not supported'", () => {
    const { title, message } = describeError("Command get_snapshot not found");
    expect(title).toBe("Couldn't reach the LocalDeck backend");
    expect(message).toBe("Command get_snapshot not found");
  });
});
