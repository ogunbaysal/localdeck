import { TriangleAlert } from "lucide-react";
import { cn } from "@/lib/utils";
import type { RuntimeStatus } from "@/types";

const STATUS_STYLE: Record<RuntimeStatus, { label: string; dot: string; text: string }> = {
  running: { label: "Running", dot: "bg-success", text: "text-success" },
  partial: { label: "Partial", dot: "bg-warning", text: "text-warning" },
  stopped: {
    label: "Stopped",
    dot: "border border-muted-foreground/60",
    text: "text-muted-foreground",
  },
};

/** Small status dot: filled for running/partial, hollow ring for stopped. */
export function StatusDot({
  status,
  pulse = false,
  className,
}: {
  status: RuntimeStatus;
  pulse?: boolean;
  className?: string;
}) {
  return (
    <span
      aria-hidden
      className={cn(
        "inline-block size-2 shrink-0 rounded-full",
        STATUS_STYLE[status].dot,
        pulse && "animate-pulse",
        className,
      )}
    />
  );
}

/**
 * Runtime status as dot + label. `stopPending` reads "Stopping…" (processes still alive
 * after a graceful stop). `attention` adds an amber marker; it is independent of status.
 */
export function StatusBadge({
  status,
  stopPending = false,
  attention = false,
  className,
}: {
  status: RuntimeStatus;
  stopPending?: boolean;
  attention?: boolean;
  className?: string;
}) {
  const style = STATUS_STYLE[status];
  return (
    <span
      data-slot="status-badge"
      data-status={status}
      className={cn("inline-flex items-center gap-1.5 text-xs font-medium", style.text, className)}
    >
      <StatusDot status={status} pulse={stopPending} />
      {stopPending ? "Stopping…" : style.label}
      {attention && (
        <TriangleAlert
          role="img"
          aria-label="Needs attention"
          className="size-3.5 text-warning"
        />
      )}
    </span>
  );
}
