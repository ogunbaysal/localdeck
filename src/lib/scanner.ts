import type { ScannerStatus } from "@/types";

export interface LimitedAccessReason {
  id: "sockets" | "socketsSkipped" | "persistence";
  title: string;
  detail: string;
}

/**
 * Why LocalDeck can only show part of the picture right now. Empty when everything is
 * readable. These are calm notices, not failures: the app keeps working with what it has.
 */
export function getLimitedAccessReasons(scanner: ScannerStatus): LimitedAccessReason[] {
  const reasons: LimitedAccessReason[] = [];

  if (!scanner.socketsAvailable || scanner.socketError) {
    reasons.push({
      id: "sockets",
      title: "Listening ports unavailable",
      detail:
        scanner.socketError?.message ??
        "The OS didn't let LocalDeck read the socket table, so ports can't be shown.",
    });
  }
  if (scanner.socketsSkipped > 0) {
    const count = scanner.socketsSkipped;
    reasons.push({
      id: "socketsSkipped",
      title: "Some ports can't be attributed",
      detail: `${count} socket${count === 1 ? "" : "s"} belong to processes the OS wouldn't describe. They show up on Ports without an owner.`,
    });
  }
  if (scanner.persistenceError) {
    reasons.push({
      id: "persistence",
      title: "Project history can't be saved",
      detail: scanner.persistenceError.message,
    });
  }
  return reasons;
}
