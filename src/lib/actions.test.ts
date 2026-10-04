import { afterEach, describe, expect, spyOn, test } from "bun:test";
import { toast } from "sonner";
import { announceStopReport, runAction } from "./actions";

afterEach(() => {
  for (const method of ["success", "error", "warning"] as const) spyOn(toast, method).mockRestore();
});

describe("runAction", () => {
  test("returns the result and shows a short success toast named after the label", async () => {
    const success = spyOn(toast, "success").mockImplementation(() => "id");
    const result = await runAction("Open folder", async () => 42);
    expect(result).toBe(42);
    expect(success).toHaveBeenCalledTimes(1);
    expect(success.mock.calls[0]?.[0]).toBe("Open folder");
  });

  test("a custom success message replaces the label, and false suppresses the toast", async () => {
    const success = spyOn(toast, "success").mockImplementation(() => "id");
    await runAction("Start", async () => undefined, { success: "Starting LocalDeck" });
    expect(success.mock.calls[0]?.[0]).toBe("Starting LocalDeck");
    success.mockClear();
    await runAction("Stop", async () => undefined, { success: false });
    expect(success).not.toHaveBeenCalled();
  });

  test("a backend error becomes an error toast with title, label and message; result is undefined", async () => {
    const error = spyOn(toast, "error").mockImplementation(() => "id");
    const success = spyOn(toast, "success").mockImplementation(() => "id");
    const result = await runAction("Force kill", async () => {
      throw { code: "PROCESS_PERMISSION_DENIED", message: "Not permitted to signal process 321" };
    });
    expect(result).toBeUndefined();
    expect(success).not.toHaveBeenCalled();
    const [title, options] = error.mock.calls[0] as [string, { description: string }];
    expect(title).toBe("Permission denied");
    expect(options.description).toBe("Force kill: Not permitted to signal process 321");
  });

  test("non-payload failures are still reported rather than swallowed", async () => {
    const error = spyOn(toast, "error").mockImplementation(() => "id");
    await runAction("Open URL", async () => {
      throw new Error("webview exploded");
    });
    const [title, options] = error.mock.calls[0] as [string, { description: string }];
    expect(title).toBe("Couldn't reach the LocalDeck backend");
    expect(options.description).toContain("webview exploded");
  });
});

describe("announceStopReport", () => {
  const permission = {
    pid: 11,
    error: { code: "PROCESS_PERMISSION_DENIED", message: "Not permitted to signal process 11" },
  } as const;

  test("success when every process was signalled", () => {
    const success = spyOn(toast, "success").mockImplementation(() => "id");
    announceStopReport("LocalDeck", { signaled: [1, 2], failed: [] }, "Stop requested");
    expect(success.mock.calls[0]?.[0]).toBe("Stop requested for LocalDeck");
  });

  test("warns when only some processes could be signalled", () => {
    const warning = spyOn(toast, "warning").mockImplementation(() => "id");
    announceStopReport("LocalDeck", { signaled: [1], failed: [permission] }, "Stop requested");
    const [title, options] = warning.mock.calls[0] as [string, { description: string }];
    expect(title).toContain("1 process(es) failed");
    expect(options.description).toContain("PID 11");
  });

  test("errors when nothing could be signalled", () => {
    const error = spyOn(toast, "error").mockImplementation(() => "id");
    const success = spyOn(toast, "success").mockImplementation(() => "id");
    announceStopReport("LocalDeck", { signaled: [], failed: [permission] }, "Stop requested");
    expect(success).not.toHaveBeenCalled();
    expect(error.mock.calls[0]?.[0]).toBe("Permission denied");
  });
});
