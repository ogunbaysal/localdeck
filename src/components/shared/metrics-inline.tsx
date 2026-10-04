import { formatBytes, formatCpu, formatProcessCount } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { Metrics } from "@/types";

/** `4.2% CPU · 286 MB · 4 processes` on one muted line. */
export function MetricsInline({ metrics, className }: { metrics: Metrics; className?: string }) {
  return (
    <span
      data-slot="metrics-inline"
      className={cn("inline-flex flex-wrap items-center gap-x-1.5 text-xs text-muted-foreground tabular-nums", className)}
    >
      <span>{formatCpu(metrics.cpuPercent)} CPU</span>
      <span aria-hidden>·</span>
      <span>{formatBytes(metrics.memoryBytes)}</span>
      <span aria-hidden>·</span>
      <span>{formatProcessCount(metrics.processCount)}</span>
    </span>
  );
}
