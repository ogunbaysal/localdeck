import { describe, expect, test } from "bun:test";
import { cdCommand, commandLine, killCommand, lsofPortCommand, shellQuote } from "./shell-quote";

describe("shellQuote", () => {
  test("leaves simple words alone and quotes everything else", () => {
    expect(shellQuote("bun")).toBe("bun");
    expect(shellQuote("/Users/me/code/app")).toBe("/Users/me/code/app");
    expect(shellQuote("")).toBe("''");
    expect(shellQuote("my app")).toBe("'my app'");
    expect(shellQuote("$HOME;rm -rf /")).toBe("'$HOME;rm -rf /'");
    expect(shellQuote("it's")).toBe(`'it'\\''s'`);
    expect(shellQuote("Anılarım 日本語")).toBe("'Anılarım 日本語'");
  });
});

describe("snippets", () => {
  test("cd quotes paths with spaces", () => {
    expect(cdCommand("/Users/me/My Projects/app")).toBe("cd '/Users/me/My Projects/app'");
  });
  test("kill is graceful by default and KILL only when asked", () => {
    expect(killCommand(42)).toBe("kill -TERM 42");
    expect(killCommand(42, true)).toBe("kill -KILL 42");
    expect(() => killCommand(0)).toThrow();
    expect(() => killCommand(1.5)).toThrow();
  });
  test("lsof helper validates the port", () => {
    expect(lsofPortCommand(3000)).toBe("lsof -nP -iTCP:3000 -sTCP:LISTEN");
    expect(() => lsofPortCommand(0)).toThrow();
    expect(() => lsofPortCommand(70000)).toThrow();
  });
  test("commandLine quotes each argument", () => {
    expect(commandLine("bun", ["run", "dev server"])).toBe("bun run 'dev server'");
  });
});
