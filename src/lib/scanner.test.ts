import { describe, expect, test } from "bun:test";
import { makeScanner } from "@/test/fixtures";
import { getLimitedAccessReasons } from "./scanner";

describe("getLimitedAccessReasons", () => {
  test("a healthy scanner has no reasons", () => {
    expect(getLimitedAccessReasons(makeScanner())).toEqual([]);
  });

  test("unreadable sockets surface the backend's own error", () => {
    const reasons = getLimitedAccessReasons(
      makeScanner({
        socketsAvailable: false,
        socketError: { code: "PORT_SCAN_FAILED", message: "Socket discovery failed: EPERM" },
      }),
    );
    expect(reasons).toHaveLength(1);
    expect(reasons[0]).toMatchObject({ id: "sockets", detail: "Socket discovery failed: EPERM" });
  });

  test("unavailable sockets without an error still explain themselves", () => {
    const [reason] = getLimitedAccessReasons(makeScanner({ socketsAvailable: false }));
    expect(reason.id).toBe("sockets");
    expect(reason.detail.length).toBeGreaterThan(0);
  });

  test("skipped sockets are counted and pluralised", () => {
    expect(getLimitedAccessReasons(makeScanner({ socketsSkipped: 1 }))[0].detail).toContain("1 socket belong");
    expect(getLimitedAccessReasons(makeScanner({ socketsSkipped: 3 }))[0].detail).toContain("3 sockets belong");
  });

  test("persistence failures are reported alongside others", () => {
    const reasons = getLimitedAccessReasons(
      makeScanner({
        socketsSkipped: 2,
        persistenceError: { code: "DATABASE_ERROR", message: "Database error: disk full" },
      }),
    );
    expect(reasons.map((reason) => reason.id)).toEqual(["socketsSkipped", "persistence"]);
  });

  test("partial process access alone is not a notice (normal for system processes)", () => {
    const scanner = makeScanner({ processAccess: { total: 300, full: 120, partial: 40, unavailable: 140 } });
    expect(getLimitedAccessReasons(scanner)).toEqual([]);
  });
});
