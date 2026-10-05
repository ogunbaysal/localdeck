/**
 * Helpers that build *copyable* shell snippets (for the clipboard only; LocalDeck never
 * runs them). Everything interpolated is POSIX single-quoted, so paths with spaces,
 * quotes or `$` paste safely into bash/zsh/fish.
 */

const SAFE = /^[A-Za-z0-9_@%+=:,./-]+$/;

/** Quote one argument for a POSIX shell. */
export function shellQuote(value: string): string {
  if (value.length > 0 && SAFE.test(value)) return value;
  return `'${value.replace(/'/g, `'\\''`)}'`;
}

/** `cd '/path with spaces'`. */
export function cdCommand(path: string): string {
  return `cd ${shellQuote(path)}`;
}

/** `kill -TERM 123` (graceful) or `kill -KILL 123`. */
export function killCommand(pid: number, force = false): string {
  if (!Number.isInteger(pid) || pid <= 0) throw new RangeError(`invalid pid: ${pid}`);
  return `kill -${force ? "KILL" : "TERM"} ${pid}`;
}

/** What to run in a terminal to see who listens on `port` (macOS and Linux). */
export function lsofPortCommand(port: number): string {
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new RangeError(`invalid port: ${port}`);
  }
  return `lsof -nP -iTCP:${port} -sTCP:LISTEN`;
}

/** `program arg1 'arg 2'` from a program and its arguments. */
export function commandLine(program: string, args: readonly string[]): string {
  return [program, ...args].map(shellQuote).join(" ");
}
