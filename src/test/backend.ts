/**
 * Fake Tauri backend for tests. `setup.ts` mocks `@tauri-apps/api/core` and
 * `@tauri-apps/api/event` with this object, so the real `@/lib/tauri` wrappers run
 * against it. Unregistered commands reject the way Tauri does for unknown commands
 * (a plain string). Everything is reset after each test.
 *
 *   backend.reply("get_snapshot", makeSnapshot());
 *   backend.fail("stop_project", { code: "PROCESS_PERMISSION_DENIED", message: "…" });
 *   backend.emit("runtime://updated", makeSnapshot());
 *   expect(backend.callsTo("stop_project")).toEqual([{ projectId: 1 }]);
 */
import type { ErrorPayload } from "@/types";

type Args = Record<string, unknown> | undefined;
type Handler = (args: Args) => unknown;
type Listener = (event: { event: string; id: number; payload: unknown }) => void;

export interface RecordedCall {
  command: string;
  args: Args;
}

class FakeBackend {
  private handlers = new Map<string, Handler>();
  private listeners = new Map<string, Set<Listener>>();
  private nextEventId = 0;
  readonly calls: RecordedCall[] = [];

  /** Register a handler; a thrown/rejected value becomes the invoke rejection. */
  handle(command: string, handler: Handler): void {
    this.handlers.set(command, handler);
  }

  reply(command: string, value: unknown): void {
    this.handlers.set(command, () => value);
  }

  fail(command: string, error: ErrorPayload | string): void {
    this.handlers.set(command, () => {
      throw error;
    });
  }

  /** Args of every call made to `command`, in order. */
  callsTo(command: string): Args[] {
    return this.calls.filter((call) => call.command === command).map((call) => call.args);
  }

  /** Deliver an event to every active listener. */
  emit(event: string, payload: unknown): void {
    for (const listener of [...(this.listeners.get(event) ?? [])]) {
      listener({ event, id: ++this.nextEventId, payload });
    }
  }

  listenerCount(event: string): number {
    return this.listeners.get(event)?.size ?? 0;
  }

  reset(): void {
    this.handlers.clear();
    this.listeners.clear();
    this.calls.length = 0;
  }

  /** Stand-in for `invoke` from `@tauri-apps/api/core`. */
  invoke = async (command: string, args?: Args): Promise<unknown> => {
    this.calls.push({ command, args });
    const handler = this.handlers.get(command);
    if (!handler) throw `Command ${command} not found`;
    return handler(args);
  };

  /** Stand-in for `listen` from `@tauri-apps/api/event`. */
  listen = async (event: string, listener: Listener): Promise<() => void> => {
    let set = this.listeners.get(event);
    if (!set) {
      set = new Set();
      this.listeners.set(event, set);
    }
    set.add(listener);
    return () => {
      set.delete(listener);
    };
  };
}

export const backend = new FakeBackend();
