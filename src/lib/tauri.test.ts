import { describe, expect, test } from "bun:test";
import { backend } from "@/test/backend";
import { makeSnapshot } from "@/test/fixtures";
import type { ErrorPayload } from "@/types";
import {
  getSnapshot,
  listenOpenProject,
  listenRuntimeUpdates,
  stopProject,
} from "./tauri";

async function rejection(promise: Promise<unknown>): Promise<unknown> {
  try {
    await promise;
  } catch (error) {
    return error;
  }
  throw new Error("expected the promise to reject");
}

describe("command errors", () => {
  test("backend errors reject with the ErrorPayload itself", async () => {
    const payload: ErrorPayload = {
      code: "PROJECT_NOT_FOUND",
      message: "Project 9 was not found",
      context: { projectId: "9" },
    };
    backend.fail("stop_project", payload);
    expect(await rejection(stopProject(9))).toEqual(payload);
  });

  test("an unknown-command string from Tauri is normalised into an IPC ErrorPayload", async () => {
    const error = await rejection(getSnapshot());
    expect(error).toEqual({
      code: "UNSUPPORTED_OPERATION",
      message: "Command get_snapshot not found",
      context: { origin: "ipc" },
    });
  });

  test("successful commands resolve with the backend value", async () => {
    const snapshot = makeSnapshot({ updatedAt: 5 });
    backend.reply("get_snapshot", snapshot);
    expect(await getSnapshot()).toEqual(snapshot);
  });
});

describe("events", () => {
  test("runtime updates deliver snapshots, drop malformed payloads and stop after unlisten", async () => {
    const received: number[] = [];
    const unlisten = await listenRuntimeUpdates((snapshot) => received.push(snapshot.updatedAt));

    backend.emit("runtime://updated", makeSnapshot({ updatedAt: 1 }));
    backend.emit("runtime://updated", { nonsense: true });
    backend.emit("runtime://updated", null);
    expect(received).toEqual([1]);

    unlisten();
    backend.emit("runtime://updated", makeSnapshot({ updatedAt: 2 }));
    expect(received).toEqual([1]);
    expect(backend.listenerCount("runtime://updated")).toBe(0);
  });

  test("open-project only accepts integer project ids", async () => {
    const opened: number[] = [];
    await listenOpenProject((id) => opened.push(id));
    backend.emit("ui://open-project", 4);
    backend.emit("ui://open-project", "4");
    backend.emit("ui://open-project", 1.5);
    expect(opened).toEqual([4]);
  });
});
